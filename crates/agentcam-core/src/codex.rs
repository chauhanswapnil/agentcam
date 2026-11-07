//! Parser for Codex session logs: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.
//!
//! Newer Codex versions write an `event_msg/item_completed` record for each
//! finished step, with typed items (`UserMessage`, `CommandExecution`,
//! `FileChange`, ...). Older logs only have `response_item` records. We
//! build events from both and keep whichever the file actually has.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde_json::Value;

use crate::Agent;
use crate::model::*;
use crate::util::{for_each_record, str_of, text_of};

#[derive(Default)]
struct State {
    out: ParsedSession,
    /// Events from `item_completed` records (newer format).
    items: Vec<Event>,
    item_touches: Vec<(usize, FileTouch)>,
    /// Events from `response_item` / `event_msg` records (older format).
    legacy: Vec<Event>,
    legacy_touches: Vec<(usize, FileTouch)>,
    pending: HashMap<String, usize>,
    /// Settings from each `turn_context`, in order, with their time.
    contexts: Vec<(String, Turn)>,
    /// Times Codex marked a turn complete.
    completes: Vec<String>,
}

/// Parse one Codex rollout file.
pub fn parse(path: &Path) -> io::Result<ParsedSession> {
    let mut st = State::default();
    st.out.session.agent = Some(Agent::Codex);
    st.out.session.source_path = path.to_string_lossy().into_owned();

    st.out.bad_lines = for_each_record(path, |rec, offset| st.record(&rec, offset))?;

    let mut out = st.out;
    let (events, touches) = if st.items.is_empty() {
        (st.legacy, st.legacy_touches)
    } else {
        (st.items, st.item_touches)
    };
    out.events = events;
    out.turns = turns_from(&out.events, &st.contexts, &st.completes);
    out.touches = touches
        .into_iter()
        .map(|(idx, mut t)| {
            t.seq = idx as u32;
            t
        })
        .collect();
    if out.session.id.is_empty() {
        out.session.id = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
    }
    out.finish();
    Ok(out)
}

