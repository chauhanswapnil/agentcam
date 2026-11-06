//! Find the git commits a session made, from its `git commit` commands.

use serde::Serialize;

use crate::model::{EventKind, ParsedSession, ToolCategory};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Commit {
    pub seq: u32,
    /// Short hash, when git printed one (`-q` hides it).
    pub hash: Option<String>,
    pub branch: Option<String>,
    pub message: Option<String>,
}

pub fn detect(p: &ParsedSession) -> Vec<Commit> {
    let mut out = Vec::new();
    for e in &p.events {
        if e.kind != EventKind::ToolCall || e.category != Some(ToolCategory::Command) || e.is_error
        {
            continue;
        }
        // The title is only the first line; heredoc messages come later.
        let full = full_command(e).unwrap_or_else(|| e.title.clone());
        let Some(args) = commit_args(&full) else {
            continue;
        };
        // git prints `[branch abc1234] message` (or `[branch (root-commit) abc1234]`).
        let printed = parse_summary(&e.body);
        let message = printed
            .as_ref()
            .map(|p| p.2.clone())
            .or_else(|| message_from(&args, &full));
        if printed.is_none() && message.is_none() {
            continue;
        }
        out.push(Commit {
            seq: e.seq,
            hash: printed.as_ref().map(|p| p.1.clone()),
            branch: printed.map(|p| p.0),
            message,
        });
    }
    out
}

/// The whole command: Claude Code's `command` string or Codex's argv.
fn full_command(e: &crate::model::Event) -> Option<String> {
    let input = e.input.as_ref()?;
    if let Some(c) = input.get("command").and_then(|v| v.as_str()) {
        return Some(c.to_string());
    }
    let parts: Vec<&str> = input
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    parts.last().map(|s| s.to_string())
}

fn parse_summary(body: &str) -> Option<(String, String, String)> {
    for line in body.lines() {
        let l = line.trim();
        let Some(rest) = l.strip_prefix('[') else {
            continue;
        };
        let Some((inside, msg)) = rest.split_once("] ") else {
            continue;
        };
        let parts: Vec<&str> = inside.split_whitespace().collect();
        let (Some(branch), Some(hash)) = (parts.first(), parts.last()) else {
            continue;
        };
        if parts.len() >= 2 && hash.len() >= 7 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some((branch.to_string(), hash.to_string(), msg.trim().to_string()));
        }
    }
    None
}

/// The arguments of a real `git commit` command in `cmd`, if any. Text
/// that only mentions `git commit` (in a heredoc or a string) doesn't count.
fn commit_args(cmd: &str) -> Option<Vec<String>> {
    for w in crate::shell::command_words(cmd) {
        let mut i = w.iter().position(|x| x == "git")?;
        // Skip options before the subcommand: `-c key=val`, `-C dir`.
        i += 1;
        while i < w.len() && w[i].starts_with('-') {
            i += if w[i] == "-c" || w[i] == "-C" { 2 } else { 1 };
        }
        if w.get(i).map(String::as_str) == Some("commit") {
            return Some(w[i + 1..].to_vec());
        }
    }
    None
}

/// The message from `-m`, or the first line of a heredoc given to `-F -`.
fn message_from(args: &[String], cmd: &str) -> Option<String> {
    if let Some(i) = args.iter().position(|a| {
        a == "-m"
            || a == "--message"
            || a.ends_with('m') && a.starts_with('-') && !a.starts_with("--")
    }) {
        let msg = args.get(i + 1)?.lines().next()?.trim().to_string();
        return (!msg.is_empty()).then_some(msg);
    }
    if args.windows(2).any(|w| w[0] == "-F" && w[1] == "-") && cmd.contains("<<") {
        let start = cmd.find("<<")?;
        return cmd[start..]
            .lines()
            .skip(1)
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(String::from);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Event;

    fn cmd(title: &str, body: &str) -> Event {
        let mut e = Event::new(EventKind::ToolCall, 0, None);
        e.category = Some(ToolCategory::Command);
        e.title = title.into();
        e.body = body.into();
        e
    }

    #[test]
    fn reads_commits() {
        let p = ParsedSession {
            events: vec![
                cmd(
                    "git add -A && git commit -m \"Fix cart totals\"",
                    "[main 4f2a91c] Fix cart totals\n 2 files changed",
                ),
                cmd("git commit -q -m 'Quiet one'", ""),
                cmd("git status", "On branch main"),
                cmd("cat > x.py <<'EOF'\nprint('git commit -m nope')\nEOF", ""),
                {
                    let mut e = cmd("git add -A && git commit -q -F - <<'EOF'", "");
                    e.input = Some(
                        serde_json::json!({"command": "git add -A && git commit -q -F - <<'EOF'\nAdd replay\n\nbody\nEOF"}),
                    );
                    e
                },
            ],
            ..Default::default()
        };
        let c = detect(&p);
        assert_eq!(c.len(), 3, "{c:?}");
        assert_eq!(c[2].message.as_deref(), Some("Add replay"));
        assert_eq!(c[0].hash.as_deref(), Some("4f2a91c"));
        assert_eq!(c[0].branch.as_deref(), Some("main"));
        assert_eq!(c[0].message.as_deref(), Some("Fix cart totals"));
        assert_eq!(
            (c[1].hash.as_deref(), c[1].message.as_deref()),
            (None, Some("Quiet one"))
        );
    }
}
