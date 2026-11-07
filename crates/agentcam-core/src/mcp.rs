//! A read-only MCP server over stdio (`agentcam mcp`), so agents can look
//! up their own past work: earlier sessions, what a repo's instructions
//! ask, which lines lasted, who wrote a line.
//!
//! [`handle`] answers one JSON-RPC message; the binary reads lines from
//! stdin and writes answers to stdout. Nothing here writes to the index.

use rusqlite::Connection;
use serde_json::{Value, json};

use crate::query::{self, Filter};

const PROTOCOL: &str = "2025-06-18";
/// Longest text one tool answer returns.
const MAX_ANSWER: usize = 60_000;

const INSTRUCTIONS: &str = "agentcam keeps a local index of past Claude Code and Codex sessions on this machine. \
Use it to see how something was done before, what a repo's AGENTS.md or CLAUDE.md asks and what tends to get missed, \
which past edits to a file lasted, and which session wrote a line. Pass `project` as the repo folder (your working directory).";

fn tools() -> Value {
    let project = json!({"type": "string", "description": "Repo folder (absolute path); worktrees count toward their repo."});
    json!([
        {
            "name": "search_sessions",
            "description": "Full-text search across past prompts, replies, commands, output and file paths. Returns the best matching steps with their session id and step number.",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string", "description": "Words to look for."},
                "project": project,
                "limit": {"type": "integer", "description": "Default 10, at most 50."}
            }, "required": ["query"]},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "recent_sessions",
            "description": "The latest sessions, newest first, with title, agent, model, time, cost and counts.",
            "inputSchema": {"type": "object", "properties": {
                "project": project,
                "limit": {"type": "integer", "description": "Default 10, at most 50."}
            }},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "get_session",
            "description": "One session: its turns (each prompt and what followed), files changed, commits, findings, lines still in the code, and instruction rules not followed.",
            "inputSchema": {"type": "object", "properties": {
                "session_id": {"type": "string"}
            }, "required": ["session_id"]},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "get_step",
            "description": "One step in full: the command or tool input, its output, and file diffs.",
            "inputSchema": {"type": "object", "properties": {
                "session_id": {"type": "string"},
                "seq": {"type": "integer", "description": "Step number."}
            }, "required": ["session_id", "seq"]},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "file_history",
            "description": "Every session step that read or changed a file, newest first.",
            "inputSchema": {"type": "object", "properties": {
                "path": {"type": "string", "description": "Absolute file path."}
            }, "required": ["path"]},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "blame",
            "description": "For each line of a file as it is today, the agent session, step, model and prompt that wrote it (or none).",
            "inputSchema": {"type": "object", "properties": {
                "path": {"type": "string", "description": "Absolute file path."},
                "start_line": {"type": "integer"},
                "end_line": {"type": "integer"}
            }, "required": ["path"]},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "project_rules",
            "description": "The checkable rules in a repo's AGENTS.md / CLAUDE.md (checks to run, package manager, files not to edit, commands not to run) and how often past sessions followed each, with examples of misses.",
            "inputSchema": {"type": "object", "properties": {
                "project": project
            }, "required": ["project"]},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "what_lasted",
            "description": "Of the lines agents added, how many are still in the code, rewritten by later agent edits, or changed since; by age, model and file.",
            "inputSchema": {"type": "object", "properties": {
                "project": project,
                "days": {"type": "integer", "description": "Edits from the last N days. Default 90."}
            }},
            "annotations": {"readOnlyHint": true}
        }
    ])
}

/// Protocol versions this server speaks, newest first.
const PROTOCOLS: &[&str] = &[PROTOCOL, "2025-03-26", "2024-11-05"];

