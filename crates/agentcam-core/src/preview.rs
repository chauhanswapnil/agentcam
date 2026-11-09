//! What draft rules would find, before they're saved.
//!
//! Sessions are rebuilt from the index (step titles, text cut to 32 KB,
//! inputs, file touches, turn settings) and checked with the draft rule
//! set. Only the rules the draft defines are reported, with a few example
//! steps each, so a rule that fires on ordinary work shows before it lands.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use crate::audit::Rules;
use crate::index::Result;
use crate::model::{
    Event, EventKind, FileAction, FileTouch, ParsedSession, Session, ToolCategory, Turn,
};

#[derive(Debug, Clone, Serialize)]
pub struct Example {
    pub session_id: String,
    pub seq: u32,
    /// What matched.
    pub excerpt: String,
    /// The step's command, path or URL, so a near miss is easy to spot.
    pub step: Option<String>,
    pub session_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RulePreview {
    pub rule_id: String,
    pub title: String,
    pub matches: usize,
    pub sessions: usize,
    pub examples: Vec<Example>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    /// Sessions checked: the most recent ones, up to the limit.
    pub sessions_checked: usize,
    pub days: i64,
    pub rules: Vec<RulePreview>,
}

fn kind(s: &str) -> EventKind {
    match s {
        "prompt" => EventKind::Prompt,
        "response" => EventKind::Response,
        "thinking" => EventKind::Thinking,
        "system" => EventKind::System,
        _ => EventKind::ToolCall,
    }
}

fn category(s: &str) -> Option<ToolCategory> {
    Some(match s {
        "command" => ToolCategory::Command,
        "read" => ToolCategory::Read,
        "edit" => ToolCategory::Edit,
        "write" => ToolCategory::Write,
        "search" => ToolCategory::Search,
        "web" => ToolCategory::Web,
        "mcp" => ToolCategory::Mcp,
        "subagent" => ToolCategory::Subagent,
        "other" => ToolCategory::Other,
        _ => return None,
    })
}

fn action(s: &str) -> FileAction {
    match s {
        "create" => FileAction::Create,
        "edit" => FileAction::Edit,
        "delete" => FileAction::Delete,
        _ => FileAction::Read,
    }
}

/// A session as the audit sees it, rebuilt from the index.
pub fn rebuild(conn: &Connection, id: &str) -> Result<ParsedSession> {
    let project_path: Option<String> = conn.query_row(
        "SELECT project_path FROM sessions WHERE id = ?",
        [id],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT seq, kind, tool_name, category, title, body, input, is_error, approval
         FROM events WHERE session_id = ? ORDER BY seq",
    )?;
    let events = stmt
        .query_map([id], |r| {
            let mut e = Event::new(kind(&r.get::<_, String>(1)?), 0, None);
            e.seq = r.get(0)?;
            e.tool_name = r.get(2)?;
            e.category = r.get::<_, Option<String>>(3)?.as_deref().and_then(category);
            e.title = r.get(4)?;
            e.body = r.get(5)?;
            e.input = r
                .get::<_, Option<String>>(6)?
                .and_then(|t| serde_json::from_str(&t).ok());
            e.is_error = r.get(7)?;
            e.approval = r.get(8)?;
            Ok(e)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut stmt =
        conn.prepare("SELECT seq, path, action FROM file_touches WHERE session_id = ?")?;
    let touches = stmt
        .query_map([id], |r| {
            Ok(FileTouch {
                seq: r.get(0)?,
                path: r.get(1)?,
                action: action(&r.get::<_, String>(2)?),
                diff: None,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT seq, mode, approval_policy, sandbox FROM turns WHERE session_id = ? ORDER BY seq",
    )?;
    let turns = stmt
        .query_map([id], |r| {
            Ok(Turn {
                seq: r.get(0)?,
                mode: r.get(1)?,
                approval_policy: r.get(2)?,
                sandbox: r.get(3)?,
                ..Default::default()
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ParsedSession {
        session: Session {
            id: id.to_string(),
            project_path,
            ..Default::default()
        },
        events,
        touches,
        turns,
        ..Default::default()
    })
}

/// Check the most recent `max_sessions` sessions from the last `days` with
/// `rules`, reporting only the rules in `only`.
pub fn preview(
    conn: &Connection,
    rules: &Rules,
    only: &[String],
    days: i64,
    max_sessions: usize,
) -> Result<Preview> {
    let since = crate::util::format_ts_ms(crate::util::now_ms() - days * 86_400_000);
    let mut stmt = conn.prepare(
        "SELECT id, coalesce(title, first_prompt) FROM sessions
         WHERE coalesce(ended_at, started_at) >= ?1 ORDER BY coalesce(ended_at, started_at) DESC LIMIT ?2",
    )?;
    let sessions: Vec<(String, Option<String>)> = stmt
        .query_map(rusqlite::params![since, max_sessions as i64], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let titles: BTreeMap<String, String> = rules
        .defs()
        .into_iter()
        .filter(|d| only.contains(&d.id))
        .map(|d| (d.id.clone(), d.title.clone()))
        .collect();
    let mut out: BTreeMap<String, RulePreview> = titles
        .iter()
        .map(|(id, title)| {
            (
                id.clone(),
                RulePreview {
                    rule_id: id.clone(),
                    title: title.clone(),
                    matches: 0,
                    sessions: 0,
                    examples: Vec::new(),
                },
            )
        })
        .collect();
    for (id, title) in &sessions {
        let p = rebuild(conn, id)?;
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for f in rules.check(&p) {
            let Some(r) = out.get_mut(&f.rule_id) else {
                continue;
            };
            r.matches += 1;
            if seen.insert(f.rule_id.clone()) {
                r.sessions += 1;
            }
            if r.examples.len() < 5 {
                let step = p
                    .events
                    .iter()
                    .find(|e| e.seq == f.seq)
                    .map(|e| crate::model::truncate(&e.title, 200))
                    .filter(|t| !t.is_empty());
                r.examples.push(Example {
                    session_id: id.clone(),
                    seq: f.seq,
                    excerpt: f.excerpt.clone(),
                    step,
                    session_title: title.clone(),
                });
            }
        }
    }
    let mut rules: Vec<RulePreview> = out.into_values().collect();
    rules.sort_by(|a, b| b.matches.cmp(&a.matches).then(a.rule_id.cmp(&b.rule_id)));
    Ok(Preview {
        sessions_checked: sessions.len(),
        days,
        rules,
    })
}

/// Rule ids a user's YAML defines (new rules or replacements).
pub fn defined_ids(user_yaml: &str) -> Vec<String> {
    serde_yaml_ng::from_str::<Option<Vec<crate::audit::RuleDef>>>(user_yaml)
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|d| d.id)
        .collect()
}
