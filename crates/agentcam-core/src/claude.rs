//! Parser for Claude Code session logs:
//! `~/.claude/projects/<escaped-path>/<session-id>.jsonl`, plus subagent
//! logs under `<session-id>/subagents/**/agent-*.jsonl`.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde_json::Value;

use crate::Agent;
use crate::model::*;
use crate::util::{duration_ms, for_each_record, str_of, text_of};

#[derive(Default, Clone, Copy)]
struct Usage {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
}

#[derive(Default)]
struct State {
    out: ParsedSession,
    /// tool_use id -> index into `out.events`.
    pending: HashMap<String, usize>,
    /// Final usage per API message (one message spans several records).
    usage: HashMap<String, Usage>,
    /// API message id -> index of the first event it produced, and its model.
    msg_first: HashMap<String, (usize, Option<String>)>,
    custom_title: Option<String>,
    ai_title: Option<String>,
    summary: Option<String>,
    /// Permission mode on the record being read, for the turn it starts.
    mode: Option<String>,
}

/// Parse one Claude Code log file.
pub fn parse(path: &Path) -> io::Result<ParsedSession> {
    let mut st = State::default();
    let session = &mut st.out.session;
    session.agent = Some(Agent::ClaudeCode);
    session.source_path = path.to_string_lossy().into_owned();
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    match subagent_parent(path) {
        Some(parent) => {
            session.id = format!("{parent}/{stem}");
            session.parent_id = Some(parent);
            session.title = subagent_title(path);
        }
        None => session.id = stem,
    }

    st.out.bad_lines = for_each_record(path, |rec, offset| st.record(&rec, offset))?;

    let mut out = st.out;
    let s = &mut out.session;
    for u in st.usage.values() {
        s.input_tokens += u.input;
        s.output_tokens += u.output;
        s.cache_read_tokens += u.cache_read;
        s.cache_write_tokens += u.cache_write;
    }
    if s.title.is_none() {
        s.title = st.custom_title.or(st.ai_title).or(st.summary);
    }
    // Put each API call's tokens on the first step it produced.
    for (id, (idx, model)) in &st.msg_first {
        if let (Some(u), Some(ev)) = (st.usage.get(id), out.events.get_mut(*idx)) {
            ev.usage = Some(TokenUsage {
                model: model.clone(),
                input: u.input,
                output: u.output,
                cache_read: u.cache_read,
                cache_write: u.cache_write,
                reasoning: 0,
            });
        }
    }
    out.finish();
    Ok(out)
}

/// For `<project>/<session-id>/subagents/.../agent-x.jsonl`, the session id.
fn subagent_parent(path: &Path) -> Option<String> {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d.file_name()? == "subagents" {
            return Some(d.parent()?.file_name()?.to_string_lossy().into_owned());
        }
        dir = d.parent();
    }
    None
}

/// Subagents have a sibling `<name>.meta.json` with a description.
fn subagent_title(path: &Path) -> Option<String> {
    let meta = path.with_extension("meta.json");
    let v: Value = serde_json::from_slice(&std::fs::read(meta).ok()?).ok()?;
    str_of(&v, "description")
        .or_else(|| str_of(&v, "agentType"))
        .map(String::from)
}

