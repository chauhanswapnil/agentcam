//! Every prompt you typed, searchable, with the ones you keep typing
//! grouped. Repeats are worth a look: a prompt typed many times may belong
//! in a slash command, a skill or the project's AGENTS.md.

use std::collections::HashMap;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::index::Result;
use crate::query::{Filter, conditions};

#[derive(Debug, Clone, Serialize)]
pub struct PromptRow {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub text: String,
    pub agent: String,
    pub project_root: Option<String>,
    pub session_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Repeated {
    /// The latest wording.
    pub text: String,
    pub count: i64,
    pub sessions: i64,
    pub projects: i64,
    pub last_ts: Option<String>,
    pub session_id: String,
    pub seq: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub total: i64,
    pub prompts: Vec<PromptRow>,
    pub repeated: Vec<Repeated>,
}

/// The same request typed with different case, spacing or punctuation.
pub fn normalize(text: &str) -> Option<String> {
    let t = text.trim();
    // Injected records, not typed: command wrappers, interruptions.
    if t.starts_with('<') || t.starts_with("[Request interrupted") || t.starts_with("Caveat:") {
        return None;
    }
    let mut out = String::new();
    let mut space = false;
    for c in t.chars().take(400) {
        if c.is_alphanumeric() {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.extend(c.to_lowercase());
        } else {
            space = true;
        }
    }
    (out.len() >= 4).then_some(out)
}

/// Prompts under `f`, newest first, optionally containing `q`.
pub fn report(conn: &Connection, f: &Filter, q: Option<&str>, limit: u32) -> Result<Report> {
    let (cond, mut args) = conditions(f);
    let mut extra = String::new();
    if let Some(q) = q.map(str::trim).filter(|q| !q.is_empty()) {
        extra = " AND e.body LIKE ? ESCAPE '\\'".into();
        let esc = q
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        args.push(format!("%{esc}%"));
    }
    let sql = format!(
        "SELECT e.session_id, e.seq, e.ts, substr(e.body, 1, 2000), s.agent, s.project_root, s.title
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.kind = 'prompt' AND s.parent_id IS NULL AND {cond}{extra}
         ORDER BY e.ts DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let all: Vec<PromptRow> = stmt
        .query_map(params_from_iter(&args), |r| {
            Ok(PromptRow {
                session_id: r.get(0)?,
                seq: r.get(1)?,
                ts: r.get(2)?,
                text: r.get(3)?,
                agent: r.get(4)?,
                project_root: r.get(5)?,
                session_title: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    struct Acc {
        first: usize,
        count: i64,
        sessions: std::collections::HashSet<String>,
        projects: std::collections::HashSet<String>,
    }
    let mut groups: HashMap<String, Acc> = HashMap::new();
    let mut total = 0;
    for (i, p) in all.iter().enumerate() {
        let Some(key) = normalize(&p.text) else {
            continue;
        };
        total += 1;
        let a = groups.entry(key).or_insert_with(|| Acc {
            first: i,
            count: 0,
            sessions: Default::default(),
            projects: Default::default(),
        });
        a.count += 1;
        a.sessions.insert(p.session_id.clone());
        if let Some(r) = &p.project_root {
            a.projects.insert(r.clone());
        }
    }
    let mut repeated: Vec<Repeated> = groups
        .into_values()
        // Typed in at least two sessions: retries inside one turn aren't habits.
        .filter(|a| a.count >= 2 && a.sessions.len() >= 2)
        .map(|a| {
            let p = &all[a.first];
            Repeated {
                text: crate::model::truncate(&p.text, 400),
                count: a.count,
                sessions: a.sessions.len() as i64,
                projects: a.projects.len() as i64,
                last_ts: p.ts.clone(),
                session_id: p.session_id.clone(),
                seq: p.seq,
            }
        })
        .collect();
    repeated.sort_by(|a, b| b.count.cmp(&a.count).then(b.last_ts.cmp(&a.last_ts)));
    repeated.truncate(40);
    let prompts = all
        .into_iter()
        .filter(|p| normalize(&p.text).is_some())
        .take(limit as usize)
        .map(|mut p| {
            p.text = crate::model::truncate(&p.text, 600);
            p
        })
        .collect();
    Ok(Report {
        total,
        prompts,
        repeated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_request_different_spelling() {
        assert_eq!(
            normalize("Commit and push").as_deref(),
            Some("commit and push")
        );
        assert_eq!(
            normalize("  commit  and push!! ").as_deref(),
            Some("commit and push")
        );
        assert_eq!(normalize("<command-name>/clear</command-name>"), None);
        assert_eq!(normalize("ok"), None);
    }
}
