//! Core of agentcam: finding agent logs, parsing them into one event
//! model, and indexing them in SQLite.

pub mod audit;
pub mod claude;
pub mod codex;
pub mod commits;
pub mod guard;
pub mod identity;
pub mod index;
pub mod model;
pub mod otel;
pub mod patterns;
pub mod prices;
pub mod query;
pub mod share;
pub mod shell;
pub mod util;

use std::path::{Path, PathBuf};

use serde::Serialize;

pub use index::{Index, default_db_path};
pub use model::ParsedSession;

/// A coding agent whose logs agentcam can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    ClaudeCode,
    Codex,
}

impl Agent {
    pub const ALL: [Agent; 2] = [Agent::ClaudeCode, Agent::Codex];

    /// Where this agent writes its session logs, relative to `home`.
    pub fn log_dir(self, home: &Path) -> PathBuf {
        match self {
            Agent::ClaudeCode => home.join(".claude").join("projects"),
            Agent::Codex => home.join(".codex").join("sessions"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude-code",
            Agent::Codex => "codex",
        }
    }
}

/// One agent's log folder and whether it exists on this machine.
#[derive(Debug, Clone, Serialize)]
pub struct LogSource {
    pub agent: Agent,
    pub path: PathBuf,
    pub exists: bool,
}

/// Look up the log folder for every known agent under `home`.
pub fn log_sources(home: &Path) -> Vec<LogSource> {
    Agent::ALL
        .into_iter()
        .map(|agent| {
            let path = agent.log_dir(home);
            let exists = path.is_dir();
            LogSource {
                agent,
                path,
                exists,
            }
        })
        .collect()
}

/// [`log_sources`] for the current user's home folder.
pub fn default_log_sources() -> Vec<LogSource> {
    dirs::home_dir()
        .map(|h| log_sources(&h))
        .unwrap_or_default()
}

/// Every session log file under `sources`, sorted by path.
pub fn discover(sources: &[LogSource]) -> Vec<(Agent, PathBuf)> {
    let mut out = Vec::new();
    for s in sources.iter().filter(|s| s.exists) {
        walk(&s.path, &mut |p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let wanted = match s.agent {
                // Under `subagents/`, only `agent-*.jsonl` are transcripts;
                // workflow `journal.jsonl` files are bookkeeping.
                Agent::ClaudeCode => {
                    name.ends_with(".jsonl")
                        && (name.starts_with("agent-")
                            || !p.components().any(|c| c.as_os_str() == "subagents"))
                }
                Agent::Codex => name.starts_with("rollout-") && name.ends_with(".jsonl"),
            };
            if wanted {
                out.push((s.agent, p.to_path_buf()));
            }
        });
    }
    out.sort();
    out
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        if ft.is_dir() {
            walk(&path, f);
        } else if ft.is_file() {
            f(&path);
        }
    }
}

/// The main checkout for a path inside an agent worktree, so work in
/// `repo/.claude/worktrees/x` counts toward `repo`.
pub fn project_root(path: &str) -> String {
    for marker in [
        "/.claude/worktrees/",
        "/.codex/worktrees/",
        "\\.claude\\worktrees\\",
    ] {
        if let Some(i) = path.find(marker) {
            return path[..i].to_string();
        }
    }
    path.trim_end_matches(['/', '\\']).to_string()
}

/// Last folder of a path, for showing a project by name.
pub fn query_basename(p: &str) -> String {
    p.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(p)
        .to_string()
}

/// Parse one log file with the right parser for `agent`.
pub fn parse_file(agent: Agent, path: &Path) -> std::io::Result<ParsedSession> {
    match agent {
        Agent::ClaudeCode => claude::parse(path),
        Agent::Codex => codex::parse(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_root_folds_worktrees() {
        assert_eq!(
            project_root("/Users/dev/app/.claude/worktrees/fix-a1"),
            "/Users/dev/app"
        );
        assert_eq!(project_root("/Users/dev/app/"), "/Users/dev/app");
        assert_eq!(project_root("/Users/dev/app"), "/Users/dev/app");
    }

    #[test]
    fn finds_existing_and_missing_log_dirs() {
        let home = std::env::temp_dir().join(format!("agentcam-test-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".claude/projects")).unwrap();

        let sources = log_sources(&home);
        let claude = sources
            .iter()
            .find(|s| s.agent == Agent::ClaudeCode)
            .unwrap();
        let codex = sources.iter().find(|s| s.agent == Agent::Codex).unwrap();

        assert!(claude.exists);
        assert!(!codex.exists);
        assert_eq!(codex.path, home.join(".codex/sessions"));

        std::fs::remove_dir_all(&home).unwrap();
    }
}
