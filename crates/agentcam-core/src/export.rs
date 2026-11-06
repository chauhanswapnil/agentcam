//! One session as Markdown or JSON, for saving or sharing by hand.
//!
//! Text comes from the index (cut to 32 KB per step) with every match of a
//! `secret: true` audit rule masked, so a key in a command or its output
//! doesn't travel with the file.

use std::fmt::Write as _;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::audit::Rules;
use crate::index::Result;
use crate::query::{SESSION_COLS, SessionRow, session_row};

/// Output lines kept per command in Markdown.
const OUTPUT_LINES: usize = 20;
const OUTPUT_CHARS: usize = 2000;

#[derive(Debug, Clone, Serialize)]
pub struct ExportTouch {
    pub path: String,
    pub action: String,
    pub diff: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportStep {
    pub seq: i64,
    pub ts: Option<String>,
    pub kind: String,
    pub tool_name: Option<String>,
    pub category: Option<String>,
    pub title: String,
    pub body: String,
    /// Tool input, parsed back to JSON when it is JSON.
    pub input: Option<serde_json::Value>,
    pub is_error: bool,
    pub duration_ms: Option<i64>,
    pub touches: Vec<ExportTouch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExport {
    pub exported_by: &'static str,
    pub session: SessionRow,
    /// The command that resumes this session, run in its folder.
    pub resume: Option<String>,
    pub steps: Vec<ExportStep>,
    pub subagents: Vec<SessionRow>,
}

/// The shell command that picks a session up again where it left off.
/// Subagents resume through their parent.
pub fn resume_command(agent: &str, id: &str, cwd: Option<&str>) -> Option<String> {
    let id = id.split('/').next()?;
    let cmd = match agent {
        "claude-code" => format!("claude --resume {id}"),
        "codex" => format!("codex resume {id}"),
        _ => return None,
    };
    Some(match cwd {
        Some(d) if !d.is_empty() => format!("cd {} && {cmd}", shell_quote(d)),
        _ => cmd,
    })
}

fn shell_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+@:".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Everything needed to write a session out, secrets masked.
pub fn session(conn: &Connection, id: &str, rules: &Rules) -> Result<Option<SessionExport>> {
    let Some(session) = conn
        .query_row(
            &format!("SELECT {SESSION_COLS} FROM sessions s WHERE s.id = ?"),
            [id],
            session_row,
        )
        .optional()?
    else {
        return Ok(None);
    };
    let m = |s: &str| rules.mask_secrets(s).into_owned();

    let mut touches = conn.prepare(
        "SELECT seq, path, action, diff FROM file_touches
         WHERE session_id = ? AND action != 'read' ORDER BY seq, rowid",
    )?;
    let mut by_seq: std::collections::HashMap<i64, Vec<ExportTouch>> = Default::default();
    for t in touches.query_map([id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            ExportTouch {
                path: r.get(1)?,
                action: r.get(2)?,
                diff: r.get::<_, Option<String>>(3)?,
            },
        ))
    })? {
        let (seq, mut t) = t?;
        t.diff = t.diff.as_deref().map(m);
        by_seq.entry(seq).or_default().push(t);
    }

