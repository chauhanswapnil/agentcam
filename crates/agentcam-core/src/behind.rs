//! The steps behind a number.
//!
//! Every total the profiler shows (a part of the time bar, a kind of token,
//! a model, a cause of cache rebuilds, a fix) is a sum over steps. `behind`
//! lists those steps, biggest first, so the UI can open any of them in the
//! replay. A key names the number: `time:tools`, `cost:output`,
//! `model:<id>`, `cache:break`, `fix:<fix id>`.

use std::collections::HashMap;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::cache::{Cause, cache_in_full, session_cache};
use crate::fixes;
use crate::index::{Result, usage_from_row};
use crate::prices::{Parts, Prices};
use crate::profile::{Part, steps_for};
use crate::query::{Filter, conditions};

/// The most rows listed; `Behind::total` says how many there were.
const MAX_ROWS: usize = 100;

/// One step that holds some of a number.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StepRef {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub session_title: Option<String>,
    /// The step's own title (a command, a file, a tool).
    pub step_title: String,
    /// How much of the number this row holds.
    pub value: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Behind {
    /// Plain words for the heading: "Tools running".
    pub title: String,
    /// `ms`, `usd`, `tokens` or `count`: how to show `value`.
    pub unit: &'static str,
    /// Rows before the cut to `items`.
    pub total: i64,
    /// Biggest first, at most 100.
    pub items: Vec<StepRef>,
}

impl Behind {
    fn new(title: &str, unit: &'static str, mut rows: Vec<StepRef>) -> Behind {
        rows.sort_by(|a, b| {
            b.value
                .partial_cmp(&a.value)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| (&a.session_id, a.seq).cmp(&(&b.session_id, b.seq)))
        });
        let total = rows.len() as i64;
        rows.truncate(MAX_ROWS);
        Behind {
            title: title.to_string(),
            unit,
            total,
            items: rows,
        }
    }
}

/// The steps behind the number `what` names, over what the filter covers or
/// (with `session`) one session and its subagents. `None` when `what` isn't
/// a known key.
pub fn behind(
    conn: &Connection,
    prices: &Prices,
    f: &Filter,
    session: Option<&str>,
    what: &str,
) -> Result<Option<Behind>> {
    let Some((kind, rest)) = what.split_once(':') else {
        return Ok(None);
    };
    Ok(match kind {
        "time" => {
            let Some(part) = Part::from_key(rest) else {
                return Ok(None);
            };
            let title = match part {
                Part::Model => "Model working",
                Part::Tools => "Tools running",
                Part::Approval => "Waiting on approval (est.)",
                Part::You => "Ready for you",
                Part::Redo => "Redoing",
                Part::Orienting => "Reading and searching before the first change",
            };
            Some(Behind::new(title, "ms", steps_for(conn, f, session, part)?))
        }
        "cost" => {
            let (title, pick): (&str, fn(&Parts) -> f64) = match rest {
                "input" => ("Fresh input", |p| p.input),
                "output" => ("Output", |p| p.output),
                "cache_read" => ("Cache reads", |p| p.cache_read),
                "cache_write" => ("Cache writes", |p| p.cache_write),
                _ => return Ok(None),
            };
            let title = format!("{title} (est., at API list prices)");
            Some(Behind::new(
                &title,
                "usd",
                cost_rows(conn, prices, f, session, None, pick)?,
            ))
        }
        "model" if !rest.is_empty() => {
            let title = format!("{rest} (est., at API list prices)");
            Some(Behind::new(
                &title,
                "usd",
                cost_rows(conn, prices, f, session, Some(rest), Parts::total)?,
            ))
        }
        "cache" => {
            let Some((title, cause)) = cause_of(rest) else {
                return Ok(None);
            };
            Some(Behind::new(
                title,
                "tokens",
                rebuild_rows(conn, prices, f, session, cause)?,
            ))
        }
        "fix" => {
            // A cache fix is its cause's rebuilds.
            if let Some(cause) = rest.strip_prefix("cache:") {
                return behind(conn, prices, f, session, &format!("cache:{cause}"));
            }
            let Some(o) = fixes::occurrences(conn, prices, f, rest)? else {
                return Ok(None);
            };
            let rows = o
                .items
                .into_iter()
                .map(|x| StepRef {
                    session_id: x.example.session_id,
                    seq: x.example.seq,
                    ts: x.ts_text,
                    session_title: x.example.title,
                    step_title: x.step_title,
                    value: x.value as f64,
                })
                .collect();
            let mut b = Behind::new(&o.title, o.unit, rows);
            b.total = b.total.max(o.total);
            Some(b)
        }
        _ => None,
    })
}

