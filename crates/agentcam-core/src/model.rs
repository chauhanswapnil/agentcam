//! The normalized event model every agent's logs are mapped into.

use serde::Serialize;
use serde_json::Value;

use crate::Agent;

/// Longest text kept for one event field in the index. Full text stays in
/// the source log and can be read back through `source_offset`.
pub const MAX_TEXT: usize = 32 * 1024;

/// One agent session: a single conversation from start to end.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Session {
    pub id: String,
    pub agent: Option<Agent>,
    pub source_path: String,
    /// The session that started this one, for subagents.
    pub parent_id: Option<String>,
    /// Working folder the agent ran in.
    pub project_path: Option<String>,
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub agent_version: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// Only set when the agent itself recorded a cost.
    pub cost_usd: Option<f64>,
    /// Reasoning tokens over the session, when the log says (Codex).
    pub reasoning_tokens: u64,
    /// Codex `service_tier` (`priority`, `flex`, ...).
    pub service_tier: Option<String>,
    /// Where the session was started from, as the agent names it
    /// (`cli`, `claude-desktop`, `vscode`, `Codex Desktop`, ...).
    pub entrypoint: Option<String>,
}

/// One turn: a prompt you typed and the work until the next one. Only the
/// start and settings are parsed; counts come from the steps in between.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Turn {
    /// Index of the prompt step that starts it.
    pub seq: u32,
    pub started_at: Option<String>,
    /// When the agent said the turn was over (Codex `task_complete`).
    pub ended_at: Option<String>,
    pub model: Option<String>,
    /// Reasoning effort: `low` ... `max`.
    pub effort: Option<String>,
    /// Claude Code permission mode (`default`, `acceptEdits`, `plan`,
    /// `auto`, ...) or Codex collaboration mode (`default`, `plan`).
    pub mode: Option<String>,
    /// Codex approval policy (`on-request`, `never`, ...).
    pub approval_policy: Option<String>,
    /// Codex sandbox (`workspace-write`, `read-only`, ...).
    pub sandbox: Option<String>,
}

/// A pull request a session linked to.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PrLink {
    pub seq: u32,
    pub timestamp: Option<String>,
    pub number: Option<i64>,
    pub url: String,
    pub repo: Option<String>,
}

/// A usage-window reading, or a limit the agent reported reaching.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LimitSample {
    pub timestamp: Option<String>,
    /// The step it arrived with, when the log ties it to one.
    pub seq: Option<u32>,
    /// `5h`, `weekly`, `session`, `spend`, ... as short labels.
    pub window: String,
    pub window_minutes: Option<u32>,
    pub used_percent: Option<f64>,
    /// When the window resets, if the log gives a time.
    pub resets_at: Option<String>,
    /// The reset as the agent wrote it, when it isn't a timestamp.
    pub resets_text: Option<String>,
    /// The limit was reached.
    pub hit: bool,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Prompt,
    Response,
    Thinking,
    /// A tool call, merged with its result once the result arrives.
    ToolCall,
    /// Errors and notices from the agent itself (not from a tool).
    System,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Prompt => "prompt",
            EventKind::Response => "response",
            EventKind::Thinking => "thinking",
            EventKind::ToolCall => "tool_call",
            EventKind::System => "system",
        }
    }
}

/// What a tool call does, independent of what each agent names its tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Command,
    Read,
    Edit,
    Write,
    Search,
    Web,
    Mcp,
    Subagent,
    Other,
}

impl ToolCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolCategory::Command => "command",
            ToolCategory::Read => "read",
            ToolCategory::Edit => "edit",
            ToolCategory::Write => "write",
            ToolCategory::Search => "search",
            ToolCategory::Web => "web",
            ToolCategory::Mcp => "mcp",
            ToolCategory::Subagent => "subagent",
            ToolCategory::Other => "other",
        }
    }
}

/// Tokens one model call used, attached to the first step it produced.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct TokenUsage {
    pub model: Option<String>,
    /// Fresh input tokens (not read from cache).
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Part of `output` spent on reasoning, when the log says (Codex).
    pub reasoning: u64,
}

