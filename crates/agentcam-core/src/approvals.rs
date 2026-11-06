//! How tool calls were allowed or stopped, with allow rules to suggest,
//! and the thinking agents did.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::index::Result;
use crate::query::{Filter, conditions};

#[derive(Debug, Clone, Serialize)]
pub struct ToolApprovals {
    pub tool_name: String,
    /// Count per label (`rule`, `auto`, `you`, `rejected`, ...).
    pub counts: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stopped {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub tool_name: Option<String>,
    pub title: String,
    pub approval: String,
    pub agent: String,
}

/// An allow rule that would let a group of calls run without asking.
#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    /// Claude Code permission rule, e.g. `Bash(npm test:*)`.
    pub rule: String,
    /// Calls you approved by hand.
    pub by_you: i64,
    /// Calls auto mode had to check first.
    pub by_auto: i64,
    pub last_ts: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Approvals {
    pub counts: BTreeMap<String, i64>,
    pub tools: Vec<ToolApprovals>,
    /// Calls that were rejected or blocked, newest first.
    pub stopped: Vec<Stopped>,
    pub suggestions: Vec<Suggestion>,
}

/// `npm test -- --watch` → `npm test`; `cd x && git status` → `git status`;
/// `python3 build.py` → `python3`. `None` for commands not worth a rule.
pub fn command_prefix(cmd: &str) -> Option<String> {
    let words = crate::shell::command_words(cmd)
        .into_iter()
        .find(|w| w.first().is_some_and(|c| c != "cd"))?;
    let first = words.first()?;
    if first.contains('=') || first.starts_with(['.', '/', '$', '(']) {
        return None;
    }
    let second = words.get(1).filter(|w| {
        w.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && w.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_:".contains(c))
            && !w.contains('.')
    });
    let word = |w: &&String| {
        w.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && w.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_:".contains(c))
    };
    // `npm run build`, `pnpm exec tsc`: the script is the part that matters.
    let third = second
        .filter(|s| matches!(s.as_str(), "run" | "exec" | "x"))
        .and_then(|_| words.get(2).filter(word));
    Some(match (second, third) {
        (Some(s), Some(t)) => format!("{first} {s} {t}"),
        (Some(s), None) => format!("{first} {s}"),
        _ => first.clone(),
    })
}

/// Commands that can do anything with their arguments: a prefix rule for
/// them would allow far more than the calls it was based on.
const TOO_BROAD: &[&str] = &[
    "bash",
    "sh",
    "zsh",
    "fish",
    "python",
    "python3",
    "node",
    "deno",
    "bun",
    "ruby",
    "perl",
    "php",
    "npx",
    "pnpx",
    "bunx",
    "uvx",
    "env",
    "export",
    "eval",
    "exec",
    "source",
    "sudo",
    "xargs",
    "rm",
    "mv",
    "dd",
    "curl",
    "wget",
    "ssh",
    "scp",
    "osascript",
    "sed",
    "awk",
    "find",
    "chmod",
    "chown",
    "kill",
    "pkill",
    "git push",
    "git reset",
    "git checkout",
    "git clean",
];

fn rule_for(tool: &str, input: Option<&str>) -> Option<String> {
    match tool {
        // Edits are better handled by a mode than per-file rules, and
        // answering a question isn't an approval.
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "AskUserQuestion" | "ExitPlanMode"
        | "EnterPlanMode" => None,
        // One rule per MCP server covers all its tools.
        t if t.starts_with("mcp__") => {
            let server = t[5..].split("__").next().filter(|s| !s.is_empty())?;
            Some(format!("mcp__{server}"))
        }
        "Bash" => {
            let v: serde_json::Value = serde_json::from_str(input?).ok()?;
            let prefix = command_prefix(v.get("command")?.as_str()?)?;
            let first = prefix.split(' ').next().unwrap_or("");
            let two: String = prefix.split(' ').take(2).collect::<Vec<_>>().join(" ");
            if TOO_BROAD.contains(&first) || TOO_BROAD.contains(&two.as_str()) {
                return None;
            }
            Some(format!("Bash({prefix}:*)"))
        }
        t => Some(t.to_string()),
    }
}

pub fn approvals(conn: &Connection, f: &Filter) -> Result<Approvals> {
    let (cond, args) = conditions(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT e.tool_name, e.approval, count(*) FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.approval IS NOT NULL AND {cond} GROUP BY 1, 2"
    ))?;
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut by_tool: HashMap<String, BTreeMap<String, i64>> = HashMap::new();
    let mut rows = stmt.query(params_from_iter(&args))?;
    while let Some(r) = rows.next()? {
        let (tool, label, n): (Option<String>, String, i64) = (r.get(0)?, r.get(1)?, r.get(2)?);
        *counts.entry(label.clone()).or_default() += n;
        *by_tool
            .entry(tool.unwrap_or_default())
            .or_default()
            .entry(label)
            .or_default() += n;
    }
    let mut tools: Vec<ToolApprovals> = by_tool
        .into_iter()
        .map(|(tool_name, counts)| ToolApprovals { tool_name, counts })
        .collect();
    tools.sort_by_key(|t| std::cmp::Reverse(t.counts.values().sum::<i64>()));
    tools.truncate(15);

