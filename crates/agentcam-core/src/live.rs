//! What agents are doing right now, and how full their usage windows are.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::index::Result;
use crate::query::{SESSION_COLS, SessionRow, session_row};
use crate::util::{format_ts_ms, parse_ts_ms};

/// A step is "recent" for this long; after that a session is quiet.
const WORKING_MS: i64 = 2 * 60_000;
/// A tool call with no step after it may be a long command still running.
const RUNNING_MS: i64 = 10 * 60_000;

#[derive(Debug, Clone, Serialize)]
pub struct LiveSession {
    pub session: SessionRow,
    /// Latest step in the session or any of its subagents.
    pub last_at: String,
    pub idle_ms: i64,
    /// `working`, `waiting` (the agent replied and the turn looks over),
    /// or `quiet`.
    pub state: &'static str,
    /// Context size of the latest model call.
    pub context_tokens: Option<i64>,
    pub active_subagents: i64,
    /// Latest steps across the session and its subagents, newest first.
    pub recent: Vec<LiveStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveStep {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub kind: String,
    pub category: Option<String>,
    pub tool_name: Option<String>,
    pub title: String,
    pub is_error: bool,
}

/// Top-level sessions with a step in the last `within_ms`, newest first.
pub fn live(conn: &Connection, now_ms: i64, within_ms: i64) -> Result<Vec<LiveSession>> {
    let cutoff = format_ts_ms(now_ms - within_ms);
    let sql = format!(
        "SELECT {SESSION_COLS}, m.last FROM (
            SELECT s.id, max(coalesce(s.ended_at, ''),
                coalesce((SELECT max(c.ended_at) FROM sessions c WHERE c.parent_id = s.id), '')) AS last
            FROM sessions s WHERE s.parent_id IS NULL
         ) m JOIN sessions s ON s.id = m.id
         WHERE m.last >= ?1 ORDER BY m.last DESC LIMIT 40"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<(SessionRow, String)> = stmt
        .query_map([&cutoff], |r| Ok((session_row(r)?, r.get(30)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let working_cutoff = format_ts_ms(now_ms - WORKING_MS);
    let mut ctx = conn.prepare_cached(
        "SELECT context_tokens FROM events WHERE session_id = ? AND context_tokens IS NOT NULL
         ORDER BY seq DESC LIMIT 1",
    )?;
    let mut subs =
        conn.prepare_cached("SELECT count(*) FROM sessions WHERE parent_id = ? AND ended_at >= ?")?;
    let mut recent = conn.prepare_cached(
        "SELECT e.session_id, e.seq, e.ts, e.kind, e.category, e.tool_name, e.title, e.is_error
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE (s.id = ?1 OR s.parent_id = ?1) AND e.ts IS NOT NULL
         ORDER BY e.ts DESC, e.seq DESC LIMIT 8",
    )?;

    let mut out = Vec::with_capacity(rows.len());
    for (session, last_at) in rows {
        let idle_ms = parse_ts_ms(&last_at).map_or(within_ms, |t| (now_ms - t).max(0));
        let context_tokens = ctx
            .query_row([&session.id], |r| r.get(0))
            .optional()?
            .flatten();
        let active_subagents: i64 =
            subs.query_row(params![session.id, working_cutoff], |r| r.get(0))?;
        let steps: Vec<LiveStep> = recent
            .query_map([&session.id], |r| {
                Ok(LiveStep {
                    session_id: r.get(0)?,
                    seq: r.get(1)?,
                    ts: r.get(2)?,
                    kind: r.get(3)?,
                    category: r.get(4)?,
                    tool_name: r.get(5)?,
                    title: r.get(6)?,
                    is_error: r.get(7)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        let state = state_of(
            steps.first().map(|s| s.kind.as_str()),
            idle_ms,
            active_subagents,
        );
        out.push(LiveSession {
            session,
            last_at,
            idle_ms,
            state,
            context_tokens,
            active_subagents,
            recent: steps,
        });
    }
    Ok(out)
}

fn state_of(last_kind: Option<&str>, idle_ms: i64, subagents: i64) -> &'static str {
    match last_kind {
        _ if subagents > 0 => "working",
        Some("response") if idle_ms <= RUNNING_MS => "waiting",
        Some("tool_call") if idle_ms <= RUNNING_MS => "working",
        _ if idle_ms <= WORKING_MS => "working",
        _ => "quiet",
    }
}

/// The latest reading of one usage window.
#[derive(Debug, Clone, Serialize)]
pub struct LimitWindow {
    pub agent: String,
    pub window: String,
    pub window_minutes: Option<i64>,
    pub used_percent: f64,
    pub resets_at: Option<String>,
    /// When this reading was taken.
    pub at: String,
    pub plan: Option<String>,
    pub session_id: String,
    /// The window has reset since the reading, so it's out of date.
    pub expired: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LimitPoint {
    pub ts: String,
    pub used_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LimitSeries {
    pub agent: String,
    pub window: String,
    pub points: Vec<LimitPoint>,
}

/// A time an agent reported reaching a limit.
#[derive(Debug, Clone, Serialize)]
pub struct LimitHit {
    pub agent: String,
    pub window: String,
    pub ts: String,
    pub resets_at: Option<String>,
    pub resets_text: Option<String>,
    pub session_id: String,
    pub seq: Option<i64>,
    pub title: Option<String>,
    pub project_root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Limits {
    pub windows: Vec<LimitWindow>,
    /// Readings over the last 7 days, per window, oldest first.
    pub series: Vec<LimitSeries>,
    /// Limits reached in the last 90 days, newest first.
    pub hits: Vec<LimitHit>,
    pub hits_30d: i64,
}

pub fn limits(conn: &Connection, now_ms: i64) -> Result<Limits> {
    let now = format_ts_ms(now_ms);
    let mut stmt = conn.prepare(
        "SELECT agent, window, window_minutes, used_percent, resets_at, ts, plan, session_id
         FROM limits l
         WHERE used_percent IS NOT NULL AND ts = (
            SELECT max(ts) FROM limits m
            WHERE m.agent = l.agent AND m.window = l.window AND m.used_percent IS NOT NULL)
         GROUP BY agent, window
         ORDER BY agent, coalesce(window_minutes, 0)",
    )?;
    let windows = stmt
        .query_map([], |r| {
            let resets_at: Option<String> = r.get(4)?;
            Ok(LimitWindow {
                agent: r.get(0)?,
                window: r.get(1)?,
                window_minutes: r.get(2)?,
                used_percent: r.get(3)?,
                expired: resets_at.as_deref().is_some_and(|t| t < now.as_str()),
                resets_at,
                at: r.get(5)?,
                plan: r.get(6)?,
                session_id: r.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let since = format_ts_ms(now_ms - 7 * 86_400_000);
    let mut stmt = conn.prepare(
        "SELECT agent, window, ts, used_percent FROM limits
         WHERE used_percent IS NOT NULL AND ts >= ? ORDER BY agent, window, ts",
    )?;
    let mut series: Vec<LimitSeries> = Vec::new();
    let mut rows = stmt.query([&since])?;
    while let Some(r) = rows.next()? {
        let (agent, window): (String, String) = (r.get(0)?, r.get(1)?);
        let point = LimitPoint {
            ts: r.get(2)?,
            used_percent: r.get(3)?,
        };
        match series.last_mut() {
            Some(s) if s.agent == agent && s.window == window => s.points.push(point),
            _ => series.push(LimitSeries {
                agent,
                window,
                points: vec![point],
            }),
        }
    }

    let since = format_ts_ms(now_ms - 90 * 86_400_000);
    let mut stmt = conn.prepare(
        "SELECT l.agent, l.window, l.ts, l.resets_at, l.resets_text, l.session_id, l.seq,
                coalesce(p.title, s.title), s.project_root
         FROM limits l JOIN sessions s ON s.id = l.session_id
         LEFT JOIN sessions p ON p.id = s.parent_id
         WHERE l.hit = 1 AND l.ts >= ? ORDER BY l.ts DESC LIMIT 200",
    )?;
    let mut hits: Vec<LimitHit> = Vec::new();
    for h in stmt.query_map([&since], |r| {
        Ok(LimitHit {
            agent: r.get(0)?,
            window: r.get(1)?,
            ts: r.get(2)?,
            resets_at: r.get(3)?,
            resets_text: r.get(4)?,
            session_id: r.get(5)?,
            seq: r.get(6)?,
            title: r.get(7)?,
            project_root: r.get(8)?,
        })
    })? {
        let h = h?;
        // One window reached once: readings repeat until it resets, and
        // Claude Code writes the notice again on each retry.
        let dup = hits.iter().any(|o| {
            o.agent == h.agent
                && o.window == h.window
                && match (&o.resets_at, &h.resets_at) {
                    (Some(a), Some(b)) => a == b,
                    _ => {
                        o.resets_text == h.resets_text
                            && parse_ts_ms(&o.ts)
                                .zip(parse_ts_ms(&h.ts))
                                .is_some_and(|(a, b)| (a - b).abs() < 6 * 3_600_000)
                    }
                }
        });
        if !dup {
            hits.push(h);
        }
    }
    let month = format_ts_ms(now_ms - 30 * 86_400_000);
    let hits_30d = hits.iter().filter(|h| h.ts >= month).count() as i64;
    hits.truncate(30);
    Ok(Limits {
        windows,
        series,
        hits,
        hits_30d,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_follow_the_last_step() {
        assert_eq!(state_of(Some("response"), 30_000, 0), "waiting");
        assert_eq!(state_of(Some("tool_call"), 5 * 60_000, 0), "working");
        assert_eq!(state_of(Some("thinking"), 30_000, 0), "working");
        assert_eq!(state_of(Some("thinking"), 5 * 60_000, 0), "quiet");
        assert_eq!(state_of(Some("response"), 20 * 60_000, 0), "quiet");
        assert_eq!(state_of(Some("response"), 20 * 60_000, 2), "working");
    }
}
