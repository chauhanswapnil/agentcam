//! What agent setup this machine has, as a team server may see it: agent
//! versions in use, MCP servers, plugins, skills and hooks by name, and a
//! few settings worth a second look (approvals skipped by default, sandbox
//! off, very broad allow rules).
//!
//! Names and kinds only. Never commands, arguments, environment values,
//! URLs, headers or file contents; hook commands are cut to the program's
//! file name. Project-level items carry no project path.

use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::index::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub os: String,
    pub arch: String,
    pub agentcam_version: String,
    /// Agent versions seen in sessions from the last 30 days.
    pub agents: Vec<AgentVersion>,
    pub items: Vec<Item>,
    pub flags: Vec<Flag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentVersion {
    pub agent: String,
    pub version: Option<String>,
    pub sessions: i64,
    pub last_used: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Item {
    /// `mcp_server`, `plugin`, `skill` or `hook`.
    pub kind: String,
    /// `claude-code` or `codex`.
    pub agent: String,
    pub name: String,
    /// MCP transport (`stdio`, `http`, `sse`), hook event, plugin state.
    pub detail: Option<String>,
    /// `user` or `project`.
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Flag {
    /// `approvals_skipped_by_default`, `sandbox_off`, `broad_allow_rule`,
    /// `danger_prompt_skipped`, `guard_hook_installed`.
    pub id: String,
    pub agent: String,
    pub scope: String,
}

/// The inventory for this machine. `home` is the user's home folder;
/// project folders come from sessions in the index.
pub fn inventory(conn: &Connection, home: &Path, agentcam_version: &str) -> Result<Inventory> {
    let since = crate::util::format_ts_ms(crate::util::now_ms() - 30 * 86_400_000);
    let mut stmt = conn.prepare(
        "SELECT agent, agent_version, count(*), max(ended_at) FROM sessions
         WHERE parent_id IS NULL AND ended_at >= ?1 GROUP BY 1, 2 ORDER BY 1, 4 DESC",
    )?;
    let agents = stmt
        .query_map([&since], |r| {
            Ok(AgentVersion {
                agent: r.get(0)?,
                version: r.get(1)?,
                sessions: r.get(2)?,
                last_used: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT project_root FROM sessions
         WHERE project_root IS NOT NULL AND ended_at >= ?1",
    )?;
    let projects: Vec<String> = stmt
        .query_map([&since], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let (items, flags) = read_setup(home, &projects);
    Ok(Inventory {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        agentcam_version: agentcam_version.to_string(),
        agents,
        items,
        flags,
    })
}

/// Items and flags from the agents' config files.
pub fn read_setup(home: &Path, projects: &[String]) -> (Vec<Item>, Vec<Flag>) {
    let mut items = BTreeSet::new();
    let mut flags = BTreeSet::new();
    let claude = "claude-code";
    let dirs = crate::ConfigDirs::new(home);

    // Claude Code settings: user, then each project's shared and local file.
    let mut settings = vec![(dirs.claude.join("settings.json"), "user")];
    for p in projects {
        let p = Path::new(p);
        settings.push((p.join(".claude/settings.json"), "project"));
        settings.push((p.join(".claude/settings.local.json"), "project"));
    }
    for (path, scope) in &settings {
        if let Some(v) = read_json(path) {
            claude_settings(&v, scope, &mut items, &mut flags);
        }
    }

    // MCP servers: ~/.claude.json (user and per project) and .mcp.json files.
    if let Some(v) = read_json(&dirs.claude_json) {
        mcp_servers(v.get("mcpServers"), claude, "user", &mut items);
        if let Some(ps) = v.get("projects").and_then(Value::as_object) {
            for p in ps.values() {
                mcp_servers(p.get("mcpServers"), claude, "project", &mut items);
            }
        }
    }
    for p in projects {
        if let Some(v) = read_json(&Path::new(p).join(".mcp.json")) {
            mcp_servers(v.get("mcpServers"), claude, "project", &mut items);
        }
    }

    // Plugins installed for Claude Code.
    if let Some(v) = read_json(&dirs.claude.join("plugins/installed_plugins.json"))
        && let Some(ps) = v.get("plugins").and_then(Value::as_object)
    {
        for name in ps.keys() {
            items.insert(Item {
                kind: "plugin".into(),
                agent: claude.into(),
                name: name.clone(),
                detail: None,
                scope: "user".into(),
            });
        }
    }

    // Skills (names only).
    for s in crate::skills::installed(home, projects) {
        let (agent, scope) = match s.source.as_str() {
            "codex" => ("codex", "user"),
            "claude-project" => (claude, "project"),
            "claude-plugin" => (claude, "plugin"),
            _ => (claude, "user"),
        };
        items.insert(Item {
            kind: "skill".into(),
            agent: agent.into(),
            name: s.name,
            detail: None,
            scope: scope.into(),
        });
    }

    // Codex: config.toml and hooks.json.
    if let Ok(text) = std::fs::read_to_string(dirs.codex.join("config.toml")) {
        codex_config(&text, &mut items, &mut flags);
    }
    if let Some(v) = read_json(&dirs.codex.join("hooks.json")) {
        hooks(&v, "codex", "user", &mut items, &mut flags);
    }

    (items.into_iter().collect(), flags.into_iter().collect())
}

pub(crate) fn read_json(path: &Path) -> Option<Value> {
    let meta = std::fs::metadata(path).ok()?;
    // ~/.claude.json can grow large with history; don't read huge files.
    if meta.len() > 32 << 20 {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn flag(flags: &mut BTreeSet<Flag>, id: &str, agent: &str, scope: &str) {
    flags.insert(Flag {
        id: id.into(),
        agent: agent.into(),
        scope: scope.into(),
    });
}

/// Allow rules that let every shell command run without asking.
pub fn is_broad_rule(rule: &str) -> bool {
    let r = rule.trim();
    matches!(
        r,
        "Bash" | "Bash(*)" | "Bash(:*)" | "Bash(**)" | "*" | "Bash(sh:*)" | "Bash(bash:*)"
    )
}

fn claude_settings(v: &Value, scope: &str, items: &mut BTreeSet<Item>, flags: &mut BTreeSet<Flag>) {
    let agent = "claude-code";
    let perms = v.get("permissions");
    if perms
        .and_then(|p| p.get("defaultMode"))
        .and_then(Value::as_str)
        == Some("bypassPermissions")
    {
        flag(flags, "approvals_skipped_by_default", agent, scope);
    }
    if perms
        .and_then(|p| p.get("allow"))
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().filter_map(Value::as_str).any(is_broad_rule))
    {
        flag(flags, "broad_allow_rule", agent, scope);
    }
    if v.get("skipDangerousModePermissionPrompt")
        .and_then(Value::as_bool)
        == Some(true)
    {
        flag(flags, "danger_prompt_skipped", agent, scope);
    }
    if v.get("sandbox")
        .and_then(|s| s.get("enabled"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        flag(flags, "sandbox_off", agent, scope);
    }
    hooks(v, agent, scope, items, flags);
}

/// `{"hooks": {"PreToolUse": [{"matcher": "...", "hooks": [{"command": "..."}]}]}}`.
fn hooks(
    v: &Value,
    agent: &str,
    scope: &str,
    items: &mut BTreeSet<Item>,
    flags: &mut BTreeSet<Flag>,
) {
    let Some(events) = v.get("hooks").and_then(Value::as_object) else {
        return;
    };
    for (event, groups) in events {
        for g in groups.as_array().into_iter().flatten() {
            for h in g
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(cmd) = h.get("command").and_then(Value::as_str) else {
                    continue;
                };
                if event == "PreToolUse" && cmd.contains("agentcam") && cmd.contains("guard") {
                    flag(flags, "guard_hook_installed", agent, scope);
                }
                let Some(name) = program(cmd) else { continue };
                items.insert(Item {
                    kind: "hook".into(),
                    agent: agent.into(),
                    name,
                    detail: Some(event.clone()),
                    scope: scope.into(),
                });
            }
        }
    }
}

/// The file name of a command's program, skipping `env X=Y` and
/// interpreters, so `python3 /Users/x/hooks/check.py --a` is `check.py`.
fn program(cmd: &str) -> Option<String> {
    let w = crate::util::command_program(cmd)?;
    // A variable or something odd isn't a name worth sending.
    if w.contains('$') {
        return None;
    }
    let name = crate::util::file_name(w);
    (!name.is_empty() && name.len() <= 64).then(|| name.to_string())
}

fn mcp_servers(v: Option<&Value>, agent: &str, scope: &str, items: &mut BTreeSet<Item>) {
    let Some(servers) = v.and_then(Value::as_object) else {
        return;
    };
    for (name, cfg) in servers {
        let detail = cfg
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| cfg.get("url").map(|_| "http".to_string()))
            .unwrap_or_else(|| "stdio".to_string());
        items.insert(Item {
            kind: "mcp_server".into(),
            agent: agent.into(),
            name: name.clone(),
            detail: Some(detail),
            scope: scope.into(),
        });
    }
}

/// A careful line reader for the few keys we need; not a TOML parser.
fn codex_config(text: &str, items: &mut BTreeSet<Item>, flags: &mut BTreeSet<Flag>) {
    let agent = "codex";
    let mut table = String::new();
    let mut server: Option<(String, bool)> = None;
    let done = |server: &mut Option<(String, bool)>, items: &mut BTreeSet<Item>| {
        if let Some((name, http)) = server.take() {
            items.insert(Item {
                kind: "mcp_server".into(),
                agent: agent.into(),
                name,
                detail: Some(if http { "http" } else { "stdio" }.into()),
                scope: "user".into(),
            });
        }
    };
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('#') || l.is_empty() {
            continue;
        }
        if let Some(t) = l.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            done(&mut server, items);
            table = t.trim().to_string();
            if let Some(name) = table.strip_prefix("mcp_servers.")
                && !name.contains('.')
            {
                server = Some((name.trim_matches('"').to_string(), false));
            }
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim().trim_matches('"'));
        if table.is_empty() {
            match (k, v) {
                ("approval_policy", "never") => {
                    flag(flags, "approvals_skipped_by_default", agent, "user")
                }
                ("sandbox_mode", "danger-full-access") => flag(flags, "sandbox_off", agent, "user"),
                _ => {}
            }
        } else if let Some((_, http)) = server.as_mut()
            && k == "url"
        {
            *http = true;
        }
    }
    done(&mut server, items);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_programs_lose_paths_and_arguments() {
        assert_eq!(
            program("/usr/local/bin/agentcam guard").as_deref(),
            Some("agentcam")
        );
        assert_eq!(
            program("python3 /Users/dev/hooks/check.py --strict").as_deref(),
            Some("check.py")
        );
        assert_eq!(program("env TOKEN=abc node x.js").as_deref(), Some("x.js"));
        assert_eq!(program("$HOME/bin/run"), None);
    }

    #[test]
    fn reads_setup_names_only() {
        let home = std::env::temp_dir().join(format!("agentcam-fleet-{}", std::process::id()));
        let proj = home.join("code/app");
        std::fs::create_dir_all(home.join(".claude/plugins")).unwrap();
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::create_dir_all(proj.join(".claude")).unwrap();
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"permissions":{"allow":["Bash(npm test:*)"],"defaultMode":"acceptEdits"},
               "hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"/opt/bin/agentcam guard"}]}]}}"#,
        )
        .unwrap();
        std::fs::write(
            proj.join(".claude/settings.local.json"),
            r#"{"permissions":{"allow":["Bash(*)"],"defaultMode":"bypassPermissions"}}"#,
        )
        .unwrap();
        std::fs::write(
            home.join(".claude.json"),
            r#"{"mcpServers":{"linear":{"type":"http","url":"https://mcp.example/x?token=sk-test-0000","headers":{"Authorization":"Bearer sk-test-0000"}}},
               "projects":{"/Users/dev/code/app":{"mcpServers":{"db":{"command":"npx","args":["pg","postgres://u:p@h/db"],"env":{"PW":"x"}}}}}}"#,
        )
        .unwrap();
        std::fs::write(
            home.join(".claude/plugins/installed_plugins.json"),
            r#"{"version":2,"plugins":{"review@tools":[{"installPath":"/Users/dev/x"}]}}"#,
        )
        .unwrap();
        std::fs::write(
            home.join(".codex/config.toml"),
            "model = \"gpt-5\"\napproval_policy = \"never\"\nsandbox_mode = \"danger-full-access\"\n\n[mcp_servers.docs]\nurl = \"https://docs.example/mcp\"\n\n[mcp_servers.fs]\ncommand = \"npx\"\nargs = [\"fs\", \"/Users/dev\"]\n",
        )
        .unwrap();

        let (items, flags) = read_setup(&home, &[proj.to_string_lossy().to_string()]);
        let names: Vec<_> = items
            .iter()
            .map(|i| {
                format!(
                    "{}:{}:{}:{}",
                    i.kind,
                    i.agent,
                    i.name,
                    i.detail.clone().unwrap_or_default()
                )
            })
            .collect();
        assert!(
            names.contains(&"mcp_server:claude-code:linear:http".to_string()),
            "{names:?}"
        );
        assert!(names.contains(&"mcp_server:claude-code:db:stdio".to_string()));
        assert!(names.contains(&"mcp_server:codex:docs:http".to_string()));
        assert!(names.contains(&"mcp_server:codex:fs:stdio".to_string()));
        assert!(names.contains(&"plugin:claude-code:review@tools:".to_string()));
        assert!(names.contains(&"hook:claude-code:agentcam:PreToolUse".to_string()));
        let mut ids: Vec<_> = flags
            .iter()
            .map(|f| format!("{}:{}:{}", f.agent, f.id, f.scope))
            .collect();
        ids.sort();
        assert_eq!(
            ids,
            vec![
                "claude-code:approvals_skipped_by_default:project",
                "claude-code:broad_allow_rule:project",
                "claude-code:guard_hook_installed:user",
                "codex:approvals_skipped_by_default:user",
                "codex:sandbox_off:user",
            ]
        );
        // Nothing but names and kinds leaves: no URLs, args, env or paths.
        let json = serde_json::to_string(&(items, flags)).unwrap();
        for bad in ["sk-test", "postgres", "https://", "/Users/", "/opt/", "PW"] {
            assert!(!json.contains(bad), "{bad} leaked: {json}");
        }
        std::fs::remove_dir_all(&home).unwrap();
    }
}