impl State {
    fn record(&mut self, rec: &Value, offset: u64) {
        let ts = str_of(rec, "timestamp").map(String::from);
        let p = rec.get("payload").unwrap_or(&Value::Null);
        let ptype = str_of(p, "type").unwrap_or("");
        match (str_of(rec, "type").unwrap_or(""), ptype) {
            ("session_meta", _) => self.meta(p),
            // The tools on offer; MCP servers are namespaces named `mcp__<server>`.
            ("response_item", "additional_tools") => {
                for ns in p
                    .get("tools")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let Some(server) = str_of(ns, "name").and_then(|n| n.strip_prefix("mcp__"))
                    else {
                        continue;
                    };
                    for t in ns
                        .get("tools")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if let (Some(name), Some(desc)) =
                            (str_of(t, "name"), str_of(t, "description"))
                        {
                            self.out.add_tool_def(server, name, desc, ts.as_deref());
                        }
                    }
                }
            }
            ("turn_context", _) => {
                if let Some(m) = str_of(p, "model") {
                    self.out.session.model = Some(m.to_string());
                }
                if let Some(ts) = ts {
                    let sandbox = p.get("sandbox_policy").and_then(|s| {
                        s.as_str()
                            .map(String::from)
                            .or_else(|| str_of(s, "type").map(String::from))
                    });
                    self.contexts.push((
                        ts,
                        Turn {
                            model: str_of(p, "model").map(String::from),
                            effort: str_of(p, "effort").map(String::from),
                            mode: p
                                .get("collaboration_mode")
                                .and_then(|c| str_of(c, "mode"))
                                .map(String::from),
                            approval_policy: str_of(p, "approval_policy").map(String::from),
                            sandbox,
                            ..Default::default()
                        },
                    ));
                }
            }
            ("event_msg", "task_complete") => {
                if let Some(ts) = ts {
                    self.completes.push(ts);
                }
            }
            ("event_msg", "thread_settings_applied") => {
                if let Some(t) = p
                    .get("thread_settings")
                    .and_then(|t| str_of(t, "service_tier"))
                {
                    self.out.session.service_tier = Some(t.to_string());
                }
            }
            ("event_msg", "token_count") => {
                self.tokens(p);
                self.rate_limits(p, ts);
            }
            ("event_msg", "item_completed") => {
                if let Some(item) = p.get("item") {
                    self.item(item, offset, ts);
                }
            }
            ("event_msg", "user_message") => {
                let text = str_of(p, "message").unwrap_or("");
                push_text(&mut self.legacy, EventKind::Prompt, text, offset, ts);
            }
            ("event_msg", "agent_message") => {
                let text = str_of(p, "message").unwrap_or("");
                push_text(&mut self.legacy, EventKind::Response, text, offset, ts);
            }
            ("response_item", "reasoning") => {
                let text = summary_text(p.get("summary"));
                push_text(&mut self.legacy, EventKind::Thinking, &text, offset, ts);
            }
            ("response_item", "function_call" | "custom_tool_call" | "local_shell_call") => {
                self.legacy_call(p, offset, ts)
            }
            ("response_item", "function_call_output" | "custom_tool_call_output") => {
                self.legacy_output(p)
            }
            _ => {}
        }
    }

    fn meta(&mut self, p: &Value) {
        let s = &mut self.out.session;
        if let Some(id) = str_of(p, "id").or_else(|| str_of(p, "session_id")) {
            s.id = id.to_string();
        }
        s.project_path = str_of(p, "cwd").map(String::from).or(s.project_path.take());
        s.agent_version = str_of(p, "cli_version").map(String::from);
        s.entrypoint = str_of(p, "originator")
            .or_else(|| str_of(p, "source"))
            .map(String::from)
            .or(s.entrypoint.take());
        s.started_at = str_of(p, "timestamp").map(String::from);
        s.parent_id = str_of(p, "parent_thread_id").map(String::from);
        // Background threads Codex starts itself (e.g. `guardian_review`).
        if let Some(src) = str_of(p, "thread_source").filter(|t| *t != "user") {
            s.title = Some(src.replace('_', " "));
        }
        if let Some(b) = p.get("git").and_then(|g| str_of(g, "branch")) {
            s.git_branch = Some(b.to_string());
        }
    }

    fn tokens(&mut self, p: &Value) {
        // Per-call usage goes on the latest step.
        if let Some(l) = p.get("info").and_then(|i| i.get("last_token_usage")) {
            let n = |k: &str| l.get(k).and_then(Value::as_u64).unwrap_or(0);
            let cached = n("cached_input_tokens");
            let model = self.out.session.model.clone();
            let target = if self.items.is_empty() {
                self.legacy.last_mut()
            } else {
                self.items.last_mut()
            };
            if let Some(ev) = target {
                let u = ev.usage.get_or_insert_with(|| TokenUsage {
                    model,
                    ..Default::default()
                });
                u.input += n("input_tokens").saturating_sub(cached);
                u.cache_read += cached;
                u.cache_write += n("cache_write_input_tokens");
                u.output += n("output_tokens");
                u.reasoning += n("reasoning_output_tokens");
            }
        }
        let Some(u) = p.get("info").and_then(|i| i.get("total_token_usage")) else {
            return;
        };
        let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        let s = &mut self.out.session;
        let cached = n("cached_input_tokens");
        s.input_tokens = n("input_tokens").saturating_sub(cached);
        s.cache_read_tokens = cached;
        s.cache_write_tokens = n("cache_write_input_tokens");
        s.output_tokens = n("output_tokens");
        s.reasoning_tokens = n("reasoning_output_tokens");
    }

    /// `rate_limits` on token counts: how full each usage window is.
    /// Only changes are kept, so a long session stores a few readings.
    fn rate_limits(&mut self, p: &Value, ts: Option<String>) {
        let Some(rl) = p.get("rate_limits").filter(|v| v.is_object()) else {
            return;
        };
        let plan = str_of(rl, "plan_type").map(String::from);
        // Names the window that was reached (`primary`, `secondary`, ...).
        let reached = str_of(rl, "rate_limit_reached_type").unwrap_or("");
        for key in ["primary", "secondary"] {
            let Some(w) = rl.get(key).filter(|v| v.is_object()) else {
                continue;
            };
            let minutes = w
                .get("window_minutes")
                .and_then(Value::as_u64)
                .map(|m| m as u32);
            let used = w.get("used_percent").and_then(Value::as_f64);
            let sample = LimitSample {
                timestamp: ts.clone(),
                seq: None,
                window: window_label(minutes, key),
                window_minutes: minutes,
                used_percent: used,
                resets_at: w
                    .get("resets_at")
                    .and_then(Value::as_i64)
                    .map(|s| crate::util::format_ts_ms(s * 1000)),
                resets_text: None,
                hit: reached.contains(key) || used.is_some_and(|u| u >= 100.0),
                plan: plan.clone(),
            };
            let same = self
                .out
                .limits
                .iter()
                .rev()
                .find(|l| l.window == sample.window)
                .is_some_and(|l| {
                    l.used_percent == sample.used_percent
                        && l.resets_at == sample.resets_at
                        && l.hit == sample.hit
                });
            if !same {
                self.out.limits.push(sample);
            }
        }
    }

    fn item(&mut self, item: &Value, offset: u64, ts: Option<String>) {
        let kind = str_of(item, "type").unwrap_or("");
        match kind {
            "UserMessage" => {
                let text = text_of(item.get("content").unwrap_or(&Value::Null));
                push_text(&mut self.items, EventKind::Prompt, &text, offset, ts);
            }
            "AgentMessage" => {
                let text = text_of(item.get("content").unwrap_or(&Value::Null));
                push_text(&mut self.items, EventKind::Response, &text, offset, ts);
            }
            "Reasoning" => {
                let text = summary_text(item.get("summary_text"));
                push_text(&mut self.items, EventKind::Thinking, &text, offset, ts);
            }
            "CommandExecution" => {
                let cmd = command_text(item.get("command"));
                let mut e = tool(ToolCategory::Command, "exec_command", &cmd, offset, ts);
                e.body = truncate(str_of(item, "aggregated_output").unwrap_or(""), MAX_TEXT);
                e.is_error = exit_code(item).is_some_and(|c| c != 0) || failed(item);
                e.duration_ms = rust_duration(item.get("duration"));
                e.input = Some(item.get("command").cloned().unwrap_or(Value::Null));
                let idx = self.items.len();
                let cwd = str_of(item, "cwd")
                    .map(|c| c.strip_prefix("file://").unwrap_or(c).to_string())
                    .or_else(|| self.out.session.project_path.clone());
                for (path, action) in crate::shell::command_touches(&cmd, cwd.as_deref()) {
                    self.item_touches.push((idx, touch(&path, action, None)));
                }
                for pc in item
                    .get("parsed_cmd")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if str_of(pc, "type") == Some("read")
                        && let Some(path) = str_of(pc, "path").or_else(|| str_of(pc, "name"))
                    {
                        self.item_touches
                            .push((idx, touch(path, FileAction::Read, None)));
                    }
                }
                self.items.push(e);
            }
            "FileChange" => {
                let idx = self.items.len();
                let mut paths = Vec::new();
                if let Some(changes) = item.get("changes").and_then(Value::as_object) {
                    for (path, c) in changes {
                        let (action, diff) = match str_of(c, "type") {
                            Some("add") => (FileAction::Create, str_of(c, "content")),
                            Some("delete") => (FileAction::Delete, None),
                            _ => (FileAction::Edit, str_of(c, "unified_diff")),
                        };
                        self.item_touches.push((idx, touch(path, action, diff)));
                        paths.push(path.clone());
                    }
                }
                let title = match paths.len() {
                    0 => "file change".to_string(),
                    1 => paths[0].clone(),
                    n => format!("{} (+{} more)", paths[0], n - 1),
                };
                let mut e = tool(ToolCategory::Edit, "apply_patch", &title, offset, ts);
                e.body = truncate(str_of(item, "stdout").unwrap_or(""), MAX_TEXT);
                e.is_error = failed(item);
                e.input = item.get("changes").cloned();
                self.items.push(e);
            }
            "McpToolCall" => {
                let server = str_of(item, "server").unwrap_or("mcp");
                let name = str_of(item, "tool").unwrap_or("tool");
                let full = format!("{server}.{name}");
                let mut e = tool(ToolCategory::Mcp, &full, &full, offset, ts);
                e.body = truncate(&value_text(item.get("result")), MAX_TEXT);
                e.is_error = failed(item);
                e.duration_ms = rust_duration(item.get("duration"));
                e.input = item.get("arguments").cloned();
                e.via = str_of(item, "appName")
                    .or_else(|| str_of(item, "server"))
                    .map(|a| format!("app:{a}"));
                self.items.push(e);
            }
            "Extension" => {
                let ext = str_of(item, "kind").unwrap_or("extension");
                let (cat, title) = match ext {
                    "web.search" => (ToolCategory::Web, str_of(item, "query").unwrap_or(ext)),
                    _ => (ToolCategory::Other, ext),
                };
                let mut e = tool(cat, ext, title, offset, ts);
                e.body = truncate(&value_text(item.get("results")), MAX_TEXT);
                e.input = item.get("action").cloned();
                self.items.push(e);
            }
            "ImageView" => {
                let path = str_of(item, "path").unwrap_or("");
                let path = path.strip_prefix("file://").unwrap_or(path);
                let idx = self.items.len();
                self.item_touches
                    .push((idx, touch(path, FileAction::Read, None)));
                self.items
                    .push(tool(ToolCategory::Read, "view_image", path, offset, ts));
            }
            _ => {}
        }
    }

    fn legacy_call(&mut self, p: &Value, offset: u64, ts: Option<String>) {
        let name = str_of(p, "name").unwrap_or("shell");
        // Function calls carry JSON-encoded `arguments`; custom tool calls
        // carry a raw `input` string.
        let args: Value = str_of(p, "arguments")
            .and_then(|a| serde_json::from_str(a).ok())
            .or_else(|| p.get("action").cloned())
            .unwrap_or_else(|| p.get("input").cloned().unwrap_or(Value::Null));
        let raw_input = str_of(p, "input").unwrap_or("");

        let idx = self.legacy.len();
        let (cat, title) = match name {
            "shell" | "exec_command" | "local_shell" | "container.exec" => {
                let cmd = command_text(args.get("command").or_else(|| args.get("cmd")));
                let cwd = str_of(&args, "workdir")
                    .map(String::from)
                    .or_else(|| self.out.session.project_path.clone());
                for (path, action) in crate::shell::command_touches(&cmd, cwd.as_deref()) {
                    self.legacy_touches.push((idx, touch(&path, action, None)));
                }
                (ToolCategory::Command, cmd)
            }
            "apply_patch" => {
                let patch = if raw_input.is_empty() {
                    str_of(&args, "input").unwrap_or("")
                } else {
                    raw_input
                };
                let files = patch_files(patch);
                for (path, action) in &files {
                    self.legacy_touches
                        .push((idx, touch(path, *action, Some(patch))));
                }
                let title = files
                    .first()
                    .map(|f| f.0.clone())
                    .unwrap_or_else(|| name.into());
                (ToolCategory::Edit, title)
            }
            "web_search" => (
                ToolCategory::Web,
                str_of(&args, "query").unwrap_or(name).into(),
            ),
            _ if str_of(p, "namespace").is_some_and(|n| n.starts_with("mcp")) => {
                (ToolCategory::Mcp, name.to_string())
            }
            _ if !raw_input.is_empty() => (ToolCategory::Other, first_line(raw_input, 300)),
            _ => (ToolCategory::Other, name.to_string()),
        };
        let mut e = tool(cat, name, &title, offset, ts);
        e.input = Some(args);
        e.call_id = str_of(p, "call_id").map(String::from);
        if let Some(id) = e.call_id.clone() {
            self.pending.insert(id, idx);
        }
        self.legacy.push(e);
    }

    fn legacy_output(&mut self, p: &Value) {
        let Some(idx) = str_of(p, "call_id").and_then(|id| self.pending.remove(id)) else {
            return;
        };
        let out = p.get("output").unwrap_or(&Value::Null);
        let text = match out {
            // Function call outputs are often a JSON string `{"output": ...}`.
            Value::String(s) => serde_json::from_str::<Value>(s)
                .ok()
                .and_then(|v| str_of(&v, "output").map(String::from))
                .unwrap_or_else(|| s.clone()),
            other => text_of(other),
        };
        self.legacy[idx].body = truncate(&text, MAX_TEXT);
    }
}