impl State {
    fn record(&mut self, rec: &Value, offset: u64) {
        let ts = str_of(rec, "timestamp").map(String::from);
        let s = &mut self.out.session;
        if s.project_path.is_none() {
            s.project_path = str_of(rec, "cwd").map(String::from);
        }
        if let Some(b) = str_of(rec, "gitBranch") {
            s.git_branch = Some(b.to_string());
        }
        if s.agent_version.is_none() {
            s.agent_version = str_of(rec, "version").map(String::from);
        }
        if s.entrypoint.is_none() {
            s.entrypoint = str_of(rec, "entrypoint").map(String::from);
        }

        match str_of(rec, "type").unwrap_or("") {
            "user" => self.user(rec, offset, ts),
            "assistant" => self.assistant(rec, offset, ts),
            "system" => self.system(rec, offset, ts),
            "custom-title" => self.custom_title = str_of(rec, "customTitle").map(String::from),
            "ai-title" => self.ai_title = str_of(rec, "aiTitle").map(String::from),
            "summary" => self.summary = str_of(rec, "summary").map(String::from),
            "pr-link" => {
                if let Some(url) = str_of(rec, "prUrl") {
                    self.out.prs.push(PrLink {
                        seq: self.out.events.len().saturating_sub(1) as u32,
                        timestamp: ts,
                        number: rec.get("prNumber").and_then(Value::as_i64),
                        url: url.to_string(),
                        repo: str_of(rec, "prRepository").map(String::from),
                    });
                }
            }
            "attachment" => self.attachment(rec, offset, ts),
            "cost-state" => {
                if let Some(c) = rec.get("totalCostUSD").and_then(Value::as_f64) {
                    self.out.session.cost_usd = Some(c);
                }
            }
            _ => {}
        }
    }

    fn push(&mut self, e: Event) -> usize {
        self.out.events.push(e);
        self.out.events.len() - 1
    }

    fn user(&mut self, rec: &Value, offset: u64, ts: Option<String>) {
        if rec.get("isMeta").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let Some(content) = rec.get("message").and_then(|m| m.get("content")) else {
            return;
        };
        self.mode = str_of(rec, "permissionMode").map(String::from);

        if let Value::String(text) = content {
            self.prompt_text(text, offset, ts);
            return;
        }
        let Some(blocks) = content.as_array() else {
            return;
        };

        let mut prompt = Vec::new();
        for b in blocks {
            match str_of(b, "type") {
                Some("tool_result") => self.tool_result(b, rec, ts.as_deref()),
                Some("text") => prompt.push(text_of(&Value::Array(vec![b.clone()]))),
                Some("image") => prompt.push("[image]".to_string()),
                _ => {}
            }
        }
        if !prompt.is_empty() {
            self.prompt_text(&prompt.join("\n"), offset, ts);
        }
    }

    fn prompt_text(&mut self, text: &str, offset: u64, ts: Option<String>) {
        let t = text.trim_start();
        if t.starts_with("<local-command-caveat>") {
            return;
        }
        // Slash commands, their output and background notices are
        // recorded as user turns but aren't typed prompts.
        let system_like = t.starts_with("<local-command-stdout>")
            || t.starts_with("<local-command-stderr>")
            || t.starts_with("<task-notification>");
        let mut e = Event::new(
            if system_like {
                EventKind::System
            } else {
                EventKind::Prompt
            },
            offset,
            ts,
        );
        e.title = match tag(t, "command-name") {
            Some(cmd) => cmd.to_string(),
            None => first_line(t, 200),
        };
        e.body = truncate(text, MAX_TEXT);
        let started_at = e.timestamp.clone();
        let idx = self.push(e);
        if !system_like {
            self.out.turns.push(Turn {
                seq: idx as u32,
                started_at,
                mode: self.mode.clone(),
                ..Default::default()
            });
        }
    }

