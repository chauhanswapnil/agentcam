//! Where agent time went.
//!
//! Splits agent time (the capped gaps between steps, the number the
//! Overview shows) into what the time was spent on. Every millisecond of a
//! gap lands in exactly one part, so the parts add up to agent time.
//!
//! A gap before a step holds two things: the tool call that was still
//! running when the gap began, and the model working out the step. The
//! tool's share is read from its own duration; the rest is the model's.

use std::collections::HashMap;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::index::Result;
use crate::query::{Filter, conditions};
use crate::util::parse_ts_ms;

/// Agent time by what it went on, in milliseconds.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct TimeBar {
    /// All of it: the sum of the parts below.
    pub total_ms: i64,
    /// The model thinking, writing and choosing its next call.
    pub model_ms: i64,
    /// Commands, edits, reads and other calls running.
    pub tools_ms: i64,
    /// An estimate: time calls sat waiting for you to allow them.
    pub approval_ms: i64,
    /// The agent was done and waiting for your next prompt (each wait
    /// counted up to the idle cap, like all agent time).
    pub you_ms: i64,
    /// Steps in retry loops, error streaks and reverts.
    pub redo_ms: i64,
    /// Part of the model and tool time: reading and searching before the
    /// first change of a turn that went on to change something.
    pub orienting_ms: i64,
    /// Calls you allowed by hand that the approval estimate rests on.
    pub approved_by_hand: i64,
    pub sessions: i64,
}

struct Step {
    session: String,
    ts: Option<i64>,
    prompt: bool,
    tool: bool,
    look: bool,
    changes: bool,
    tool_name: Option<String>,
    gap: i64,
    duration: Option<i64>,
    approval: Option<String>,
    wasted: bool,
}

const COLUMNS: &str = "e.session_id, e.ts, e.kind, e.category, e.tool_name, e.gap_ms,
     e.duration_ms, e.approval, e.wasted";

/// The time bar across everything a filter covers.
pub fn time_bar(conn: &Connection, f: &Filter) -> Result<TimeBar> {
    let (cond, args) = conditions(f);
    let sql = format!(
        "SELECT {COLUMNS} FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE {cond} ORDER BY e.session_id, e.seq"
    );
    split(&load(conn, &sql, &args)?)
}

/// The time bar of one session and its subagents.
pub fn session_time_bar(conn: &Connection, id: &str) -> Result<TimeBar> {
    let sql = format!(
        "SELECT {COLUMNS} FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE s.id = ?1 OR s.parent_id = ?1 ORDER BY e.session_id, e.seq"
    );
    split(&load(conn, &sql, &[id.to_string()])?)
}