fn push_text(list: &mut Vec<Event>, kind: EventKind, text: &str, offset: u64, ts: Option<String>) {
    if text.trim().is_empty() {
        return;
    }
    let mut e = Event::new(kind, offset, ts);
    e.title = first_line(text, 200);
    e.body = truncate(text, MAX_TEXT);
    list.push(e);
}

fn tool(cat: ToolCategory, name: &str, title: &str, offset: u64, ts: Option<String>) -> Event {
    let mut e = Event::new(EventKind::ToolCall, offset, ts);
    e.tool_name = Some(name.to_string());
    e.category = Some(cat);
    e.title = first_line(title, 300);
    e
}

fn touch(path: &str, action: FileAction, diff: Option<&str>) -> FileTouch {
    FileTouch {
        seq: 0,
        path: path.to_string(),
        action,
        diff: diff.map(|d| truncate(d, MAX_TEXT)),
    }
}

/// `["/bin/zsh", "-lc", "ls -la"]` -> `ls -la`; a plain string stays as is.
fn command_text(cmd: Option<&Value>) -> String {
    match cmd {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            match parts.as_slice() {
                [_, flag, script] if flag.starts_with('-') && flag.ends_with('c') => {
                    script.to_string()
                }
                _ => parts.join(" "),
            }
        }
        _ => String::new(),
    }
}

