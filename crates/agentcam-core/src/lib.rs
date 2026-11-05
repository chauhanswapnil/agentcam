//! Core of agentcam: where agent logs live, and (soon) the parsers,
//! event model and index built from them.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// A coding agent whose logs agentcam can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