    fn tool_result(&mut self, block: &Value, rec: &Value, ts: Option<&str>) {
        let structured = rec.get("toolUseResult");
        let Some(id) = str_of(block, "tool_use_id") else {
            return;
        };
        let Some(idx) = self.pending.remove(id) else {
            return;
        };
        let ev = &mut self.out.events[idx];
        let content = block.get("content").unwrap_or(&Value::Null);
        let mut body = text_of(content);
        if body.is_empty() && content.to_string().contains(r#""type":"image""#) {
            body = "[image]".to_string();
        }
        ev.body = truncate(&body, MAX_TEXT);
        ev.is_error = block.get("is_error").and_then(Value::as_bool) == Some(true);
        ev.duration_ms = duration_ms(ev.timestamp.as_deref(), ts);
        ev.approval = approval(rec);
        let seq = idx as u32;
        // A call that was rejected or blocked never ran: it changed no
        // files, and the refusal isn't the agent's error.
        if ev
            .approval
            .as_deref()
            .is_some_and(|a| a == "rejected" || a.starts_with("blocked"))
        {
            ev.is_error = false;
            self.out.touches.retain(|t| t.seq != seq);
            return;
        }

        let Some(r) = structured.filter(|r| r.is_object()) else {
            return;
        };
        // Fill in diffs for file changes now that we have the result.
        if let Some(touch) = self.out.touches.iter_mut().rev().find(|t| t.seq == seq) {
            if let Some(patch) = r.get("structuredPatch").and_then(Value::as_array)
                && !patch.is_empty()
            {
                touch.diff = Some(truncate(&hunks_to_diff(patch), MAX_TEXT));
            } else if str_of(r, "type") == Some("create") {
                touch.action = FileAction::Create;
                touch.diff = str_of(r, "content").map(|c| truncate(c, MAX_TEXT));
            }
        }
    }