fn load(conn: &Connection, sql: &str, args: &[String]) -> Result<Vec<Step>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params_from_iter(args), |r| {
        let kind: String = r.get(2)?;
        let category: Option<String> = r.get(3)?;
        let c = category.as_deref();
        Ok(Step {
            session: r.get(0)?,
            ts: r
                .get::<_, Option<String>>(1)?
                .as_deref()
                .and_then(parse_ts_ms),
            prompt: kind == "prompt",
            tool: kind == "tool_call",
            look: matches!(c, Some("read" | "search" | "web" | "mcp")),
            changes: matches!(c, Some("edit" | "write")),
            tool_name: r.get(4)?,
            gap: r.get(5)?,
            duration: r.get(6)?,
            approval: r.get(7)?,
            wasted: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A call that waited on the person: allowed by hand, or turned down.
fn by_hand(approval: Option<&str>) -> bool {
    matches!(approval, Some("you" | "rejected"))
}

/// How long each tool usually runs when nobody has to allow it: the median
/// over calls a rule, a mode or the config let through.
fn usual_runs(steps: &[Step]) -> HashMap<&str, i64> {
    let mut by_tool: HashMap<&str, Vec<i64>> = HashMap::new();
    for s in steps {
        if let (true, Some(name), Some(d)) = (s.tool, s.tool_name.as_deref(), s.duration)
            && !by_hand(s.approval.as_deref())
            && !s
                .approval
                .as_deref()
                .is_some_and(|a| a.starts_with("blocked"))
        {
            by_tool.entry(name).or_default().push(d);
        }
    }
    by_tool
        .into_iter()
        .map(|(name, mut d)| {
            d.sort_unstable();
            (name, d[d.len() / 2])
        })
        .collect()
}

fn split(steps: &[Step]) -> Result<TimeBar> {
    let usual = usual_runs(steps);
    let mut bar = TimeBar::default();
    let mut prev: Option<&Step> = None;
    // When the tool calls in flight will all have ended.
    let mut running_end: Option<i64> = None;
    // Reading and searching so far in this turn; kept only if the turn
    // goes on to change a file.
    let (mut turn_looking, mut turn_changed) = (0i64, false);

    for c in steps {
        if prev.is_none_or(|p| p.session != c.session) {
            prev = None;
            running_end = None;
            (turn_looking, turn_changed) = (0, false);
            bar.sessions += 1;
        }
        let g = c.gap;
        if c.prompt {
            bar.you_ms += g;
            (turn_looking, turn_changed) = (0, false);
        } else if let Some(p) = prev {
            // The share of the gap a tool call was still running for.
            let tool = match (running_end, p.ts) {
                (Some(end), Some(start)) => (end - start).clamp(0, g),
                _ => 0,
            };
            let model = g - tool;
            if p.wasted {
                bar.redo_ms += tool;
            } else if by_hand(p.approval.as_deref()) {
                // The log times the wait and the run as one. Take off how
                // long this tool usually runs; the rest was the wait. A
                // call that was turned down never ran at all.
                let run = if p.approval.as_deref() == Some("rejected") {
                    0
                } else {
                    let name = p.tool_name.as_deref().unwrap_or("");
                    usual.get(name).copied().unwrap_or(tool).min(tool)
                };
                bar.approval_ms += tool - run;
                bar.tools_ms += run;
            } else {
                bar.tools_ms += tool;
            }
            if c.wasted {
                bar.redo_ms += model;
            } else {
                bar.model_ms += model;
            }
            if !turn_changed && !c.wasted && !p.wasted && (c.look || p.look) {
                turn_looking += g;
            }
        } else {
            // A session's first step has no gap to split.
            bar.model_ms += g;
        }

        if c.tool {
            if by_hand(c.approval.as_deref()) {
                bar.approved_by_hand += 1;
            }
            if let (Some(ts), Some(d)) = (c.ts, c.duration) {
                running_end = Some(running_end.map_or(ts + d, |e| e.max(ts + d)));
            }
            if c.changes && !turn_changed {
                turn_changed = true;
                bar.orienting_ms += turn_looking;
            }
        } else {
            running_end = None;
        }
        prev = Some(c);
    }
    bar.total_ms = bar.model_ms + bar.tools_ms + bar.approval_ms + bar.you_ms + bar.redo_ms;
    Ok(bar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Index;

    struct Row {
        at: i64,
        kind: &'static str,
        category: Option<&'static str>,
        tool: Option<&'static str>,
        gap: i64,
        dur: Option<i64>,
        approval: Option<&'static str>,
        wasted: bool,
    }

    fn row(at: i64, kind: &'static str, gap: i64) -> Row {
        Row {
            at,
            kind,
            category: None,
            tool: None,
            gap,
            dur: None,
            approval: None,
            wasted: false,
        }
    }

    fn call(at: i64, tool: &'static str, category: &'static str, gap: i64, dur: i64) -> Row {
        Row {
            category: Some(category),
            tool: Some(tool),
            dur: Some(dur * 1000),
            ..row(at, "tool_call", gap)
        }
    }

    fn index(session: &str, rows: &[Row]) -> Index {
        let idx = Index::open_in_memory().unwrap();
        idx.conn()
            .execute(
                "INSERT INTO sessions (id, agent, source_path) VALUES (?1, 'claude-code', '/Users/dev/x.jsonl')",
                [session],
            )
            .unwrap();
        for (seq, r) in rows.iter().enumerate() {
            let ts = format!("2026-11-03T10:{:02}:{:02}.000Z", r.at / 60, r.at % 60);
            idx.conn()
                .execute(
                    "INSERT INTO events (session_id, seq, ts, kind, category, tool_name, title, body,
                        source_offset, gap_ms, duration_ms, approval, wasted)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, '', '', 0, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        session,
                        seq as i64,
                        ts,
                        r.kind,
                        r.category,
                        r.tool,
                        r.gap * 1000,
                        r.dur,
                        r.approval,
                        r.wasted
                    ],
                )
                .unwrap();
        }
        idx
    }

    #[test]
    fn every_second_lands_in_one_part() {
        let idx = index(
            "s1",
            &[
                row(0, "prompt", 0),
                // 10 s of model, then a 2 s read.
                call(10, "Read", "read", 10, 2),
                // The read ran for 2 s of this gap; the model had 8.
                call(20, "Edit", "edit", 10, 1),
                // A command you allowed by hand: 60 s from call to result.
                Row {
                    approval: Some("you"),
                    ..call(25, "Bash", "command", 5, 60)
                },
                // Bash usually runs 10 s, so 50 s of that was the wait.
                row(90, "response", 65),
                // You came back after 110 s.
                row(200, "prompt", 110),
                // A retry: its model time and its run both count as redoing.
                Row {
                    approval: Some("rule"),
                    wasted: true,
                    ..call(205, "Bash", "command", 5, 10)
                },
                row(220, "response", 15),
            ],
        );
        let b = time_bar(idx.conn(), &Filter::default()).unwrap();
        assert_eq!(b.model_ms, (10 + 8 + 4 + 5 + 5) * 1000);
        assert_eq!(b.tools_ms, (2 + 1 + 10) * 1000);
        assert_eq!(b.approval_ms, 50_000);
        assert_eq!(b.you_ms, 110_000);
        assert_eq!(b.redo_ms, 15_000);
        // The parts add up to agent time: the sum of the gaps.
        assert_eq!(b.total_ms, 220_000);
        // The read came before the turn's first edit: both gaps it touched.
        assert_eq!(b.orienting_ms, 20_000);
        assert_eq!((b.approved_by_hand, b.sessions), (1, 1));
        assert_eq!(session_time_bar(idx.conn(), "s1").unwrap(), b);
    }

    #[test]
    fn reading_in_a_turn_that_changes_nothing_is_not_finding_its_way() {
        let idx = index(
            "s2",
            &[
                row(0, "prompt", 0),
                call(10, "Read", "read", 10, 1),
                call(20, "Grep", "search", 10, 1),
                row(30, "response", 10),
            ],
        );
        let b = time_bar(idx.conn(), &Filter::default()).unwrap();
        assert_eq!(b.orienting_ms, 0);
        assert_eq!(b.total_ms, 30_000);
        assert_eq!((b.model_ms, b.tools_ms), (28_000, 2_000));
    }

    #[test]
    fn calls_in_flight_together_are_counted_once() {
        // Two reads start a second apart; the first runs 20 s. The model's
        // reply comes 25 s after the second call.
        let idx = index(
            "s3",
            &[
                row(0, "prompt", 0),
                call(5, "Read", "read", 5, 20),
                call(6, "Read", "read", 1, 2),
                row(31, "response", 25),
            ],
        );
        let b = time_bar(idx.conn(), &Filter::default()).unwrap();
        // 1 s between the calls, then the long read ends at 25: 19 more.
        assert_eq!(b.tools_ms, 20_000);
        assert_eq!(b.model_ms, 5_000 + 6_000);
        assert_eq!(b.total_ms, 31_000);
    }

    #[test]
    fn a_call_you_turned_down_is_all_wait() {
        let idx = index(
            "s4",
            &[
                row(0, "prompt", 0),
                Row {
                    approval: Some("rejected"),
                    ..call(5, "Bash", "command", 5, 30)
                },
                row(40, "response", 35),
            ],
        );
        let b = time_bar(idx.conn(), &Filter::default()).unwrap();
        assert_eq!((b.approval_ms, b.tools_ms), (30_000, 0));
        assert_eq!(b.model_ms, 10_000);
    }
}
