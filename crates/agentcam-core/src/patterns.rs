//! Spot where an agent got stuck: the same call failing again and again,
//! a run of failures, or a file edited back and forth.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::model::{EventKind, FileAction, ParsedSession};

/// A stretch of a session that looks like wasted effort.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Pattern {
    /// `retry_loop`, `error_streak`, `edit_churn` or `revert`.
    pub kind: &'static str,
    pub start_seq: u32,
    pub end_seq: u32,
    pub title: String,
    /// Agent time spent on the wasted steps.
    pub wasted_ms: i64,
    /// Tokens spent on the wasted steps (where the log has them).
    pub wasted_tokens: i64,
}

/// Same call failing at least this often counts as a retry loop.
const RETRY_MIN: usize = 3;
/// This many failed tool calls in a row count as an error streak.
const STREAK_MIN: usize = 4;
/// This many edits to one file count as churn.
const CHURN_MIN: usize = 6;

/// `gaps[i]` is the agent time before step `i` (see the index).
pub fn detect(p: &ParsedSession, gaps: &[i64]) -> Vec<Pattern> {
    let ev = &p.events;
    let cost = |i: usize| -> (i64, i64) {
        let ms = ev[i]
            .duration_ms
            .map_or(gaps.get(i).copied().unwrap_or(0), |d| d as i64);
        let t = ev[i]
            .usage
            .as_ref()
            .map_or(0, |u| (u.input + u.output + u.cache_write) as i64);
        (ms, t)
    };
    let mut out = Vec::new();

    // Retry loops: the same command (or tool + target) failing repeatedly.
    let mut fails: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in ev.iter().enumerate() {
        if e.kind == EventKind::ToolCall && e.is_error {
            let key = format!(
                "{}|{}",
                e.tool_name.as_deref().unwrap_or(""),
                normalize(&e.title)
            );
            fails.entry(key).or_default().push(i);
        }
    }
    let mut loops: Vec<_> = fails
        .into_values()
        .filter(|v| v.len() >= RETRY_MIN)
        .collect();
    loops.sort();
    for idx in loops {
        let (ms, t) = idx
            .iter()
            .map(|&i| cost(i))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        out.push(Pattern {
            kind: "retry_loop",
            start_seq: idx[0] as u32,
            end_seq: *idx.last().unwrap() as u32,
            title: format!(
                "Retried {} times: {}",
                idx.len(),
                crate::model::first_line(&ev[idx[0]].title, 120)
            ),
            wasted_ms: ms,
            wasted_tokens: t,
        });
    }

    // Error streaks: consecutive failed tool calls (other steps in between
    // don't break the streak; a successful tool call does).
    let mut run: Vec<usize> = Vec::new();
    let flush = |run: &mut Vec<usize>, out: &mut Vec<Pattern>| {
        if run.len() >= STREAK_MIN {
            let (ms, t) = run
                .iter()
                .map(|&i| cost(i))
                .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            out.push(Pattern {
                kind: "error_streak",
                start_seq: run[0] as u32,
                end_seq: *run.last().unwrap() as u32,
                title: format!("{} tool calls in a row errored", run.len()),
                wasted_ms: ms,
                wasted_tokens: t,
            });
        }
        run.clear();
    };
    for (i, e) in ev.iter().enumerate() {
        if e.kind != EventKind::ToolCall {
            continue;
        }
        if e.is_error {
            run.push(i);
        } else {
            flush(&mut run, &mut out);
        }
    }
    flush(&mut run, &mut out);

    // Reverts: an edit that undoes an earlier edit to the same file.
    let mut edits: Vec<(usize, String, String, String)> = Vec::new();
    for (i, e) in ev.iter().enumerate() {
        let Some(inp) = e.input.as_ref() else {
            continue;
        };
        if e.tool_name.as_deref() != Some("Edit") {
            continue;
        }
        let s = |k: &str| inp.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let (file, old, new) = (s("file_path"), s("old_string"), s("new_string"));
        if old.is_empty() && new.is_empty() {
            continue;
        }
        if let Some(prev) = edits
            .iter()
            .rev()
            .find(|(_, f, o, n)| *f == file && *o == new && *n == old)
        {
            let (ms, t) = (prev.0..=i)
                .map(cost)
                .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            out.push(Pattern {
                kind: "revert",
                start_seq: prev.0 as u32,
                end_seq: i as u32,
                title: format!("Undid an earlier edit to {}", short_path(&file)),
                wasted_ms: ms,
                wasted_tokens: t,
            });
        }
        edits.push((i, file, old, new));
    }

    // Churn: one file edited many times.
    let mut per_file: HashMap<&str, Vec<u32>> = HashMap::new();
    for t in &p.touches {
        if t.action == FileAction::Edit {
            per_file.entry(t.path.as_str()).or_default().push(t.seq);
        }
    }
    let mut churn: Vec<_> = per_file
        .into_iter()
        .filter(|(_, v)| v.len() >= CHURN_MIN)
        .collect();
    churn.sort_by_key(|(_, v)| v[0]);
    for (path, seqs) in churn {
        out.push(Pattern {
            kind: "edit_churn",
            start_seq: seqs[0],
            end_seq: *seqs.last().unwrap(),
            title: format!("Edited {} {} times", short_path(path), seqs.len()),
            // Churn is a signal, not proof of waste: count no time.
            wasted_ms: 0,
            wasted_tokens: 0,
        });
    }
    out.sort_by_key(|p| p.start_seq);
    out
}