    fn assistant(&mut self, rec: &Value, offset: u64, ts: Option<String>) {
        let Some(msg) = rec.get("message") else {
            return;
        };
        if let Some(m) = str_of(msg, "model").filter(|m| !m.starts_with('<')) {
            self.out.session.model = Some(m.to_string());
        }
        if let (Some(id), Some(u)) = (str_of(msg, "id"), msg.get("usage")) {
            let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
            self.usage.insert(
                id.to_string(),
                Usage {
                    input: n("input_tokens"),
                    output: n("output_tokens"),
                    cache_read: n("cache_read_input_tokens"),
                    cache_write: n("cache_creation_input_tokens"),
                },
            );
        }
        let Some(blocks) = msg.get("content").and_then(Value::as_array) else {
            return;
        };
        // Claude Code writes "You've hit your session limit · resets 3pm
        // (...)" as an assistant message flagged as an API error.
        if rec.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) {
            let text = text_of(msg.get("content").unwrap_or(&Value::Null));
            let mut e = Event::new(EventKind::System, offset, ts.clone());
            e.title = first_line(&text, 200);
            e.body = truncate(&text, MAX_TEXT);
            let limit = str_of(rec, "error") == Some("rate_limit");
            e.is_error = !limit;
            self.push(e);
            if limit {
                self.out
                    .limits
                    .push(limit_hit(&text, ts, self.out.events.len() - 1));
            }
            return;
        }
        if let Some(turn) = self.out.turns.last_mut() {
            if turn.model.is_none() {
                turn.model = str_of(msg, "model")
                    .filter(|m| !m.starts_with('<'))
                    .map(String::from);
            }
            if turn.effort.is_none() {
                turn.effort = str_of(rec, "effort").map(String::from);
            }
        }
        let before = self.out.events.len();
        self.assistant_blocks(blocks, offset, ts, rec);
        let via = str_of(rec, "attributionSkill")
            .map(|s| format!("skill:{s}"))
            .or_else(|| str_of(rec, "attributionPlugin").map(|s| format!("plugin:{s}")));
        let thinking_ms = rec.get("thinkingDurationMs").and_then(Value::as_u64);
        for e in &mut self.out.events[before..] {
            if e.via.is_none() {
                e.via.clone_from(&via);
            }
            if e.kind == EventKind::Thinking && e.duration_ms.is_none() {
                e.duration_ms = thinking_ms;
            }
        }
        if let Some(id) = str_of(msg, "id")
            && self.out.events.len() > before
            && !self.msg_first.contains_key(id)
        {
            let model = str_of(msg, "model")
                .filter(|m| !m.starts_with('<'))
                .map(String::from);
            self.msg_first.insert(id.to_string(), (before, model));
        }
    }

    fn assistant_blocks(&mut self, blocks: &[Value], offset: u64, ts: Option<String>, rec: &Value) {
        for b in blocks {
            match str_of(b, "type") {
                Some("text") => {
                    let text = str_of(b, "text").unwrap_or("");
                    if text.trim().is_empty() {
                        continue;
                    }
                    let mut e = Event::new(EventKind::Response, offset, ts.clone());
                    e.title = first_line(text, 200);
                    e.body = truncate(text, MAX_TEXT);
                    self.push(e);
                }
                Some("thinking") => {
                    // Redacted thinking has an empty text and only a signature.
                    let Some(text) = str_of(b, "thinking") else {
                        continue;
                    };
                    let mut e = Event::new(EventKind::Thinking, offset, ts.clone());
                    e.title = first_line(text, 200);
                    e.body = truncate(text, MAX_TEXT);
                    self.push(e);
                }
                Some("tool_use") => self.tool_use(b, offset, ts.clone(), str_of(rec, "cwd")),
                _ => {}
            }
        }
    }

    fn tool_use(&mut self, b: &Value, offset: u64, ts: Option<String>, cwd: Option<&str>) {
        let name = str_of(b, "name").unwrap_or("unknown");
        let input = b.get("input").cloned().unwrap_or(Value::Null);
        let arg = |k: &str| str_of(&input, k).map(String::from);
        let (category, title, touch) = match name {
            "Bash" => (ToolCategory::Command, arg("command"), None),
            "Read" => (ToolCategory::Read, arg("file_path"), Some(FileAction::Read)),
            "Edit" | "MultiEdit" => (ToolCategory::Edit, arg("file_path"), Some(FileAction::Edit)),
            "NotebookEdit" => (
                ToolCategory::Edit,
                arg("notebook_path"),
                Some(FileAction::Edit),
            ),
            "Write" => (
                ToolCategory::Write,
                arg("file_path"),
                Some(FileAction::Edit),
            ),
            "Glob" | "Grep" | "LS" => (ToolCategory::Search, arg("pattern").or(arg("path")), None),
            "WebFetch" => (ToolCategory::Web, arg("url"), None),
            "WebSearch" => (ToolCategory::Web, arg("query"), None),
            "Task" | "Agent" => (ToolCategory::Subagent, arg("description"), None),
            n if n.starts_with("mcp__") => (ToolCategory::Mcp, None, None),
            _ => (ToolCategory::Other, None, None),
        };

        let mut e = Event::new(EventKind::ToolCall, offset, ts);
        e.title = first_line(&title.clone().unwrap_or_else(|| name.to_string()), 300);
        e.tool_name = Some(name.to_string());
        e.category = Some(category);
        e.call_id = str_of(b, "id").map(String::from);
        e.input = Some(input);
        let idx = self.push(e);
        if let Some(id) = self.out.events[idx].call_id.clone() {
            self.pending.insert(id, idx);
        }
        if category == ToolCategory::Command
            && let Some(cmd) = &title
        {
            let cwd = cwd.or(self.out.session.project_path.as_deref());
            for (path, action) in crate::shell::command_touches(cmd, cwd) {
                self.out.touches.push(FileTouch {
                    seq: idx as u32,
                    path,
                    action,
                    diff: None,
                });
            }
        }
        if let (Some(action), Some(path)) = (touch, title) {
            self.out.touches.push(FileTouch {
                seq: idx as u32,
                path,
                action,
                diff: None,
            });
        }
    }

    /// Attachments are context Claude Code adds to the conversation. One
    /// kind matters here: a file changed outside the session while it ran.
    fn attachment(&mut self, rec: &Value, offset: u64, ts: Option<String>) {
        let Some(a) = rec.get("attachment") else {
            return;
        };
        if str_of(a, "type") != Some("edited_text_file") {
            return;
        }
        let Some(path) = str_of(a, "filename") else {
            return;
        };
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let mut e = Event::new(EventKind::System, offset, ts);
        e.tool_name = Some("external_edit".into());
        e.title = format!("{name} changed outside the session");
        e.body = path.to_string();
        self.push(e);
    }

    fn system(&mut self, rec: &Value, offset: u64, ts: Option<String>) {
        if str_of(rec, "subtype") == Some("compact_boundary") {
            let mut e = Event::new(EventKind::System, offset, ts);
            e.title = "Context compacted".into();
            let meta = rec.get("compactMetadata");
            let pre = meta
                .and_then(|m| m.get("preTokens"))
                .and_then(Value::as_u64);
            let trigger = meta.and_then(|m| str_of(m, "trigger")).unwrap_or("auto");
            e.body = match pre {
                Some(n) => format!("{trigger} compaction at {n} tokens"),
                None => format!("{trigger} compaction"),
            };
            self.push(e);
            return;
        }
        let level = str_of(rec, "level").unwrap_or("");
        if level != "error" && level != "warning" {
            return;
        }
        let mut e = Event::new(EventKind::System, offset, ts);
        let detail = rec
            .get("error")
            .map(|v| {
                v.as_str()
                    .map(String::from)
                    .unwrap_or_else(|| v.to_string())
            })
            .or_else(|| str_of(rec, "content").map(String::from))
            .unwrap_or_default();
        e.title = str_of(rec, "subtype").unwrap_or(level).to_string();
        e.body = truncate(&detail, MAX_TEXT);
        e.is_error = level == "error";
        self.push(e);
    }
}