fn cause_of(key: &str) -> Option<(&'static str, Cause)> {
    Some(match key {
        "break" => ("Cache written again after a break", Cause::Break),
        "model_switch" => (
            "Cache written again after a model switch",
            Cause::ModelSwitch,
        ),
        "effort_change" => (
            "Cache written again after an effort change",
            Cause::EffortChange,
        ),
        "unknown" => ("Cache written again, cause not in the log", Cause::Unknown),
        _ => return None,
    })
}

/// One row per session (per step inside a single session): the dollars one
/// kind of token, or one model, cost. `seq` is the step that cost most.
fn cost_rows(
    conn: &Connection,
    prices: &Prices,
    f: &Filter,
    session: Option<&str>,
    model: Option<&str>,
    pick: fn(&Parts) -> f64,
) -> Result<Vec<StepRef>> {
    let columns = "e.session_id, e.seq, e.ts, e.title, s.title,
         e.model, e.tok_in, e.tok_out, e.tok_cache_read, e.tok_cache_write, e.tok_cache_write_1h, e.tier";
    let has_tokens = "e.tok_in + e.tok_out + e.tok_cache_read + e.tok_cache_write > 0";
    let (sql, args) = match session {
        Some(id) => (
            format!(
                "SELECT {columns} FROM events e JOIN sessions s ON s.id = e.session_id
                 WHERE (s.id = ?1 OR s.parent_id = ?1) AND {has_tokens}"
            ),
            vec![id.to_string()],
        ),
        None => {
            let (cond, args) = conditions(f);
            (
                format!(
                    "SELECT {columns} FROM events e JOIN sessions s ON s.id = e.session_id
                     WHERE {cond} AND {has_tokens}"
                ),
                args,
            )
        }
    };
    // Per session, the total and the step that held the most of it.
    struct Acc {
        row: StepRef,
        biggest: f64,
    }
    let mut by: HashMap<(String, i64), Acc> = HashMap::new();
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(&args))?;
    while let Some(r) = rows.next()? {
        let u = usage_from_row(r, 5)?;
        if model.is_some_and(|m| u.model.as_deref() != Some(m)) {
            continue;
        }
        let Some(parts) = prices.parts(&u) else {
            continue;
        };
        let usd = pick(&parts);
        if usd <= 0.0 {
            continue;
        }
        let (id, seq): (String, i64) = (r.get(0)?, r.get(1)?);
        // Inside one session every step is its own row.
        let key = if session.is_some() {
            (id.clone(), seq)
        } else {
            (id.clone(), -1)
        };
        let acc = by.entry(key).or_insert_with(|| Acc {
            row: StepRef {
                session_id: id,
                seq,
                ts: None,
                session_title: None,
                step_title: String::new(),
                value: 0.0,
            },
            biggest: 0.0,
        });
        acc.row.value += usd;
        if usd > acc.biggest {
            acc.biggest = usd;
            acc.row.seq = seq;
            acc.row.ts = r.get(2)?;
            acc.row.step_title = r.get(3)?;
            acc.row.session_title = r.get(4)?;
        }
    }
    Ok(by.into_values().map(|a| a.row).collect())
}