/// The steps counted as wasted: failed calls inside loops and streaks,
/// and every step of a revert. Each step counts once.
pub fn wasted_steps(p: &ParsedSession, patterns: &[Pattern]) -> std::collections::BTreeSet<u32> {
    let mut seen = std::collections::BTreeSet::new();
    for pat in patterns.iter().filter(|p| p.wasted_ms > 0) {
        for i in pat.start_seq..=pat.end_seq {
            let e = &p.events[i as usize];
            if pat.kind == "revert" || (e.kind == EventKind::ToolCall && e.is_error) {
                seen.insert(i);
            }
        }
    }
    seen
}

/// Total wasted time, counting each step once even if patterns overlap.
pub fn wasted_ms(p: &ParsedSession, gaps: &[i64], patterns: &[Pattern]) -> i64 {
    wasted_steps(p, patterns)
        .into_iter()
        .map(|i| {
            let e = &p.events[i as usize];
            e.duration_ms
                .map_or(gaps.get(i as usize).copied().unwrap_or(0), |d| d as i64)
        })
        .sum()
}

/// Collapse numbers and whitespace so `pytest -k a` retried with small
/// changes still groups together.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn short_path(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Event, FileTouch, ToolCategory};
    use serde_json::json;

    fn call(seq: u32, title: &str, err: bool) -> Event {
        let mut e = Event::new(EventKind::ToolCall, 0, None);
        e.seq = seq;
        e.tool_name = Some("Bash".into());
        e.category = Some(ToolCategory::Command);
        e.title = title.into();
        e.is_error = err;
        e.duration_ms = Some(1000);
        e
    }

    #[test]
    fn finds_retry_loops_and_streaks() {
        let events = vec![
            call(0, "npm test", true),
            call(1, "npm test", true),
            call(2, "cat a.ts", true),
            call(3, "npm test", true),
            call(4, "npm test", false),
            call(5, "ls", false),
        ];
        let p = ParsedSession {
            events,
            ..Default::default()
        };
        let pats = detect(&p, &[0; 6]);
        let kinds: Vec<_> = pats.iter().map(|p| p.kind).collect();
        assert_eq!(kinds, ["retry_loop", "error_streak"]);
        assert_eq!(pats[0].title, "Retried 3 times: npm test");
        assert_eq!(
            (pats[0].start_seq, pats[0].end_seq, pats[0].wasted_ms),
            (0, 3, 3000)
        );
        assert_eq!(wasted_ms(&p, &[0; 6], &pats), 4000, "each failed step once");
    }

    #[test]
    fn finds_reverts_and_churn() {
        let edit = |seq, old: &str, new: &str| {
            let mut e = Event::new(EventKind::ToolCall, 0, None);
            e.seq = seq;
            e.tool_name = Some("Edit".into());
            e.input = Some(json!({"file_path": "/a/x.ts", "old_string": old, "new_string": new}));
            e
        };
        let events: Vec<_> = (0..6)
            .map(|i| {
                if i == 3 {
                    edit(i, "b", "a")
                } else {
                    edit(i, "a", "b")
                }
            })
            .collect();
        let touches = (0..6)
            .map(|i| FileTouch {
                seq: i,
                path: "/a/x.ts".into(),
                action: FileAction::Edit,
                diff: None,
            })
            .collect();
        let p = ParsedSession {
            events,
            touches,
            ..Default::default()
        };
        let kinds: Vec<_> = detect(&p, &[0; 6]).iter().map(|p| p.kind).collect();
        assert!(
            kinds.contains(&"revert") && kinds.contains(&"edit_churn"),
            "{kinds:?}"
        );
    }
}