/// Text between `<name>` and `</name>`.
fn tag<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = s.find(&open)? + open.len();
    let end = s[start..].find(&format!("</{name}>"))? + start;
    Some(s[start..end].trim())
}

/// Turn Claude Code's `structuredPatch` hunks into unified diff text.
fn hunks_to_diff(hunks: &[Value]) -> String {
    let mut out = String::new();
    for h in hunks {
        let n = |k: &str| h.get(k).and_then(Value::as_u64).unwrap_or(0);
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            n("oldStart"),
            n("oldLines"),
            n("newStart"),
            n("newLines")
        ));
        for line in h
            .get("lines")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            out.push_str(line.as_str().unwrap_or(""));
            out.push('\n');
        }
    }
    out
}

/// How Claude Code let a tool call run or stopped it, from
/// `permissionDecision` and `toolDenialKind` on the tool result.
fn approval(rec: &Value) -> Option<String> {
    let label = match str_of(rec, "toolDenialKind") {
        Some("user-rejected") => "rejected",
        Some("automode-blocked" | "automode-unavailable") => "blocked-auto",
        Some("permission-rule") => "blocked-rule",
        Some(_) => "blocked-rule",
        None => {
            let d = rec.get("permissionDecision")?;
            let source = str_of(d, "source").unwrap_or("");
            let reason = str_of(d, "reasonType").unwrap_or("");
            match (str_of(d, "decision")?, source, reason) {
                ("accept", s, _) if s.starts_with("user") => "you",
                ("accept", _, "classifier") => "auto",
                ("accept", _, "rule" | "subcommandResults") => "rule",
                ("accept", _, "mode") => "mode",
                ("accept", _, _) => "config",
                (_, "user_abort", _) => "rejected",
                (_, s, _) if s.starts_with("user") => "rejected",
                (_, _, "classifier") => "blocked-auto",
                (_, _, "safetyCheck") => "blocked-safety",
                _ => "blocked-rule",
            }
        }
    };
    Some(label.to_string())
}

/// Read "You've hit your weekly limit · resets Nov 9, 3pm (Europe/London)".
fn limit_hit(text: &str, ts: Option<String>, idx: usize) -> LimitSample {
    let lower = text.to_lowercase();
    let window = lower
        .split_once("hit your ")
        .and_then(|(_, r)| r.split_once(" limit"))
        .map(|(w, _)| w.trim().replace(' ', "-"))
        .filter(|w| !w.is_empty() && w.len() <= 24)
        .unwrap_or_else(|| "usage".into());
    let resets_text = text
        .split_once("resets ")
        .map(|(_, r)| first_line(r, 80))
        .filter(|r| !r.is_empty());
    LimitSample {
        timestamp: ts,
        seq: Some(idx as u32),
        window,
        resets_text,
        hit: true,
        ..Default::default()
    }
}