/// Reasoning summaries: a list of strings, or of `{text}` blocks.
fn summary_text(v: Option<&Value>) -> String {
    let Some(Value::Array(parts)) = v else {
        return String::new();
    };
    parts
        .iter()
        .filter_map(|p| p.as_str().or_else(|| str_of(p, "text")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn exit_code(item: &Value) -> Option<i64> {
    match item.get("exit_code")? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn failed(item: &Value) -> bool {
    matches!(
        str_of(item, "status"),
        Some("failed" | "declined" | "error")
    )
}

/// Rust `Duration` serialized as `{secs, nanos}`.
fn rust_duration(v: Option<&Value>) -> Option<u64> {
    let v = v?;
    let secs = v.get("secs")?.as_u64()?;
    let nanos = v.get("nanos").and_then(Value::as_u64).unwrap_or(0);
    Some(secs * 1000 + nanos / 1_000_000)
}

/// Text from a tool result that may be a string, content blocks or JSON.
fn value_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => {
            if let Some(c) = v.get("content") {
                let t = text_of(c);
                if !t.is_empty() {
                    return t;
                }
            }
            let t = text_of(v);
            if t.is_empty() { v.to_string() } else { t }
        }
    }
}

/// Files named in an `apply_patch` body.
fn patch_files(patch: &str) -> Vec<(String, FileAction)> {
    patch
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if let Some(p) = l.strip_prefix("*** Add File: ") {
                Some((p.to_string(), FileAction::Create))
            } else if let Some(p) = l.strip_prefix("*** Update File: ") {
                Some((p.to_string(), FileAction::Edit))
            } else {
                l.strip_prefix("*** Delete File: ")
                    .map(|p| (p.to_string(), FileAction::Delete))
            }
        })
        .collect()
}