    let mut stmt = conn.prepare(&format!(
        "SELECT e.session_id, e.seq, e.ts, e.tool_name, e.title, e.approval, s.agent
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE (e.approval = 'rejected' OR e.approval LIKE 'blocked%') AND {cond}
         ORDER BY e.ts DESC LIMIT 30"
    ))?;
    let stopped = stmt
        .query_map(params_from_iter(&args), |r| {
            Ok(Stopped {
                session_id: r.get(0)?,
                seq: r.get(1)?,
                ts: r.get(2)?,
                tool_name: r.get(3)?,
                title: r.get(4)?,
                approval: r.get(5)?,
                agent: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    // Calls you approved by hand, or that auto mode had to check, grouped
    // by the rule that would cover them.
    let mut stmt = conn.prepare(&format!(
        "SELECT e.tool_name, e.input, e.approval, e.ts FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.approval IN ('you', 'auto') AND e.tool_name IS NOT NULL AND {cond}"
    ))?;
    let mut groups: HashMap<String, Suggestion> = HashMap::new();
    let mut rows = stmt.query(params_from_iter(&args))?;
    while let Some(r) = rows.next()? {
        let (tool, input, label, ts): (String, Option<String>, String, Option<String>) =
            (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
        let Some(rule) = rule_for(&tool, input.as_deref()) else {
            continue;
        };
        let g = groups.entry(rule.clone()).or_insert(Suggestion {
            rule,
            by_you: 0,
            by_auto: 0,
            last_ts: None,
        });
        if label == "you" {
            g.by_you += 1;
        } else {
            g.by_auto += 1;
        }
        if ts > g.last_ts {
            g.last_ts = ts;
        }
    }
    let mut suggestions: Vec<Suggestion> = groups
        .into_values()
        .filter(|g| g.by_you >= 2 || g.by_auto >= 10)
        .collect();
    suggestions.sort_by_key(|g| std::cmp::Reverse(g.by_you * 20 + g.by_auto));
    suggestions.truncate(12);
    Ok(Approvals {
        counts,
        tools,
        stopped,
        suggestions,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Thought {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub text: String,
    pub duration_ms: Option<i64>,
    pub agent: String,
    pub model: Option<String>,
    pub title: Option<String>,
    pub project_root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelThinking {
    pub model: Option<String>,
    pub blocks: i64,
    pub total_ms: i64,
    /// Blocks with a recorded time.
    pub timed: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Thinking {
    pub blocks: i64,
    pub total_ms: i64,
    pub by_model: Vec<ModelThinking>,
    pub items: Vec<Thought>,
}

/// Thinking steps, longest first (`sort = "longest"`) or newest first.
pub fn thinking(conn: &Connection, f: &Filter, sort: &str, limit: u32) -> Result<Thinking> {
    let (cond, args) = conditions(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT s.model, count(*), coalesce(sum(e.duration_ms), 0), count(e.duration_ms)
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.kind = 'thinking' AND {cond} GROUP BY 1 ORDER BY 2 DESC"
    ))?;
    let by_model: Vec<ModelThinking> = stmt
        .query_map(params_from_iter(&args), |r| {
            Ok(ModelThinking {
                model: r.get(0)?,
                blocks: r.get(1)?,
                total_ms: r.get(2)?,
                timed: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let order = if sort == "longest" {
        "coalesce(e.duration_ms, 0) DESC, length(e.body) DESC"
    } else {
        "e.ts DESC"
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT e.session_id, e.seq, e.ts, substr(e.body, 1, 1200), e.duration_ms, s.agent, s.model,
                coalesce(r.title, r.first_prompt), s.project_root
         FROM events e JOIN sessions s ON s.id = e.session_id
         LEFT JOIN sessions r ON r.id = coalesce(s.parent_id, s.id)
         WHERE e.kind = 'thinking' AND {cond} ORDER BY {order} LIMIT {limit}"
    ))?;
    let items = stmt
        .query_map(params_from_iter(&args), |r| {
            Ok(Thought {
                session_id: r.get(0)?,
                seq: r.get(1)?,
                ts: r.get(2)?,
                text: r.get(3)?,
                duration_ms: r.get(4)?,
                agent: r.get(5)?,
                model: r.get(6)?,
                title: r.get(7)?,
                project_root: r.get(8)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(Thinking {
        blocks: by_model.iter().map(|m| m.blocks).sum(),
        total_ms: by_model.iter().map(|m| m.total_ms).sum(),
        by_model,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_make_sensible_rules() {
        assert_eq!(
            command_prefix("npm test -- --watch").as_deref(),
            Some("npm test")
        );
        assert_eq!(
            command_prefix("cd web && npm run build").as_deref(),
            Some("npm run build")
        );
        assert_eq!(
            command_prefix("git status --short").as_deref(),
            Some("git status")
        );
        assert_eq!(
            command_prefix("python3 scripts/x.py").as_deref(),
            Some("python3")
        );
        assert_eq!(command_prefix("FOO=1 make").as_deref(), None);
        assert_eq!(command_prefix("./run.sh").as_deref(), None);
        assert_eq!(
            rule_for("Bash", Some(r#"{"command":"cargo test -q"}"#)).as_deref(),
            Some("Bash(cargo test:*)")
        );
        assert_eq!(rule_for("Edit", None), None);
        assert_eq!(
            rule_for("Bash", Some(r#"{"command":"python3 x.py"}"#)),
            None
        );
        assert_eq!(rule_for("Bash", Some(r#"{"command":"git push -f"}"#)), None);
        assert_eq!(
            rule_for("mcp__XcodeBuildMCP__tap", None).as_deref(),
            Some("mcp__XcodeBuildMCP")
        );
        assert_eq!(rule_for("AskUserQuestion", None), None);
        assert_eq!(rule_for("WebFetch", None).as_deref(), Some("WebFetch"));
    }
}