fn rebuild_rows(
    conn: &Connection,
    prices: &Prices,
    f: &Filter,
    session: Option<&str>,
    cause: Cause,
) -> Result<Vec<StepRef>> {
    let cache = match session {
        Some(id) => session_cache(conn, prices, id)?,
        None => cache_in_full(conn, prices, f)?,
    };
    Ok(cache
        .latest
        .into_iter()
        .filter(|b| b.cause == cause)
        .map(|b| StepRef {
            step_title: match &b.model {
                Some(m) => format!("Wrote {} tokens to the cache ({m})", b.tokens),
                None => format!("Wrote {} tokens to the cache", b.tokens),
            },
            session_id: b.session_id,
            seq: b.seq,
            ts: b.ts,
            session_title: b.title,
            value: b.tokens as f64,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Index;

    /// A step to insert: (second, kind, category, tool, gap secs, duration
    /// secs, approval, wasted).
    type Row = (
        i64,
        &'static str,
        Option<&'static str>,
        Option<&'static str>,
        i64,
        Option<i64>,
        Option<&'static str>,
        bool,
    );

    fn index(session: &str, rows: &[Row]) -> Index {
        let idx = Index::open_in_memory().unwrap();
        idx.conn()
            .execute(
                "INSERT INTO sessions (id, agent, source_path, title) VALUES (?1, 'claude-code', '/Users/dev/x.jsonl', 'Fix login')",
                [session],
            )
            .unwrap();
        for (seq, (at, kind, category, tool, gap, dur, approval, wasted)) in rows.iter().enumerate()
        {
            let ts = format!("2026-11-03T10:{:02}:{:02}.000Z", at / 60, at % 60);
            idx.conn()
                .execute(
                    "INSERT INTO events (session_id, seq, ts, kind, category, tool_name, title, body,
                        source_offset, gap_ms, duration_ms, approval, wasted)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?11, '', 0, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        session,
                        seq as i64,
                        ts,
                        kind,
                        category,
                        tool,
                        gap * 1000,
                        dur.map(|d| d * 1000),
                        approval,
                        wasted,
                        format!("step {seq}")
                    ],
                )
                .unwrap();
        }
        idx
    }

    fn call(at: i64, tool: &'static str, category: &'static str, gap: i64, dur: i64) -> Row {
        (
            at,
            "tool_call",
            Some(category),
            Some(tool),
            gap,
            Some(dur),
            None,
            false,
        )
    }

    fn plain(at: i64, kind: &'static str, gap: i64) -> Row {
        (at, kind, None, None, gap, None, None, false)
    }

    /// The fixture from `profile::every_second_lands_in_one_part`.
    fn fixture() -> Index {
        index(
            "s1",
            &[
                plain(0, "prompt", 0),
                call(10, "Read", "read", 10, 2),
                call(20, "Edit", "edit", 10, 1),
                (
                    25,
                    "tool_call",
                    Some("command"),
                    Some("Bash"),
                    5,
                    Some(60),
                    Some("you"),
                    false,
                ),
                plain(90, "response", 65),
                plain(200, "prompt", 110),
                (
                    205,
                    "tool_call",
                    Some("command"),
                    Some("Bash"),
                    5,
                    Some(10),
                    Some("rule"),
                    true,
                ),
                plain(220, "response", 15),
            ],
        )
    }

    fn ask(idx: &Index, what: &str) -> Option<Behind> {
        behind(idx.conn(), idx.prices(), &Filter::default(), None, what).unwrap()
    }

    #[test]
    fn steps_behind_tools_running_add_up_to_the_part() {
        let idx = fixture();
        let b = ask(&idx, "time:tools").unwrap();
        assert_eq!((b.title.as_str(), b.unit), ("Tools running", "ms"));
        // The 60 s call is the approval's run (10 s); the read took 2 s, the
        // edit 1 s.
        let values: Vec<f64> = b.items.iter().map(|x| x.value).collect();
        assert_eq!(values, vec![10_000.0, 2_000.0, 1_000.0]);
        assert_eq!(values.iter().sum::<f64>(), 13_000.0);
        assert_eq!(b.total, 3);
        // Biggest first, and each row names its step.
        assert_eq!(b.items[0].seq, 3);
        assert_eq!(b.items[0].step_title, "step 3");
        assert_eq!(b.items[0].session_title.as_deref(), Some("Fix login"));
    }

    #[test]
    fn every_time_part_together_is_agent_time() {
        let idx = fixture();
        let total: f64 = ["model", "tools", "approval", "you", "redo"]
            .iter()
            .map(|p| {
                ask(&idx, &format!("time:{p}"))
                    .unwrap()
                    .items
                    .iter()
                    .map(|x| x.value)
                    .sum::<f64>()
            })
            .sum();
        let bar = crate::profile::time_bar(idx.conn(), &Filter::default()).unwrap();
        assert_eq!(total as i64, bar.total_ms);
        // The same, for one session.
        let one = behind(
            idx.conn(),
            idx.prices(),
            &Filter::default(),
            Some("s1"),
            "time:you",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            one.items.iter().map(|x| x.value).sum::<f64>(),
            bar.you_ms as f64
        );
    }

    #[test]
    fn the_hand_approved_call_is_what_approval_time_rests_on() {
        let idx = fixture();
        let b = ask(&idx, "time:approval").unwrap();
        assert_eq!(b.items.len(), 1);
        assert_eq!((b.items[0].seq, b.items[0].value), (3, 50_000.0));
    }

    #[test]
    fn reading_before_the_first_change_is_listed_by_turn() {
        let idx = fixture();
        let b = ask(&idx, "time:orienting").unwrap();
        // One turn read first: it began on the prompt at seq 0.
        assert_eq!(b.items.len(), 1);
        assert_eq!((b.items[0].seq, b.items[0].value), (0, 20_000.0));
    }

    #[test]
    fn an_unknown_key_is_none() {
        let idx = fixture();
        for key in [
            "",
            "time",
            "time:nothing",
            "cost:cheese",
            "cache:never",
            "nope:x",
            "model:",
        ] {
            assert_eq!(ask(&idx, key), None, "{key}");
        }
    }

    #[test]
    fn costs_are_listed_by_session_with_the_costliest_step() {
        let idx = Index::open_in_memory().unwrap();
        idx.conn()
            .execute_batch(
                "INSERT INTO sessions (id, agent, source_path, title) VALUES
                    ('a', 'claude-code', '/Users/dev/a.jsonl', 'Big one'),
                    ('b', 'claude-code', '/Users/dev/b.jsonl', 'Small one');
                 INSERT INTO events (session_id, seq, ts, kind, title, body, source_offset, model, tok_in, tok_out, tok_cache_read, tok_cache_write)
                 VALUES
                    ('a', 0, '2026-11-03T10:00:00.000Z', 'response', 'first', '', 0, 'claude-opus-5-5', 1000, 100, 0, 0),
                    ('a', 1, '2026-11-03T10:01:00.000Z', 'response', 'second', '', 0, 'claude-opus-5-5', 1000, 9000, 0, 0),
                    ('b', 0, '2026-11-03T10:02:00.000Z', 'response', 'only', '', 0, 'claude-sonnet-5-5', 1000, 100, 0, 0);",
            )
            .unwrap();
        let b = ask(&idx, "cost:output").unwrap();
        assert_eq!(b.unit, "usd");
        assert_eq!(b.items.len(), 2);
        assert_eq!(
            (
                b.items[0].session_id.as_str(),
                b.items[0].seq,
                b.items[0].step_title.as_str()
            ),
            ("a", 1, "second")
        );
        assert!(b.items[0].value > b.items[1].value);
        // A model's rows are only its own calls.
        let m = ask(&idx, "model:claude-sonnet-5-5").unwrap();
        assert_eq!(m.items.len(), 1);
        assert_eq!(m.items[0].session_id, "b");
        assert!(ask(&idx, "model:no-such-model").unwrap().items.is_empty());
    }

    #[test]
    fn a_rebuild_after_a_break_is_listed_with_its_tokens() {
        let idx = Index::open_in_memory().unwrap();
        idx.conn()
            .execute(
                "INSERT INTO sessions (id, agent, source_path, title) VALUES ('s', 'claude-code', '/Users/dev/x.jsonl', 'Fix login')",
                [],
            )
            .unwrap();
        // The calls from cache.rs's break test: 90 minutes away loses an
        // hour-long cache at seq 2.
        for (seq, (min, read, write)) in [
            (0, 0, 100_000),
            (1, 100_000, 2_000),
            (91, 0, 104_000),
            (92, 104_000, 1_000),
        ]
        .into_iter()
        .enumerate()
        {
            idx.conn()
                .execute(
                    "INSERT INTO events (session_id, seq, ts, kind, title, body, source_offset, model,
                        tok_in, tok_out, tok_cache_read, tok_cache_write, tok_cache_write_1h)
                     VALUES ('s', ?1, ?2, 'response', 'reply', '', 0, 'claude-opus-5-5', 1000, 100, ?3, ?4, ?4)",
                    rusqlite::params![
                        seq as i64,
                        format!("2026-11-03T{:02}:{:02}:00.000Z", 10 + min / 60, min % 60),
                        read,
                        write
                    ],
                )
                .unwrap();
        }
        let b = ask(&idx, "cache:break").unwrap();
        assert_eq!(b.unit, "tokens");
        assert_eq!(b.items.len(), 1);
        assert_eq!((b.items[0].seq, b.items[0].value), (2, 104_000.0));
        // A fix for the same cause lists the same rows.
        assert_eq!(ask(&idx, "fix:cache:break").unwrap().items, b.items);
        assert!(ask(&idx, "cache:model_switch").unwrap().items.is_empty());
    }
}