impl TokenUsage {
    /// How full the context window was for this call.
    pub fn context(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }
}

/// One step in a session.
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub seq: u32,
    pub timestamp: Option<String>,
    pub kind: EventKind,
    pub tool_name: Option<String>,
    pub category: Option<ToolCategory>,
    /// One-line label: the command, the file path, the first line of text.
    pub title: String,
    /// Main text: prompt, response or thinking text; tool output.
    pub body: String,
    /// Tool input as the agent sent it.
    pub input: Option<Value>,
    pub is_error: bool,
    pub duration_ms: Option<u64>,
    /// Byte offset of the record in the source file that started this event.
    pub source_offset: u64,
    /// Agent's own id for the call, used to pair results.
    #[serde(skip)]
    pub call_id: Option<String>,
    /// Model tokens spent producing this step, when the log says.
    pub usage: Option<TokenUsage>,
    /// How a tool call was allowed or stopped: `rule`, `auto`, `mode`,
    /// `you`, `rejected`, `blocked-auto`, `blocked-rule`, `blocked-safety`.
    pub approval: Option<String>,
    /// What produced the step, when the agent says: `skill:name`,
    /// `plugin:name`, `app:name` (an MCP app).
    pub via: Option<String>,
}

impl Event {
    pub fn new(kind: EventKind, source_offset: u64, timestamp: Option<String>) -> Self {
        Event {
            seq: 0,
            timestamp,
            kind,
            tool_name: None,
            category: None,
            title: String::new(),
            body: String::new(),
            input: None,
            is_error: false,
            duration_ms: None,
            source_offset,
            call_id: None,
            usage: None,
            approval: None,
            via: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Read,
    Create,
    Edit,
    Delete,
}

impl FileAction {
    pub fn as_str(self) -> &'static str {
        match self {
            FileAction::Read => "read",
            FileAction::Create => "create",
            FileAction::Edit => "edit",
            FileAction::Delete => "delete",
        }
    }
}

/// A file a tool call read or changed.
#[derive(Debug, Clone, Serialize)]
pub struct FileTouch {
    /// `seq` of the tool call event.
    pub seq: u32,
    pub path: String,
    pub action: FileAction,
    /// Unified diff for edits, full content for creates, when the log has it.
    pub diff: Option<String>,
}

/// Everything parsed from one log file.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ParsedSession {
    pub session: Session,
    pub events: Vec<Event>,
    pub touches: Vec<FileTouch>,
    pub limits: Vec<LimitSample>,
    /// One per prompt you typed, with the settings the agent ran with.
    pub turns: Vec<Turn>,
    /// Pull requests the session opened or linked.
    pub prs: Vec<PrLink>,
    /// Lines that weren't valid JSON.
    pub bad_lines: u32,
}

impl ParsedSession {
    /// Number events in order, fill in first prompt and start/end times.
    pub fn finish(&mut self) {
        for (i, e) in self.events.iter_mut().enumerate() {
            e.seq = i as u32;
        }
        let s = &mut self.session;
        if s.first_prompt.is_none() {
            s.first_prompt = self
                .events
                .iter()
                .find(|e| e.kind == EventKind::Prompt)
                .map(|e| truncate(&e.body, 500));
        }
        let mut times = self.events.iter().filter_map(|e| e.timestamp.as_deref());
        if let Some(first) = times.next() {
            let (mut lo, mut hi) = (first, first);
            for t in times {
                lo = lo.min(t);
                hi = hi.max(t);
            }
            s.started_at.get_or_insert_with(|| lo.to_string());
            s.ended_at = Some(hi.to_string());
        }
    }
}

/// Cut `s` to at most `max` bytes on a char boundary, marking the cut.
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// First non-empty line of `s`, cut to `max` bytes.
pub fn first_line(s: &str, max: usize) -> String {
    let line = s
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    truncate(line, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("héllo", 2), "h…");
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn first_line_skips_blank_lines() {
        assert_eq!(first_line("\n\n  hello world \nnext", 100), "hello world");
    }
}