/// A turn per prompt, with the settings of the last `turn_context` before
/// the next prompt and the last completion inside it.
fn turns_from(events: &[Event], contexts: &[(String, Turn)], completes: &[String]) -> Vec<Turn> {
    let starts: Vec<(usize, &str)> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == EventKind::Prompt)
        .map(|(i, e)| (i, e.timestamp.as_deref().unwrap_or("")))
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(k, &(idx, start))| {
            let next = starts.get(k + 1).map(|s| s.1);
            let before_next = |t: &str| next.is_none_or(|n| t < n);
            let mut turn = contexts
                .iter()
                .rev()
                .find(|(t, _)| before_next(t))
                .map(|(_, c)| c.clone())
                .unwrap_or_default();
            turn.seq = idx as u32;
            turn.started_at = (!start.is_empty()).then(|| start.to_string());
            turn.ended_at = completes
                .iter()
                .filter(|t| t.as_str() >= start && before_next(t))
                .max()
                .cloned();
            turn
        })
        .collect()
}

/// `300` minutes is the 5-hour window, `10080` the weekly one.
fn window_label(minutes: Option<u32>, key: &str) -> String {
    match minutes {
        Some(300) => "5h".into(),
        Some(10080) => "weekly".into(),
        Some(m) if m % 1440 == 0 => format!("{}d", m / 1440),
        Some(m) if m % 60 == 0 => format!("{}h", m / 60),
        Some(m) => format!("{m}m"),
        None => key.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn command_text_unwraps_shell() {
        assert_eq!(
            command_text(Some(&json!(["/bin/zsh", "-lc", "ls -la"]))),
            "ls -la"
        );
        assert_eq!(command_text(Some(&json!(["git", "status"]))), "git status");
        assert_eq!(command_text(Some(&json!("pwd"))), "pwd");
    }

    #[test]
    fn patch_files_finds_all_actions() {
        let p = "*** Begin Patch\n*** Add File: a.txt\n+hi\n*** Update File: b.rs\n*** Delete File: c.md\n*** End Patch";
        let f = patch_files(p);
        assert_eq!(f.len(), 3);
        assert_eq!(f[0], ("a.txt".into(), FileAction::Create));
        assert_eq!(f[2], ("c.md".into(), FileAction::Delete));
    }
}
