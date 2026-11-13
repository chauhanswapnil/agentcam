//! Turns (a prompt you typed and the work until the next one), approvals,
//! thinking, pull requests and the skills and apps behind steps.

use std::collections::BTreeMap;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::index::Result;
use crate::query::{Filter, conditions};
use crate::util::parse_ts_ms;

/// One turn with what happened in it.
#[derive(Debug, Clone, Serialize)]
pub struct TurnRow {
    pub n: i64,
    pub seq: i64,
    pub end_seq: i64,
    /// First line of the prompt.
    pub prompt: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    /// From the prompt to the turn's last step (or Codex's own end).
    pub duration_ms: Option<i64>,
    /// Time between the previous turn's end and this prompt.
    pub wait_ms: Option<i64>,
    pub steps: i64,
    pub tool_calls: i64,
    pub errors: i64,
    pub cost_usd: Option<f64>,
    pub thinking_ms: i64,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub mode: Option<String>,
    pub approval_policy: Option<String>,
    pub sandbox: Option<String>,
}

const TURN_COLS: &str = "t.n, t.seq, t.end_seq,
    (SELECT title FROM events p WHERE p.session_id = t.session_id AND p.seq = t.seq),
    t.started_at,
    coalesce(t.ended_at, (SELECT max(x.ts) FROM events x
        WHERE x.session_id = t.session_id AND x.seq BETWEEN t.seq AND t.end_seq)),
    (SELECT count(*) FROM events x WHERE x.session_id = t.session_id AND x.seq BETWEEN t.seq AND t.end_seq),
    (SELECT count(*) FROM events x WHERE x.session_id = t.session_id AND x.seq BETWEEN t.seq AND t.end_seq
        AND x.kind = 'tool_call'),
    (SELECT coalesce(sum(x.is_error), 0) FROM events x
        WHERE x.session_id = t.session_id AND x.seq BETWEEN t.seq AND t.end_seq),
    (SELECT sum(x.cost_usd) FROM events x WHERE x.session_id = t.session_id AND x.seq BETWEEN t.seq AND t.end_seq),
    (SELECT coalesce(sum(x.duration_ms), 0) FROM events x
        WHERE x.session_id = t.session_id AND x.seq BETWEEN t.seq AND t.end_seq AND x.kind = 'thinking'),
    t.model, t.effort, t.mode, t.approval_policy, t.sandbox";

fn turn_row(r: &rusqlite::Row) -> rusqlite::Result<TurnRow> {
    let started_at: Option<String> = r.get(4)?;
    let ended_at: Option<String> = r.get(5)?;
    let ms = |t: &Option<String>| t.as_deref().and_then(parse_ts_ms);
    Ok(TurnRow {
        n: r.get(0)?,
        seq: r.get(1)?,
        end_seq: r.get(2)?,
        prompt: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
        duration_ms: ms(&started_at)
            .zip(ms(&ended_at))
            .map(|(a, b)| (b - a).max(0)),
        started_at,
        ended_at,
        wait_ms: None,
        steps: r.get(6)?,
        tool_calls: r.get(7)?,
        errors: r.get(8)?,
        cost_usd: r.get(9)?,
        thinking_ms: r.get(10)?,
        model: r.get(11)?,
        effort: r.get(12)?,
        mode: r.get(13)?,
        approval_policy: r.get(14)?,
        sandbox: r.get(15)?,
    })
}

/// Fill in how long you took to reply before each turn.
fn with_waits(mut rows: Vec<TurnRow>) -> Vec<TurnRow> {
    for i in 1..rows.len() {
        let prev = rows[i - 1].ended_at.as_deref().and_then(parse_ts_ms);
        let start = rows[i].started_at.as_deref().and_then(parse_ts_ms);
        rows[i].wait_ms = prev.zip(start).map(|(a, b)| (b - a).max(0));
    }
    rows
}