/// Answer one JSON-RPC message, or `None` for notifications. Every tool
/// answer has secrets masked with `rules` (the same masking as exports).
pub fn handle(conn: &Connection, rules: &crate::audit::Rules, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let id = id?;
    let result = match method {
        "initialize" => {
            // Echo the client's version if we speak it, else offer ours.
            let v = msg
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .filter(|v| PROTOCOLS.contains(v))
                .unwrap_or(PROTOCOL);
            Ok(json!({
                "protocolVersion": v,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "agentcam", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = msg
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = msg
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or(json!({}));
            Ok(match call(conn, name, &args) {
                Ok(text) => {
                    let text = rules.mask_secrets(&text).into_owned();
                    json!({"content": [{"type": "text", "text": cut(text)}]})
                }
                Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
            })
        }
        _ => Err(json!({"code": -32601, "message": format!("unknown method {method}")})),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

fn cut(mut s: String) -> String {
    if s.len() > MAX_ANSWER {
        let mut i = MAX_ANSWER;
        while !s.is_char_boundary(i) {
            i -= 1;
        }
        s.truncate(i);
        s.push_str("\n… (cut; ask for less)");
    }
    s
}

fn s<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Value::as_str).filter(|v| !v.is_empty())
}

fn n(a: &Value, k: &str, default: i64, max: i64) -> i64 {
    a.get(k)
        .and_then(Value::as_i64)
        .unwrap_or(default)
        .clamp(1, max)
}

fn project_filter(a: &Value) -> Filter {
    Filter {
        project: s(a, "project").map(crate::project_root),
        ..Default::default()
    }
}

fn pretty(v: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn err(e: impl std::fmt::Display) -> String {
    format!("agentcam index error: {e}")
}

fn call(conn: &Connection, name: &str, a: &Value) -> Result<String, String> {
    match name {
        "search_sessions" => {
            let q = s(a, "query").ok_or("query is required")?;
            let hits = query::search(
                conn,
                q,
                &project_filter(a),
                None,
                None,
                n(a, "limit", 10, 50) as u32,
            )
            .map_err(err)?;
            if hits.is_empty() {
                return Ok("No matches.".into());
            }
            let rows: Vec<Value> = hits
                .iter()
                .map(|h| {
                    json!({
                        "session_id": h.session_id, "seq": h.seq, "when": h.ts, "kind": h.kind,
                        "step": h.title, "match": h.snippet.replace(['\u{1}', '\u{2}'], ""),
                        "session": h.session_title, "project": h.project_root, "agent": h.agent
                    })
                })
                .collect();
            Ok(pretty(&rows))
        }
        "recent_sessions" => {
            let page = query::sessions(conn, &project_filter(a), n(a, "limit", 10, 50) as u32, 0)
                .map_err(err)?;
            let rows: Vec<Value> = page
                .items
                .iter()
                .map(|x| {
                    json!({
                        "session_id": x.id, "title": x.title.as_ref().or(x.first_prompt.as_ref()),
                        "agent": x.agent, "model": x.model, "project": x.project_root, "branch": x.git_branch,
                        "ended": x.ended_at, "agent_time_min": x.active_ms / 60_000,
                        "steps": x.event_count, "errored_steps": x.error_count, "files": x.file_count,
                        "est_cost_usd": x.est_cost_usd
                    })
                })
                .collect();
            Ok(pretty(&rows))
        }
        "get_session" => {
            let id = s(a, "session_id").ok_or("session_id is required")?;
            let d = query::session(conn, id)
                .map_err(err)?
                .ok_or("no session with that id")?;
            // Each path and action once, in the order first touched.
            let mut seen = std::collections::HashSet::new();
            let files: Vec<(String, String)> = d
                .files
                .iter()
                .filter(|f| f.action != "read")
                .map(|f| (f.path.clone(), f.action.clone()))
                .filter(|x| seen.insert(x.clone()))
                .collect();
            let missed: Vec<Value> = d
                .follow
                .as_ref()
                .map(|f| {
                    f.outcomes
                        .iter()
                        .filter(|o| o.status == "not_followed")
                        .map(|o| {
                            let rule = f.rules.iter().find(|r| r.id == o.rule_id);
                            json!({"rule": rule.map(|r| &r.source), "detail": o.detail, "seq": o.seq})
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(pretty(&json!({
                "session_id": d.session.id, "title": d.session.title, "agent": d.session.agent,
                "model": d.session.model, "project": d.session.project_root, "branch": d.session.git_branch,
                "started": d.session.started_at, "ended": d.session.ended_at,
                "agent_time_min": d.session.active_ms / 60_000, "est_cost_usd": d.session.est_cost_usd,
                "turns": d.turns.iter().map(|t| json!({
                    "n": t.n, "first_seq": t.seq, "last_seq": t.end_seq, "prompt": t.prompt,
                    "steps": t.steps, "errored": t.errors
                })).collect::<Vec<_>>(),
                "files_changed": files.iter().map(|(p, a)| format!("{a} {p}")).collect::<Vec<_>>(),
                "commits": d.commits.iter().map(|c| json!({"seq": c.seq, "hash": c.hash, "message": c.message})).collect::<Vec<_>>(),
                "pull_requests": d.prs.iter().map(|p| &p.url).collect::<Vec<_>>(),
                "findings": d.findings.iter().map(|f| json!({"seq": f.seq, "rule": f.title, "severity": f.severity})).collect::<Vec<_>>(),
                "lines_lasted": d.lasted,
                "instruction_rules_not_followed": missed,
                "subagents": d.subagents.iter().map(|x| json!({"session_id": x.id, "title": x.title})).collect::<Vec<_>>(),
            })))
        }
        "get_step" => {
            let id = s(a, "session_id").ok_or("session_id is required")?;
            let seq = a
                .get("seq")
                .and_then(Value::as_i64)
                .ok_or("seq is required")?;
            let st = query::step(conn, id, seq)
                .map_err(err)?
                .ok_or("no such step")?;
            Ok(pretty(&json!({
                "seq": st.seq, "when": st.ts, "kind": st.kind, "tool": st.tool_name, "title": st.title,
                "errored": st.is_error, "input": st.input.map(|i| crate::model::truncate(&i, 8_000)),
                "output": crate::model::truncate(&st.body, 20_000),
                "files": st.touches.iter().map(|t| json!({
                    "path": t.path, "action": t.action,
                    "diff": t.diff.as_deref().map(|d| crate::model::truncate(d, 8_000)),
                    "lines_lasted": t.lasted
                })).collect::<Vec<_>>()
            })))
        }
        "file_history" => {
            let p = s(a, "path").ok_or("path is required")?;
            let rows = query::file_history(conn, p, 100).map_err(err)?;
            if rows.is_empty() {
                return Ok("No session touched this file.".into());
            }
            Ok(pretty(&rows))
        }
        "blame" => {
            let p = s(a, "path").ok_or("path is required")?;
            let b = crate::survival::blame(conn, p).map_err(err)?;
            if !b.found {
                return Err("can't read that file".into());
            }
            if b.edits.is_empty() && b.lines.is_empty() {
                return Ok("No agent session edited this file.".into());
            }
            let start = a
                .get("start_line")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .max(1) as usize;
            let end = a
                .get("end_line")
                .and_then(Value::as_u64)
                .map_or(start + 299, |e| e as usize);
            let mut out = format!(
                "{} of {} lines written by agent sessions.\n\nEdits:\n",
                b.agent_lines,
                b.lines.len()
            );
            for (i, e) in b.edits.iter().enumerate() {
                out.push_str(&format!(
                    "[{i}] {} #{} {} {} {} — {}\n",
                    e.session_id,
                    e.seq,
                    &e.ts[..e.ts.len().min(10)],
                    e.agent,
                    e.model.as_deref().unwrap_or(""),
                    e.prompt.as_deref().unwrap_or("").replace('\n', " ")
                ));
            }
            out.push_str("\nLines:\n");
            for l in b.lines.iter().filter(|l| l.n >= start && l.n <= end) {
                let tag = l.edit.map_or("   ".to_string(), |e| format!("[{e}]"));
                out.push_str(&format!("{:>5} {tag} {}\n", l.n, l.text));
            }
            Ok(out)
        }
        "project_rules" => {
            let f = project_filter(a);
            if f.project.is_none() {
                return Err("project is required".into());
            }
            let r = crate::follow::report(conn, &f).map_err(err)?;
            if r.is_empty() {
                return Ok(
                    "No checkable rules found in this repo's AGENTS.md or CLAUDE.md.".into(),
                );
            }
            Ok(pretty(&r))
        }
        "what_lasted" => {
            let days = n(a, "days", 90, 3650);
            let mut f = project_filter(a);
            f.since = Some(crate::util::format_ts_ms(
                crate::util::now_ms() - days * 86_400_000,
            ));
            let r = crate::survival::report(conn, &f, crate::util::now_ms()).map_err(err)?;
            Ok(pretty(&json!({
                "settled": r.settled, "last_day": r.recent, "by_age": r.ages,
                "by_agent_model": r.by_agent, "files_most_changed": r.files.iter().take(20).collect::<Vec<_>>()
            })))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_list_and_unknowns() {
        let conn = Connection::open_in_memory().unwrap();
        let rules = crate::audit::Rules::builtin();
        let handle = |c: &Connection, m: &Value| handle(c, &rules, m);
        let init = handle(&conn, &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}})).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "agentcam");
        let future = handle(&conn, &json!({"jsonrpc":"2.0","id":9,"method":"initialize","params":{"protocolVersion":"2099-01-01"}})).unwrap();
        assert_eq!(future["result"]["protocolVersion"], PROTOCOL);
        assert!(
            handle(
                &conn,
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
            )
            .is_none()
        );
        let list = handle(
            &conn,
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"blame") && names.contains(&"project_rules"));
        let bad = handle(&conn, &json!({"jsonrpc":"2.0","id":3,"method":"nope"})).unwrap();
        assert_eq!(bad["error"]["code"], -32601);
        let missing = handle(&conn, &json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_step","arguments":{}}})).unwrap();
        assert_eq!(missing["result"]["isError"], true);
    }
}
