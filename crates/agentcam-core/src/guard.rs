//! "Block next time": rules that a Claude Code `PreToolUse` hook checks
//! before the agent runs a tool. The hook runs `agentcam guard`, which
//! reads the call from stdin and exits 2 to block it.
//!
//! Guard rules live in `guard.yaml` next to the index. agentcam only
//! writes this file; it never edits the agent's own settings.

use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuardRule {
    pub id: String,
    /// Claude Code tool name (`Bash`, `Edit`, `Write`, `Read`, ...) or `*`.
    pub tool: String,
    /// Regex checked against the call's main field (see [`subject`]).
    pub pattern: String,
    /// Shown to the agent when it is blocked.
    pub reason: String,
    #[serde(default)]
    pub created: Option<String>,
    /// The step this rule was made from, if any.
    #[serde(default)]
    pub from_session: Option<String>,
    #[serde(default)]
    pub from_seq: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum GuardError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("guard.yaml: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    #[error("bad pattern: {0}")]
    Pattern(#[from] regex::Error),
}

pub fn guard_path(db: &Path) -> PathBuf {
    db.with_file_name("guard.yaml")
}

pub fn load(path: &Path) -> Result<Vec<GuardRule>, GuardError> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(serde_yaml_ng::from_str::<Option<Vec<GuardRule>>>(&t)?.unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

/// Write atomically: a hook may be reading the file at the same moment.
pub fn save(path: &Path, rules: &[GuardRule]) -> Result<(), GuardError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let header = "# agentcam guard rules: tool calls the Claude Code hook blocks.\n\
                  # Edit in the agentcam UI (Audit page) or by hand.\n";
    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(
        &tmp,
        format!("{header}{}", serde_yaml_ng::to_string(rules)?),
    )?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// The text a rule's pattern is checked against: the command for `Bash`,
/// the path for file tools, the URL for `WebFetch`, otherwise the input as
/// JSON.
pub fn subject(tool: &str, input: &Value) -> String {
    let s = |k: &str| input.get(k).and_then(Value::as_str).map(String::from);
    match tool {
        "Bash" => s("command"),
        "Read" | "Edit" | "MultiEdit" | "Write" => s("file_path"),
        "NotebookEdit" => s("notebook_path"),
        "WebFetch" => s("url"),
        "WebSearch" => s("query"),
        "Glob" | "Grep" => s("pattern"),
        _ => None,
    }
    .unwrap_or_else(|| input.to_string())
}

/// Check one rule up front so bad patterns are refused when saved.
pub fn validate(rule: &GuardRule) -> Result<(), GuardError> {
    Regex::new(&rule.pattern)?;
    Ok(())
}

/// The first rule that blocks this call. Rules with bad patterns are
/// skipped, never fatal: a broken rule must not stop the agent.
pub fn check<'a>(rules: &'a [GuardRule], tool: &str, input: &Value) -> Option<&'a GuardRule> {
    let text = subject(tool, input);
    rules.iter().find(|r| {
        (r.tool == "*" || r.tool == tool)
            && Regex::new(&r.pattern).is_ok_and(|re| re.is_match(&text))
    })
}

/// A starting pattern for blocking a step like this one: the command's
/// first words, or the exact path.
pub fn suggest(tool: &str, title: &str) -> String {
    let esc = |s: &str| regex::escape(s);
    if tool == "Bash" {
        let words: Vec<&str> = title.split_whitespace().take(5).collect();
        let joined = words
            .iter()
            .map(|w| esc(w))
            .collect::<Vec<_>>()
            .join(r"\s+");
        format!(r"(^|[;&|]\s*){joined}")
    } else {
        format!("^{}$", esc(title))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(tool: &str, pattern: &str) -> GuardRule {
        GuardRule {
            id: "g".into(),
            tool: tool.into(),
            pattern: pattern.into(),
            reason: "no".into(),
            created: None,
            from_session: None,
            from_seq: None,
        }
    }

    #[test]
    fn blocks_matching_calls_only() {
        let rules = vec![
            rule("Bash", r"\bgit\s+push\b.*\s(--force|-f)\b"),
            rule("Edit", r"/\.env$"),
        ];
        assert!(
            check(
                &rules,
                "Bash",
                &json!({"command": "git push -f origin main"})
            )
            .is_some()
        );
        assert!(check(&rules, "Bash", &json!({"command": "git push origin main"})).is_none());
        assert!(check(&rules, "Edit", &json!({"file_path": "/app/.env"})).is_some());
        assert!(
            check(&rules, "Read", &json!({"file_path": "/app/.env"})).is_none(),
            "tool must match"
        );
        assert!(check(&[rule("*", "secret")], "mcp__x__y", &json!({"q": "secret"})).is_some());
    }

    #[test]
    fn bad_patterns_never_block() {
        assert!(check(&[rule("Bash", "(")], "Bash", &json!({"command": "("})).is_none());
        assert!(validate(&rule("Bash", "(")).is_err());
    }

    #[test]
    fn suggestions_match_the_step() {
        let p = suggest("Bash", "git push -q -f");
        let rules = vec![rule("Bash", &p)];
        assert!(
            check(
                &rules,
                "Bash",
                &json!({"command": "git push -q origin main"})
            )
            .is_none()
        );
        assert!(
            check(
                &rules,
                "Bash",
                &json!({"command": "cd x && git push -q -f origin main"})
            )
            .is_some()
        );
        let p = suggest("Write", "/a/b (1).txt");
        assert!(Regex::new(&p).unwrap().is_match("/a/b (1).txt"));
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("agentcam-guard-{}", std::process::id()));
        let path = dir.join("guard.yaml");
        assert!(load(&path).unwrap().is_empty());
        let rules = vec![rule("Bash", r"rm\s+-rf\s+/")];
        save(&path, &rules).unwrap();
        assert_eq!(load(&path).unwrap(), rules);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