    let mut stmt = conn.prepare(
        "SELECT seq, ts, kind, tool_name, category, title, body, input, is_error, duration_ms
         FROM events WHERE session_id = ? ORDER BY seq",
    )?;
    let steps = stmt
        .query_map([id], |r| {
            let seq: i64 = r.get(0)?;
            let input: Option<String> = r.get(7)?;
            let title: String = r.get(5)?;
            let body: String = r.get(6)?;
            Ok(ExportStep {
                seq,
                ts: r.get(1)?,
                kind: r.get(2)?,
                tool_name: r.get(3)?,
                category: r.get(4)?,
                title: m(&title),
                body: m(&body),
                input: input.map(|i| {
                    let i = m(&i);
                    serde_json::from_str(&i).unwrap_or(serde_json::Value::String(i))
                }),
                is_error: r.get(8)?,
                duration_ms: r.get(9)?,
                touches: Vec::new(),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .map(|mut s| {
            s.touches = by_seq.remove(&s.seq).unwrap_or_default();
            s
        })
        .collect();

    let mut subs = conn.prepare(&format!(
        "SELECT {SESSION_COLS} FROM sessions s WHERE s.parent_id = ? ORDER BY s.started_at"
    ))?;
    let subagents = subs
        .query_map([id], session_row)?
        .collect::<std::result::Result<_, _>>()?;

    let mut session = session;
    session.title = session.title.as_deref().map(m);
    session.first_prompt = session.first_prompt.as_deref().map(m);
    Ok(Some(SessionExport {
        exported_by: concat!("agentcam ", env!("CARGO_PKG_VERSION")),
        resume: resume_command(&session.agent, &session.id, session.project_path.as_deref()),
        session,
        steps,
        subagents,
    }))
}

fn fence(text: &str) -> &'static str {
    if text.contains("````") {
        "`````"
    } else if text.contains("```") {
        "````"
    } else {
        "```"
    }
}

fn block(out: &mut String, lang: &str, text: &str) {
    let f = fence(text);
    let _ = writeln!(out, "{f}{lang}\n{}\n{f}\n", text.trim_end());
}

/// The first lines of a command's output.
fn short(text: &str) -> String {
    let mut s: String = text
        .lines()
        .take(OUTPUT_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if s.len() > OUTPUT_CHARS {
        let mut cut = OUTPUT_CHARS;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    let left = text.lines().count().saturating_sub(OUTPUT_LINES);
    if left == 0 && s.len() < text.trim_end().len() {
        s.push_str("\n…");
    } else if left > 0 {
        let _ = write!(
            s,
            "\n… ({} more line{})",
            left,
            if left == 1 { "" } else { "s" }
        );
    }
    s
}

fn command_of(s: &ExportStep) -> Option<String> {
    let i = s.input.as_ref()?;
    let c = i.get("command").or_else(|| i.get("cmd"))?;
    match c {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(a) => Some(
            a.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

fn agent_label(a: &str) -> &str {
    match a {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        other => other,
    }
}

/// The session as a Markdown document: prompts, replies, commands with
/// the start of their output, and diffs. Thinking is left out.
pub fn markdown(x: &SessionExport) -> String {
    let s = &x.session;
    let mut out = String::new();
    let title = s
        .title
        .clone()
        .or_else(|| {
            s.first_prompt
                .as_deref()
                .map(|p| crate::model::first_line(p, 80))
        })
        .unwrap_or_else(|| "Untitled session".into());
    let _ = writeln!(out, "# {title}\n");
    let mut facts = vec![format!("- **Agent:** {}", agent_label(&s.agent))];
    if let Some(p) = &s.project_path {
        facts.push(format!("- **Folder:** `{p}`"));
    }
    if let Some(b) = &s.git_branch {
        facts.push(format!("- **Branch:** `{b}`"));
    }
    if let Some(m) = &s.model {
        facts.push(format!("- **Model:** {m}"));
    }
    if let Some(t) = &s.started_at {
        facts.push(format!("- **Started:** {t}"));
    }
    if let Some(t) = &s.ended_at {
        facts.push(format!("- **Last step:** {t}"));
    }
    facts.push(format!(
        "- **Steps:** {} ({} tool calls, {} errored)",
        s.event_count, s.tool_call_count, s.error_count
    ));
    if let Some(c) = s.est_cost_usd {
        facts.push(format!("- **Estimated cost:** ${c:.2}"));
    }
    if let Some(r) = &x.resume {
        facts.push(format!("- **Resume:** `{r}`"));
    }
    facts.push(format!("- **Session id:** `{}`", s.id));
    let _ = writeln!(out, "{}\n", facts.join("\n"));
    let _ = writeln!(
        out,
        "_Exported by {}. Secrets that match audit rules are masked; long text is cut to 32 KB._\n",
        x.exported_by
    );

    for st in &x.steps {
        match st.kind.as_str() {
            "prompt" => {
                let _ = writeln!(out, "## You\n");
                let _ = writeln!(
                    out,
                    "{}\n",
                    quote(if st.body.is_empty() {
                        &st.title
                    } else {
                        &st.body
                    })
                );
            }
            "response" => {
                let _ = writeln!(out, "## {}\n", agent_label(&s.agent));
                let _ = writeln!(
                    out,
                    "{}\n",
                    (if st.body.is_empty() {
                        &st.title
                    } else {
                        &st.body
                    })
                    .trim_end()
                );
            }
            "thinking" => {}
            "system" => {
                let _ = writeln!(
                    out,
                    "> **Note:** {}\n",
                    crate::model::first_line(&st.title, 200)
                );
            }
            _ => tool(&mut out, st),
        }
    }
    if !x.subagents.is_empty() {
        let _ = writeln!(out, "## Subagents\n");
        for sub in &x.subagents {
            let t = sub
                .title
                .clone()
                .or_else(|| {
                    sub.first_prompt
                        .as_deref()
                        .map(|p| crate::model::first_line(p, 80))
                })
                .unwrap_or_else(|| sub.id.clone());
            let _ = writeln!(out, "- {t} ({} steps) `{}`", sub.event_count, sub.id);
        }
        out.push('\n');
    }
    out
}

fn quote(text: &str) -> String {
    text.trim_end()
        .lines()
        .map(|l| {
            if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool(out: &mut String, st: &ExportStep) {
    let name = st.tool_name.as_deref().unwrap_or("Tool");
    let err = if st.is_error { " · errored" } else { "" };
    if st.category.as_deref() == Some("command")
        && let Some(cmd) = command_of(st)
    {
        let _ = writeln!(out, "**{name}**{err}\n");
        block(out, "sh", &cmd);
        if !st.body.trim().is_empty() {
            block(out, "text", &short(&st.body));
        }
    } else {
        let _ = writeln!(
            out,
            "**{name}**{err} · {}\n",
            crate::model::first_line(&st.title, 200)
        );
        if st.is_error && !st.body.trim().is_empty() {
            block(out, "text", &short(&st.body));
        }
    }
    for t in &st.touches {
        match &t.diff {
            Some(d) => {
                let _ = writeln!(out, "`{}` ({})\n", t.path, t.action);
                block(out, "diff", d);
            }
            None => {
                let _ = writeln!(out, "`{}` ({}, no diff in the log)\n", t.path, t.action);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_commands_run_in_the_session_folder() {
        assert_eq!(
            resume_command(
                "claude-code",
                "abc",
                Some("/Users/dev/app/.claude/worktrees/x")
            ),
            Some("cd /Users/dev/app/.claude/worktrees/x && claude --resume abc".into())
        );
        assert_eq!(
            resume_command("codex", "019a", Some("/Users/dev/my app")),
            Some("cd '/Users/dev/my app' && codex resume 019a".into())
        );
        // A subagent resumes through its parent.
        assert_eq!(
            resume_command("claude-code", "abc/agent-1", None),
            Some("claude --resume abc".into())
        );
        assert_eq!(resume_command("other", "x", None), None);
    }

    #[test]
    fn short_output_keeps_the_first_lines() {
        let long: String = (0..30).map(|i| format!("line {i}\n")).collect();
        let s = short(&long);
        assert!(s.starts_with("line 0\n") && s.contains("line 19"));
        assert!(!s.contains("line 20") && s.ends_with("(10 more lines)"));
        assert_eq!(short("ok\n"), "ok");
    }

    #[test]
    fn fences_grow_past_backticks_in_the_text() {
        assert_eq!(fence("plain"), "```");
        assert_eq!(fence("has ``` inside"), "````");
    }
}
