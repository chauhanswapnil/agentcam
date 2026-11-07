//! What may leave this machine: the metadata a team server would receive.
//!
//! This module is the contract. Everything a sync sends is built here from
//! [`SessionMeta`], and nothing else. It never includes prompts, replies,
//! thinking, command text or output, tool inputs, file contents or diffs.
//! File paths are sent relative to the project; paths outside it are cut to
//! their file name. Tests below fail if a content field slips in.

use std::collections::BTreeMap;

use rusqlite::{Connection, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::index::Result;
use crate::query::{Filter, conditions, severity_name};

/// Bump when the fields below change; the server checks it.
pub const SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub parent_id: Option<String>,
    /// Changes whenever the session is re-indexed with new data.
    pub version: String,
    pub machine_id: String,
    pub os_user: Option<String>,
    pub agent: String,
    pub agent_version: Option<String>,
    pub model: Option<String>,
    /// `host/owner/repo` from the git remote, when there is one.
    pub project_remote: Option<String>,
    /// Folder name of the project root (never the full local path).
    pub project_name: Option<String>,
    pub branch: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub active_ms: i64,
    pub retry_ms: i64,
    pub steps: BTreeMap<String, i64>,
    pub tool_calls: BTreeMap<String, i64>,
    pub errored_steps: i64,
    pub tokens: Tokens,
    pub est_cost_usd: Option<f64>,
    pub peak_context: i64,
    pub findings: Vec<FindingMeta>,
    pub commits: Vec<CommitMeta>,
    pub files: Vec<FileMeta>,
    /// Prompts typed (turns). Counts only.
    #[serde(default)]
    pub turns: i64,
    /// Lines this session and its subagents added, and where they are now.
    /// Only on top-level sessions.
    #[serde(default)]
    pub lasted: Option<LastedMeta>,
    /// How the session followed the project's AGENTS.md / CLAUDE.md rules.
    /// Only on top-level sessions with checkable rules.
    #[serde(default)]
    pub follow: Option<FollowMeta>,
    #[serde(default)]
    pub prs: Vec<PrMeta>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LastedMeta {
    pub added: i64,
    pub kept: i64,
    pub reworked: i64,
    pub changed: i64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FollowMeta {
    pub followed: i64,
    pub not_followed: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrMeta {
    pub number: Option<i64>,
    /// `owner/repo` as the PR link names it.
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tokens {
    /// Input tokens not read from cache.
    pub fresh_input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingMeta {
    pub rule_id: String,
    pub severity: String,
    pub step: i64,
    /// Already masked for secrets at index time.
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMeta {
    pub step: i64,
    pub hash: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub path: String,
    pub action: String,
}

/// A path as it may be shared: relative to the project root, or just the
/// file name when it's outside the project (no home folders or usernames).
pub fn share_path(path: &str, root: Option<&str>) -> String {
    if let Some(root) = root.filter(|r| !r.is_empty())
        && let Some(rel) = path.strip_prefix(root).and_then(|r| r.strip_prefix('/'))
    {
        return rel.to_string();
    }
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    format!("(outside project)/{name}")
}

/// Free text as it may be shared (finding excerpts): paths inside the
/// project become relative, and home folders become `~`, so no username
/// or local layout leaves the machine.
pub fn share_text(text: &str, root: Option<&str>) -> String {
    let mut t = text.to_string();
    if let Some(root) = root.filter(|r| r.len() > 1) {
        t = t.replace(&format!("{root}/"), "").replace(root, ".");
    }
    let home = regex::Regex::new(r"(?:/Users|/home)/[^/\s'\x22]+|[A-Za-z]:\\Users\\[^\\\s'\x22]+")
        .expect("valid regex");
    home.replace_all(&t, "~").into_owned()
}

/// Metadata for top-level sessions with activity under `f` and their
/// subagents, newest first.
pub fn outbox(conn: &Connection, f: &Filter, limit: u32) -> Result<Vec<SessionMeta>> {
    let (cond, args) = conditions(f);
    let wh = format!(
        "s.id IN (SELECT DISTINCT e.session_id FROM events e JOIN sessions s ON s.id = e.session_id WHERE {cond})
         ORDER BY s.ended_at DESC LIMIT {limit}"
    );
    load(conn, &wh, args)
}

/// Every session's id and version, for a sync to find what changed.
pub fn versions(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT id, version FROM sessions ORDER BY ended_at")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}

/// Metadata for the given sessions, in the order given (unknown ids are
/// skipped).
pub fn metas(conn: &Connection, ids: &[String]) -> Result<Vec<SessionMeta>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; ids.len()].join(",");
    let mut out = load(conn, &format!("s.id IN ({marks})"), ids.to_vec())?;
    let pos: std::collections::HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    out.sort_by_key(|m| {
        pos.get(m.session_id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    Ok(out)
}

fn load(conn: &Connection, wh: &str, args: Vec<String>) -> Result<Vec<SessionMeta>> {
    let sql = format!(
        "SELECT s.id, s.parent_id, s.version, s.machine_id, s.os_user, s.agent, s.agent_version, s.model,
                s.project_remote, s.project_root, s.git_branch, s.started_at, s.ended_at, s.active_ms,
                s.wasted_ms, s.error_count, s.input_tokens, s.output_tokens, s.cache_read_tokens,
                s.cache_write_tokens, s.est_cost_usd, s.peak_context, s.turn_count
         FROM sessions s
         WHERE {wh}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut out: Vec<(SessionMeta, Option<String>)> = stmt
        .query_map(params_from_iter(&args), |r| {
            let root: Option<String> = r.get(9)?;
            Ok((
                SessionMeta {
                    session_id: r.get(0)?,
                    parent_id: r.get(1)?,
                    version: r.get(2)?,
                    machine_id: r.get(3)?,
                    os_user: r.get(4)?,
                    agent: r.get(5)?,
                    agent_version: r.get(6)?,
                    model: r.get(7)?,
                    project_remote: r.get(8)?,
                    project_name: root.as_deref().map(crate::query_basename),
                    branch: r.get(10)?,
                    started_at: r.get(11)?,
                    ended_at: r.get(12)?,
                    active_ms: r.get(13)?,
                    retry_ms: r.get(14)?,
                    errored_steps: r.get(15)?,
                    tokens: Tokens {
                        fresh_input: r.get(16)?,
                        output: r.get(17)?,
                        cache_read: r.get(18)?,
                        cache_write: r.get(19)?,
                    },
                    est_cost_usd: r.get(20)?,
                    peak_context: r.get(21)?,
                    turns: r.get::<_, Option<i64>>(22)?.unwrap_or(0),
                    steps: BTreeMap::new(),
                    tool_calls: BTreeMap::new(),
                    findings: Vec::new(),
                    commits: Vec::new(),
                    files: Vec::new(),
                    lasted: None,
                    follow: None,
                    prs: Vec::new(),
                },
                root,
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;

    let mut steps = conn.prepare(
        "SELECT kind, category, count(*) FROM events WHERE session_id = ? GROUP BY 1, 2",
    )?;
    let mut finds = conn.prepare(
        "SELECT rule_id, severity, seq, excerpt FROM findings WHERE session_id = ? ORDER BY seq",
    )?;
    let mut commits =
        conn.prepare("SELECT seq, hash, message FROM commits WHERE session_id = ? ORDER BY seq")?;
    let mut files = conn.prepare(
        "SELECT path, action FROM file_touches WHERE session_id = ? GROUP BY path, action ORDER BY min(seq)",
    )?;
    let mut prs = conn.prepare(
        "SELECT number, repo FROM prs WHERE session_id = ? GROUP BY url ORDER BY min(seq)",
    )?;
    for (m, root) in &mut out {
        let mut rows = steps.query([&m.session_id])?;
        while let Some(r) = rows.next()? {
            let (kind, cat, n): (String, Option<String>, i64) = (r.get(0)?, r.get(1)?, r.get(2)?);
            *m.steps.entry(kind).or_default() += n;
            if let Some(c) = cat {
                *m.tool_calls.entry(c).or_default() += n;
            }
        }
        m.findings = finds
            .query_map([&m.session_id], |r| {
                Ok(FindingMeta {
                    rule_id: r.get(0)?,
                    severity: severity_name(r.get(1)?).unwrap_or_default(),
                    step: r.get(2)?,
                    excerpt: share_text(&r.get::<_, String>(3)?, root.as_deref()),
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        m.commits = commits
            .query_map([&m.session_id], |r| {
                Ok(CommitMeta {
                    step: r.get(0)?,
                    hash: r.get(1)?,
                    message: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        m.files = files
            .query_map([&m.session_id], |r| {
                let path: String = r.get(0)?;
                Ok(FileMeta {
                    path: share_path(&path, root.as_deref()),
                    action: r.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        m.prs = prs
            .query_map([&m.session_id], |r| {
                Ok(PrMeta {
                    number: r.get(0)?,
                    repo: r.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        if m.parent_id.is_none() {
            m.lasted = crate::survival::session_counts(conn, &m.session_id)?.map(|c| LastedMeta {
                added: c.added,
                kept: c.kept,
                reworked: c.reworked,
                changed: c.changed,
            });
            m.follow = crate::follow::session(conn, &m.session_id)?.and_then(|f| {
                let n = |s: &str| f.outcomes.iter().filter(|o| o.status == s).count() as i64;
                let fm = FollowMeta {
                    followed: n("followed"),
                    not_followed: n("not_followed"),
                };
                (fm.followed + fm.not_followed > 0).then_some(fm)
            });
        }
    }
    Ok(out.into_iter().map(|(m, _)| m).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpts_lose_home_folders() {
        let root = Some("/Users/me/app");
        assert_eq!(share_text("/Users/me/app/src/.env", root), "src/.env");
        assert_eq!(
            share_text("/Users/me/Desktop/shots/a.png", root),
            "~/Desktop/shots/a.png"
        );
        assert_eq!(
            share_text("cat /home/dev/.ssh/id_rsa", None),
            "cat ~/.ssh/id_rsa"
        );
        assert_eq!(
            share_text(r"C:\Users\Dev\secrets.txt", None),
            r"~\secrets.txt"
        );
        assert_eq!(share_text("git push -f", root), "git push -f");
    }

    #[test]
    fn paths_never_leave_the_project() {
        assert_eq!(
            share_path("/Users/me/app/src/a.ts", Some("/Users/me/app")),
            "src/a.ts"
        );
        assert_eq!(
            share_path("/Users/me/.ssh/id_ed25519", Some("/Users/me/app")),
            "(outside project)/id_ed25519"
        );
        assert_eq!(
            share_path("/Users/me/app2/x", Some("/Users/me/app")),
            "(outside project)/x"
        );
        assert_eq!(share_path("rel/x.ts", None), "(outside project)/x.ts");
    }
}
