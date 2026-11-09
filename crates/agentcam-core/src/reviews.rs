//! Findings someone has looked at, and rules they muted.
//!
//! A finding marked "looks fine" or a rule muted for a project drops out of
//! the Audit page's list of findings to review, so new ones stand out.
//! Kept in `reviews.json` next to the index rather than in it, so marks
//! survive the index being rebuilt.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Longest note kept with a mark.
const NOTE_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewedFinding {
    pub session_id: String,
    pub seq: i64,
    pub rule_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutedRule {
    pub rule_id: String,
    /// A project root, or `None` for every project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reviews {
    #[serde(default)]
    pub findings: Vec<ReviewedFinding>,
    #[serde(default)]
    pub muted: Vec<MutedRule>,
}

/// One change from the UI.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Change {
    Review {
        session_id: String,
        seq: i64,
        rule_id: String,
        note: Option<String>,
    },
    Unreview {
        session_id: String,
        seq: i64,
        rule_id: String,
    },
    Mute {
        rule_id: String,
        project: Option<String>,
        note: Option<String>,
    },
    Unmute {
        rule_id: String,
        project: Option<String>,
    },
}

pub fn path(db: &Path) -> PathBuf {
    db.with_file_name("reviews.json")
}

/// The saved marks; a missing or unreadable file means none.
pub fn load(path: &Path) -> Reviews {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Write the marks, replacing the file in one step.
pub fn save(path: &Path, r: &Reviews) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(r).unwrap_or_default())?;
    std::fs::rename(tmp, path)
}

fn note(n: Option<String>) -> Option<String> {
    n.map(|n| crate::model::truncate(n.trim(), NOTE_CHARS))
        .filter(|n| !n.is_empty())
}

impl Reviews {
    /// `reviewed`, `muted` or `None` for a finding in `project`.
    pub fn state(
        &self,
        session_id: &str,
        seq: i64,
        rule_id: &str,
        project: Option<&str>,
    ) -> Option<&'static str> {
        if self
            .findings
            .iter()
            .any(|f| f.session_id == session_id && f.seq == seq && f.rule_id == rule_id)
        {
            return Some("reviewed");
        }
        if self.muted.iter().any(|m| {
            m.rule_id == rule_id && (m.project.is_none() || m.project.as_deref() == project)
        }) {
            return Some("muted");
        }
        None
    }

    /// Whether a rule is muted everywhere.
    pub fn muted_everywhere(&self, rule_id: &str) -> bool {
        self.muted
            .iter()
            .any(|m| m.rule_id == rule_id && m.project.is_none())
    }

    pub fn apply(&mut self, change: Change) {
        let at = crate::util::format_ts_ms(crate::util::now_ms());
        match change {
            Change::Review {
                session_id,
                seq,
                rule_id,
                note: n,
            } => {
                self.findings.retain(|f| {
                    !(f.session_id == session_id && f.seq == seq && f.rule_id == rule_id)
                });
                self.findings.push(ReviewedFinding {
                    session_id,
                    seq,
                    rule_id,
                    note: note(n),
                    at,
                });
            }
            Change::Unreview {
                session_id,
                seq,
                rule_id,
            } => {
                self.findings.retain(|f| {
                    !(f.session_id == session_id && f.seq == seq && f.rule_id == rule_id)
                });
            }
            Change::Mute {
                rule_id,
                project,
                note: n,
            } => {
                self.muted
                    .retain(|m| !(m.rule_id == rule_id && m.project == project));
                self.muted.push(MutedRule {
                    rule_id,
                    project,
                    note: note(n),
                    at,
                });
            }
            Change::Unmute { rule_id, project } => {
                self.muted
                    .retain(|m| !(m.rule_id == rule_id && m.project == project));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_mute_and_undo() {
        let mut r = Reviews::default();
        r.apply(Change::Review {
            session_id: "s1".into(),
            seq: 4,
            rule_id: "rm-rf".into(),
            note: Some("  cleaning build output  ".into()),
        });
        assert_eq!(r.state("s1", 4, "rm-rf", None), Some("reviewed"));
        assert_eq!(r.findings[0].note.as_deref(), Some("cleaning build output"));
        assert_eq!(r.state("s1", 5, "rm-rf", None), None);

        r.apply(Change::Mute {
            rule_id: "package-install".into(),
            project: Some("/Users/dev/app".into()),
            note: None,
        });
        assert_eq!(
            r.state("s2", 1, "package-install", Some("/Users/dev/app")),
            Some("muted")
        );
        assert_eq!(
            r.state("s2", 1, "package-install", Some("/Users/dev/other")),
            None
        );
        assert!(!r.muted_everywhere("package-install"));

        r.apply(Change::Unreview {
            session_id: "s1".into(),
            seq: 4,
            rule_id: "rm-rf".into(),
        });
        r.apply(Change::Unmute {
            rule_id: "package-install".into(),
            project: Some("/Users/dev/app".into()),
        });
        assert_eq!(r, Reviews::default());
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("agentcam-reviews-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("reviews.json");
        assert_eq!(load(&p), Reviews::default(), "missing file: no marks");
        let mut r = Reviews::default();
        r.apply(Change::Mute {
            rule_id: "sudo".into(),
            project: None,
            note: None,
        });
        save(&p, &r).unwrap();
        assert!(load(&p).muted_everywhere("sudo"));
        std::fs::write(&p, "not json").unwrap();
        assert_eq!(load(&p), Reviews::default(), "a broken file reads as none");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