pub fn session_turns(conn: &Connection, id: &str) -> Result<Vec<TurnRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TURN_COLS} FROM turns t WHERE t.session_id = ? ORDER BY t.n"
    ))?;
    let rows = stmt
        .query_map([id], turn_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(with_waits(rows))
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TurnGroup {
    /// Model, effort or mode; `null` when the log didn't say.
    pub key: Option<String>,
    pub turns: i64,
    pub median_ms: i64,
    pub median_steps: i64,
    pub avg_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongTurn {
    pub session_id: String,
    pub agent: String,
    pub title: Option<String>,
    pub turn: TurnRow,
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnStats {
    pub turns: i64,
    pub median_ms: i64,
    pub p90_ms: i64,
    pub median_steps: i64,
    /// Typical time between a turn ending and your next prompt.
    pub median_wait_ms: Option<i64>,
    /// Turn counts by length: under 30 s, 30 s–2 min, 2–10 min, 10–30 min, longer.
    pub lengths: [i64; 5],
    pub by_model: Vec<TurnGroup>,
    pub by_effort: Vec<TurnGroup>,
    pub by_mode: Vec<TurnGroup>,
    pub longest: Vec<LongTurn>,
}

fn quantile(sorted: &[i64], q: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn group(rows: &[&TurnRow], key: impl Fn(&TurnRow) -> Option<String>) -> Vec<TurnGroup> {
    let mut by: BTreeMap<Option<String>, Vec<&TurnRow>> = BTreeMap::new();
    for r in rows {
        by.entry(key(r)).or_default().push(r);
    }
    let mut out: Vec<TurnGroup> = by
        .into_iter()
        .map(|(key, rs)| {
            let mut d: Vec<i64> = rs.iter().filter_map(|r| r.duration_ms).collect();
            d.sort_unstable();
            let mut st: Vec<i64> = rs.iter().map(|r| r.steps).collect();
            st.sort_unstable();
            let costs: Vec<f64> = rs.iter().filter_map(|r| r.cost_usd).collect();
            TurnGroup {
                key,
                turns: rs.len() as i64,
                median_ms: quantile(&d, 0.5),
                median_steps: quantile(&st, 0.5),
                avg_cost_usd: (!costs.is_empty())
                    .then(|| costs.iter().sum::<f64>() / costs.len() as f64),
            }
        })
        .collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.turns));
    out
}

/// A turn with its session id, agent and session title.
type LoadedTurn = (String, String, Option<String>, TurnRow);

/// The bucket a turn's length falls in, for `TurnStats::lengths`: under 30 s,
/// 30 s to 2 min, 2 to 10 min, 10 to 30 min, over 30 min.
pub(crate) fn length_bucket(ms: i64) -> usize {
    match ms {
        m if m < 30_000 => 0,
        m if m < 120_000 => 1,
        m if m < 600_000 => 2,
        m if m < 1_800_000 => 3,
        _ => 4,
    }
}

/// Turns of top-level sessions whose prompt matches `f`, with waits filled
/// in: (session id, agent, session title, turn), by session then turn.
fn load_turns(conn: &Connection, f: &Filter) -> Result<Vec<LoadedTurn>> {
    let (cond, args) = conditions(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT {TURN_COLS}, t.session_id, s.agent, s.title
         FROM turns t JOIN events e ON e.session_id = t.session_id AND e.seq = t.seq
         JOIN sessions s ON s.id = t.session_id
         WHERE s.parent_id IS NULL AND {cond}
         ORDER BY t.session_id, t.n"
    ))?;
    let mut all: Vec<LoadedTurn> = Vec::new();
    let mut rows = stmt.query(params_from_iter(&args))?;
    while let Some(r) = rows.next()? {
        all.push((r.get(16)?, r.get(17)?, r.get(18)?, turn_row(r)?));
    }
    // Waits are within a session.
    let mut i = 0;
    while i < all.len() {
        let mut j = i;
        while j < all.len() && all[j].0 == all[i].0 {
            j += 1;
        }
        let turns: Vec<TurnRow> = all[i..j].iter().map(|x| x.3.clone()).collect();
        for (k, t) in with_waits(turns).into_iter().enumerate() {
            all[i + k].3 = t;
        }
        i = j;
    }
    Ok(all)
}

/// The turns in one length bucket (0 to 4, see `length_bucket`), longest
/// first: (session id, session title, turn).
pub fn turns_in_bucket(
    conn: &Connection,
    f: &Filter,
    bucket: usize,
) -> Result<Vec<(String, Option<String>, TurnRow)>> {
    let mut rows: Vec<_> = load_turns(conn, f)?
        .into_iter()
        .filter(|x| x.3.duration_ms.is_some_and(|d| length_bucket(d) == bucket))
        .map(|x| (x.0, x.2, x.3))
        .collect();
    rows.sort_by_key(|x| std::cmp::Reverse(x.2.duration_ms.unwrap_or(0)));
    Ok(rows)
}

/// Turns of top-level sessions whose prompt matches `f`.
pub fn turn_stats(conn: &Connection, f: &Filter) -> Result<TurnStats> {
    let all = load_turns(conn, f)?;
    let turns: Vec<&TurnRow> = all.iter().map(|x| &x.3).collect();
    let mut d: Vec<i64> = turns.iter().filter_map(|t| t.duration_ms).collect();
    d.sort_unstable();
    let mut steps: Vec<i64> = turns.iter().map(|t| t.steps).collect();
    steps.sort_unstable();
    // Gaps over 8 hours are a night's sleep, not a reply.
    let mut waits: Vec<i64> = turns
        .iter()
        .filter_map(|t| t.wait_ms)
        .filter(|w| *w < 8 * 3_600_000)
        .collect();
    waits.sort_unstable();
    let mut lengths = [0i64; 5];
    for ms in &d {
        lengths[length_bucket(*ms)] += 1;
    }
    let mut longest: Vec<&(String, String, Option<String>, TurnRow)> = all.iter().collect();
    longest.sort_by_key(|x| std::cmp::Reverse(x.3.duration_ms.unwrap_or(0)));
    Ok(TurnStats {
        turns: turns.len() as i64,
        median_ms: quantile(&d, 0.5),
        p90_ms: quantile(&d, 0.9),
        median_steps: quantile(&steps, 0.5),
        median_wait_ms: (!waits.is_empty()).then(|| quantile(&waits, 0.5)),
        lengths,
        by_model: group(&turns, |t| t.model.clone()),
        by_effort: group(&turns, |t| t.effort.clone()),
        by_mode: group(&turns, |t| t.mode.clone()),
        longest: longest
            .into_iter()
            .take(10)
            .map(|x| LongTurn {
                session_id: x.0.clone(),
                agent: x.1.clone(),
                title: x.2.clone(),
                turn: x.3.clone(),
            })
            .collect(),
    })
}

/// A pull request a session linked.
#[derive(Debug, Clone, Serialize)]
pub struct PrRow {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub number: Option<i64>,
    pub url: String,
    pub repo: Option<String>,
    pub agent: String,
    pub title: Option<String>,
}

/// Pull requests, newest first; one session's when `session` is set.
pub fn prs(conn: &Connection, f: &Filter, session: Option<&str>, limit: u32) -> Result<Vec<PrRow>> {
    let (cond, mut args) = conditions(f);
    let only = match session {
        Some(id) => {
            args.push(id.to_string());
            args.push(id.to_string());
            "AND (s.id = ? OR s.parent_id = ?)"
        }
        None => "",
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT p.session_id, p.seq, p.ts, p.number, p.url, p.repo, s.agent, coalesce(ps.title, s.title)
         FROM prs p JOIN sessions s ON s.id = p.session_id
         LEFT JOIN sessions ps ON ps.id = s.parent_id
         LEFT JOIN events e ON e.session_id = p.session_id AND e.seq = p.seq
         WHERE {cond} {only}
         GROUP BY p.url ORDER BY max(p.ts) DESC LIMIT {limit}"
    ))?;
    let rows = stmt.query_map(params_from_iter(&args), |r| {
        Ok(PrRow {
            session_id: r.get(0)?,
            seq: r.get(1)?,
            ts: r.get(2)?,
            number: r.get(3)?,
            url: r.get(4)?,
            repo: r.get(5)?,
            agent: r.get(6)?,
            title: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Skills, plugins, MCP servers and apps, and how much they were used.
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    /// `skill`, `plugin`, `app` or `mcp`.
    pub kind: String,
    pub name: String,
    pub steps: i64,
    pub errors: i64,
    pub sessions: i64,
    pub last_ts: Option<String>,
}

pub fn sources(conn: &Connection, f: &Filter) -> Result<Vec<Source>> {
    let (cond, args) = conditions(f);
    // MCP tools are named `mcp__server__tool` (Claude Code) or
    // `server.tool` (Codex); group them by server.
    let mut stmt = conn.prepare(&format!(
        "SELECT kind, name, count(*), sum(is_error), count(DISTINCT root), max(ts) FROM (
            SELECT substr(e.via, 1, instr(e.via, ':') - 1) AS kind, substr(e.via, instr(e.via, ':') + 1) AS name,
                   e.is_error, coalesce(s.parent_id, s.id) AS root, e.ts
            FROM events e JOIN sessions s ON s.id = e.session_id
            WHERE e.via IS NOT NULL AND instr(e.via, ':') > 0 AND {cond}
            UNION ALL
            SELECT 'mcp',
                   CASE WHEN e.tool_name LIKE 'mcp\\_\\_%' ESCAPE '\\'
                        THEN substr(substr(e.tool_name, 6), 1, instr(substr(e.tool_name, 6), '__') - 1)
                        ELSE substr(e.tool_name, 1, instr(e.tool_name, '.') - 1) END,
                   e.is_error, coalesce(s.parent_id, s.id), e.ts
            FROM events e JOIN sessions s ON s.id = e.session_id
            WHERE e.category = 'mcp' AND e.via IS NULL AND {cond}
         ) WHERE name != '' GROUP BY kind, name ORDER BY count(*) DESC LIMIT 60"
    ))?;
    let doubled: Vec<&String> = args.iter().chain(args.iter()).collect();
    let rows = stmt.query_map(params_from_iter(doubled), |r| {
        Ok(Source {
            kind: r.get(0)?,
            name: r.get(1)?,
            steps: r.get(2)?,
            errors: r.get(3)?,
            sessions: r.get(4)?,
            last_ts: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
