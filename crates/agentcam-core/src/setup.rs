//! The agents' setup on this machine, for the local Setup page: settings
//! files, memory and instruction files, MCP servers, hooks and skills, with
//! checks worth a look. Shown locally only; the team server gets the
//! names-only version in `fleet.rs`.
//!
//! Environment variables are listed by name, never with their values.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::ConfigDirs;
use crate::index::Result;
use crate::util::command_program;

#[derive(Debug, Clone, Serialize)]
pub struct SettingsFile {
    pub agent: String,
    /// `user`, `project`, `local` or `managed`.
    pub scope: String,
    pub project: Option<String>,
    pub path: String,
    /// Couldn't be read as JSON / TOML: the reason.
    pub error: Option<String>,
    pub default_mode: Option<String>,
    /// Codex `sandbox_mode`.
    pub sandbox: Option<String>,
    pub model: Option<String>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub ask: Vec<String>,
    /// Names only.
    pub env: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryFile {
    /// `user` or `project`.
    pub scope: String,
    pub project: Option<String>,
    pub path: String,
    pub chars: usize,
    /// About four characters a token.
    pub tokens: usize,
    /// `@file` imports in it.
    pub imports: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServer {
    pub agent: String,
    pub name: String,
    pub transport: String,
    /// The program for stdio servers, the host for remote ones.
    pub target: Option<String>,
    pub scope: String,
    pub project: Option<String>,
    pub defined_in: String,
    /// Tool calls to it in the last 30 days.
    pub calls_30d: i64,
    /// Its tools bring in outside content (issues, errors, chat, web).
    pub outside_content: bool,
    /// Started with `npx`/`uvx`/... and no version: every start runs the
    /// latest release.
    pub unpinned: bool,
    /// Names of config keys (env, headers) holding what looks like a
    /// secret written in plain text. Never the values.
    pub secret_keys: Vec<String>,
    /// A remote server reached over plain `http://`.
    pub plain_http: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hook {
    pub agent: String,
    pub event: String,
    pub matcher: Option<String>,
    pub command: String,
    pub scope: String,
    pub defined_in: String,
    /// The command names a file that isn't there.
    pub missing: bool,
    /// Hook steps in the logs over the last 30 days, and how many errored.
    pub runs_30d: i64,
    pub errors_30d: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub source: String,
    pub chars: u64,
    pub uses_90d: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: String,
    /// `look` (worth a second look) or `info`.
    pub level: String,
    pub title: String,
    pub detail: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Setup {
    pub dirs: ConfigDirs,
    pub settings: Vec<SettingsFile>,
    pub memory: Vec<MemoryFile>,
    pub mcp: Vec<McpServer>,
    pub hooks: Vec<Hook>,
    pub skills: Vec<SkillInfo>,
    /// Average skill listing size per session over 30 days, in characters.
    pub skill_listing_chars: Option<i64>,
    /// What MCP tools, skills and instruction files tell the agent.
    pub tools: crate::toolcheck::ToolCheck,
    pub checks: Vec<Check>,
}

/// Memory files over this many characters (~10k tokens) are worth a look:
/// they load into every session.
const BIG_MEMORY: usize = 40_000;
/// A skill listing over this many characters (~3k tokens) per session.
const BIG_LISTING: i64 = 12_000;

fn json_file(path: &Path) -> Option<std::result::Result<Value, String>> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).map_err(|e| format!("not valid JSON: {e}")))
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn basename(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).to_string()
}

/// Projects with sessions in the last 90 days.
fn recent_projects(conn: &Connection) -> Result<Vec<String>> {
    let since = crate::util::format_ts_ms(crate::util::now_ms() - 90 * 86_400_000);
    let mut stmt = conn.prepare(
        "SELECT project_root, max(ended_at) FROM sessions
         WHERE project_root IS NOT NULL AND parent_id IS NULL AND ended_at >= ?1
         GROUP BY 1 ORDER BY 2 DESC LIMIT 40",
    )?;
    let rows = stmt
        .query_map([since], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}

pub fn report(conn: &Connection, home: &Path) -> Result<Setup> {
    let dirs = ConfigDirs::new(home);
    let projects: Vec<String> = recent_projects(conn)?
        .into_iter()
        .filter(|p| Path::new(p).is_dir())
        .collect();
    let since30 = crate::util::format_ts_ms(crate::util::now_ms() - 30 * 86_400_000);
    let since90 = crate::util::format_ts_ms(crate::util::now_ms() - 90 * 86_400_000);
    let mut checks: Vec<Check> = Vec::new();
    let mut look = |id: &str, title: String, detail: String, path: Option<String>| {
        checks.push(Check {
            id: id.into(),
            level: "look".into(),
            title,
            detail,
            path,
        })
    };

    // Claude Code settings files.
    let mut settings = Vec::new();
    let mut hooks = Vec::new();
    let mut files: Vec<(std::path::PathBuf, &str, Option<String>)> = vec![
        (dirs.claude.join("settings.json"), "user", None),
        (dirs.claude.join("settings.local.json"), "user", None),
    ];
    for m in [
        "/Library/Application Support/ClaudeCode/managed-settings.json",
        "/etc/claude-code/managed-settings.json",
    ] {
        files.push((m.into(), "managed", None));
    }
    for p in &projects {
        files.push((
            Path::new(p).join(".claude/settings.json"),
            "project",
            Some(p.clone()),
        ));
        files.push((
            Path::new(p).join(".claude/settings.local.json"),
            "local",
            Some(p.clone()),
        ));
    }
    for (path, scope, project) in files {
        let Some(parsed) = json_file(&path) else {
            continue;
        };
        let path_s = path.to_string_lossy().to_string();
        let mut f = SettingsFile {
            agent: "claude-code".into(),
            scope: scope.into(),
            project,
            path: path_s.clone(),
            error: None,
            default_mode: None,
            sandbox: None,
            model: None,
            allow: vec![],
            deny: vec![],
            ask: vec![],
            env: vec![],
        };
        match parsed {
            Err(e) => {
                look(
                    "settings_unreadable",
                    format!("{} can't be read", basename(&path_s)),
                    e.clone(),
                    Some(path_s.clone()),
                );
                f.error = Some(e);
            }
            Ok(v) => {
                let perms = v.get("permissions");
                f.default_mode = perms
                    .and_then(|p| p.get("defaultMode"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                f.allow = strings(perms.and_then(|p| p.get("allow")));
                f.deny = strings(perms.and_then(|p| p.get("deny")));
                f.ask = strings(perms.and_then(|p| p.get("ask")));
                f.model = v.get("model").and_then(Value::as_str).map(str::to_string);
                f.env = v
                    .get("env")
                    .and_then(Value::as_object)
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                read_hooks(&v, "claude-code", scope, &path_s, &mut hooks);
            }
        }
        settings.push(f);
    }
    // Codex config.toml: the keys that matter, read line by line.
    let codex_cfg = dirs.codex.join("config.toml");
    let mut mcp: Vec<McpServer> = Vec::new();
    if let Ok(text) = std::fs::read_to_string(&codex_cfg) {
        let path_s = codex_cfg.to_string_lossy().to_string();
        let mut f = SettingsFile {
            agent: "codex".into(),
            scope: "user".into(),
            project: None,
            path: path_s.clone(),
            error: None,
            default_mode: None,
            sandbox: None,
            model: None,
            allow: vec![],
            deny: vec![],
            ask: vec![],
            env: vec![],
        };
        let mut table = String::new();
        let mut server: Option<McpServer> = None;
        for line in text.lines().map(str::trim) {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some(t) = line.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
                mcp.extend(server.take());
                table = t.trim().to_string();
                if let Some(name) = table
                    .strip_prefix("mcp_servers.")
                    .filter(|n| !n.contains('.'))
                {
                    server = Some(McpServer {
                        agent: "codex".into(),
                        name: name.trim_matches('"').into(),
                        transport: "stdio".into(),
                        target: None,
                        scope: "user".into(),
                        project: None,
                        defined_in: path_s.clone(),
                        calls_30d: 0,
                        outside_content: false,
                        unpinned: false,
                        secret_keys: vec![],
                        plain_http: false,
                    });
                }
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim().trim_matches('"'));
            if table.is_empty() {
                match k {
                    "model" => f.model = Some(v.into()),
                    "approval_policy" => f.default_mode = Some(format!("approval_policy = {v}")),
                    "sandbox_mode" => f.sandbox = Some(v.into()),
                    _ => {}
                }
            } else if let Some(s) = server.as_mut() {
                match k {
                    "url" => {
                        s.transport = "http".into();
                        s.target = host_of(v);
                        s.plain_http = is_plain_http(v);
                    }
                    "command" => s.target = Some(basename(v)),
                    "args" => {
                        let args: Vec<String> = v
                            .trim_matches(['[', ']'])
                            .split(',')
                            .map(|a| a.trim().trim_matches('"').to_string())
                            .collect();
                        s.unpinned = unpinned(s.target.as_deref().unwrap_or(""), &args);
                    }
                    _ => {}
                }
            } else if let Some(s) = mcp.last_mut().filter(|s| {
                table == format!("mcp_servers.{}.env", s.name)
                    || table == format!("mcp_servers.{}.http_headers", s.name)
            }) {
                if looks_secret(k, v) {
                    s.secret_keys.push(k.to_string());
                }
            }
        }
        mcp.extend(server.take());
        settings.push(f);
    }
    if let Some(Ok(v)) = json_file(&dirs.codex.join("hooks.json")) {
        read_hooks(
            &v,
            "codex",
            "user",
            &dirs.codex.join("hooks.json").to_string_lossy(),
            &mut hooks,
        );
    }

    // Claude Code MCP servers.
    let claude_json = dirs.claude_json.to_string_lossy().to_string();
    if let Some(Ok(v)) = json_file(&dirs.claude_json) {
        mcp_from(v.get("mcpServers"), "user", None, &claude_json, &mut mcp);
        if let Some(ps) = v.get("projects").and_then(Value::as_object) {
            for (p, cfg) in ps {
                if projects.contains(p) {
                    mcp_from(
                        cfg.get("mcpServers"),
                        "local",
                        Some(p.clone()),
                        &claude_json,
                        &mut mcp,
                    );
                }
            }
        }
    }
    for p in &projects {
        let f = Path::new(p).join(".mcp.json");
        if let Some(Ok(v)) = json_file(&f) {
            mcp_from(
                v.get("mcpServers"),
                "project",
                Some(p.clone()),
                &f.to_string_lossy(),
                &mut mcp,
            );
        }
    }
    // Calls per MCP server: Claude Code names tools `mcp__<server>__<tool>`.
    let mut calls: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT tool_name, count(*) FROM events
             WHERE tool_name LIKE 'mcp\\_\\_%' ESCAPE '\\' AND ts >= ?1 GROUP BY 1",
        )?;
        let mut rows = stmt.query([&since30])?;
        while let Some(r) = rows.next()? {
            let name: String = r.get(0)?;
            let n: i64 = r.get(1)?;
            if let Some(server) = name
                .strip_prefix("mcp__")
                .and_then(|r| r.split("__").next())
            {
                *calls.entry(server.to_lowercase()).or_default() += n;
            }
        }
        // Codex MCP calls record the server in `via` (`app:<server>`) or the title.
        let mut stmt = conn.prepare(
            "SELECT e.tool_name, count(*) FROM events e JOIN sessions s ON s.id = e.session_id
             WHERE s.agent = 'codex' AND e.category = 'mcp' AND e.ts >= ?1 GROUP BY 1",
        )?;
        let mut rows = stmt.query([&since30])?;
        while let Some(r) = rows.next()? {
            let name: Option<String> = r.get(0)?;
            let n: i64 = r.get(1)?;
            if let Some(server) = name
                .as_deref()
                .and_then(|t| t.split(['.', '/', ':']).next())
            {
                *calls.entry(server.to_lowercase()).or_default() += n;
            }
        }
    }
    for m in &mut mcp {
        let key = m.name.to_lowercase().replace(['-', ' '], "_");
        m.calls_30d = calls
            .iter()
            .filter(|(k, _)| k.replace('-', "_") == key)
            .map(|(_, n)| *n)
            .sum();
    }

    // Hook runs from the logs, matched by program name.
    {
        let mut stmt = conn
            .prepare("SELECT title, is_error FROM events WHERE tool_name = 'hook' AND ts >= ?1")?;
        let runs: Vec<(String, bool)> = stmt
            .query_map([&since30], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for h in &mut hooks {
            let prog = command_program(&h.command)
                .map(basename)
                .unwrap_or_default();
            if prog.is_empty() {
                continue;
            }
            for (t, err) in &runs {
                if t.starts_with(&h.event) && t.contains(&format!("({prog})")) {
                    h.runs_30d += 1;
                    h.errors_30d += i64::from(*err);
                }
            }
        }
    }

    // Memory and instruction files.
    let mut memory = Vec::new();
    let mut mem_file = |path: &Path, scope: &str, project: Option<String>| {
        if let Ok(text) = std::fs::read_to_string(path) {
            let chars = text.chars().count();
            memory.push(MemoryFile {
                scope: scope.into(),
                project,
                path: path.to_string_lossy().to_string(),
                chars,
                tokens: chars / 4,
                imports: text
                    .lines()
                    .filter(|l| {
                        l.trim_start().starts_with('@') && !l.trim_start().starts_with("@@")
                    })
                    .count(),
            });
        }
    };
    mem_file(&dirs.claude.join("CLAUDE.md"), "user", None);
    mem_file(&dirs.codex.join("AGENTS.md"), "user", None);
    for p in &projects {
        for name in [
            "CLAUDE.md",
            "AGENTS.md",
            "CLAUDE.local.md",
            ".claude/CLAUDE.md",
        ] {
            mem_file(&Path::new(p).join(name), "project", Some(p.clone()));
        }
    }

    // Skills, and how often each ran.
    let mut uses: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt =
            conn.prepare("SELECT skill, count(*) FROM skill_uses WHERE ts >= ?1 GROUP BY 1")?;
        let mut rows = stmt.query([&since90])?;
        while let Some(r) = rows.next()? {
            uses.insert(r.get(0)?, r.get(1)?);
        }
    }
    let skills: Vec<SkillInfo> = crate::skills::installed(home, &projects)
        .into_iter()
        .map(|s| SkillInfo {
            uses_90d: uses.get(&s.name).copied().unwrap_or(0),
            name: s.name,
            source: s.source,
            chars: s.chars,
        })
        .collect();
    let skill_listing_chars: Option<i64> = conn
        .query_row(
            "SELECT CAST(avg(skills_listing_chars) AS INTEGER) FROM sessions
             WHERE skills_listing_chars > 0 AND ended_at >= ?1",
            [&since30],
            |r| r.get(0),
        )
        .ok()
        .flatten();

    // Checks.
    for f in &settings {
        if f.default_mode.as_deref() == Some("bypassPermissions")
            || f.default_mode.as_deref() == Some("approval_policy = never")
        {
            look(
                "approvals_skipped_by_default",
                "Approvals skipped by default".into(),
                format!(
                    "{} runs every tool call without asking unless a rule says otherwise.",
                    where_(f)
                ),
                Some(f.path.clone()),
            );
        }
        if f.sandbox.as_deref() == Some("danger-full-access") {
            look(
                "sandbox_off",
                "Codex sandbox off".into(),
                "sandbox_mode = danger-full-access lets commands reach the whole machine.".into(),
                Some(f.path.clone()),
            );
        }
        for a in f.allow.iter().filter(|a| crate::fleet::is_broad_rule(a)) {
            look(
                "broad_allow_rule",
                format!("Allow rule {a} covers every shell command"),
                format!(
                    "In {}. Narrower rules (one per command) keep the questions you want.",
                    where_(f)
                ),
                Some(f.path.clone()),
            );
        }
    }
    for m in &memory {
        if m.chars > BIG_MEMORY {
            look(
                "memory_large",
                format!("{} is about {}k tokens", basename(&m.path), m.tokens / 1000),
                "It loads into every session in this scope. Moving rarely needed parts into skills or linked docs leaves more room for the work.".into(),
                Some(m.path.clone()),
            );
        }
    }
    if let Some(n) = skill_listing_chars.filter(|n| *n > BIG_LISTING) {
        look(
            "skill_listing_heavy",
            format!(
                "The skill listing takes about {}k tokens per session",
                n / 4000
            ),
            format!(
                "{} skills are listed. Skills that never run still cost their description in every session.",
                skills.len()
            ),
            None,
        );
    }
    for h in hooks.iter().filter(|h| h.missing) {
        look(
            "hook_missing_program",
            format!("{} hook points at a file that isn't there", h.event),
            format!("`{}` in {}", h.command, basename(&h.defined_in)),
            Some(h.defined_in.clone()),
        );
    }
    for h in hooks
        .iter()
        .filter(|h| h.runs_30d >= 3 && h.errors_30d * 2 >= h.runs_30d)
    {
        look(
            "hook_erroring",
            format!(
                "{} hook errored in {} of {} runs",
                h.event, h.errors_30d, h.runs_30d
            ),
            format!(
                "`{}`. Its output is on the hook steps in the replay.",
                h.command
            ),
            Some(h.defined_in.clone()),
        );
    }
    instruction_conflicts(&projects, &settings, &mut look);

    // MCP servers and what they, skills and instruction files tell the agent.
    for m in &mut mcp {
        m.outside_content = crate::toolcheck::brings_outside_content(&m.name, m.target.as_deref());
    }
    let tools = crate::toolcheck::report(conn, home, &projects)?;
    let outside: Vec<&str> = mcp
        .iter()
        .filter(|m| m.outside_content)
        .map(|m| m.name.as_str())
        .collect();
    let open_shell = settings.iter().find(|f| {
        f.default_mode.as_deref() == Some("bypassPermissions")
            || f.default_mode.as_deref() == Some("approval_policy = never")
            || f.allow.iter().any(|a| crate::fleet::is_broad_rule(a))
    });
    if let (false, Some(f)) = (outside.is_empty(), open_shell) {
        look(
            "outside_content_shell",
            "Outside content can reach a shell that runs without asking".into(),
            format!(
                "{} bring{} in issues, messages or web pages, and {} lets shell commands run without asking. \
                 An instruction planted in that content could run as a command. Narrower allow rules or the guard hook keep a question in the way.",
                outside.join(", "),
                if outside.len() == 1 { "s" } else { "" },
                where_(f)
            ),
            Some(f.path.clone()),
        );
    }
    for m in mcp.iter().filter(|m| !m.secret_keys.is_empty()) {
        look(
            "mcp_secret_in_config",
            format!("{} has a secret written in its config", m.name),
            format!(
                "In {}: {}. A reference such as ${{TOKEN}} keeps the value out of the file.",
                basename(&m.defined_in),
                m.secret_keys.join(", ")
            ),
            Some(m.defined_in.clone()),
        );
    }
    for m in mcp.iter().filter(|m| m.plain_http) {
        look(
            "mcp_plain_http",
            format!("{} connects over plain http", m.name),
            format!(
                "{} in {}. Tokens and tool results travel unencrypted; https keeps them private.",
                m.target.as_deref().unwrap_or("The server"),
                basename(&m.defined_in)
            ),
            Some(m.defined_in.clone()),
        );
    }
    for st in &tools.servers {
        // A tool whose flagged text arrived in a change is told by the change.
        let changed: HashSet<&str> = st
            .changes
            .iter()
            .filter(|c| !c.new_flags.is_empty())
            .map(|c| c.tool.as_str())
            .collect();
        for t in st
            .tools
            .iter()
            .filter(|t| !t.flags.is_empty() && !changed.contains(t.tool.as_str()))
            .take(10)
        {
            let f = &t.flags[0];
            let what = if t.tool == "(instructions)" {
                format!("{}'s instructions", st.server)
            } else {
                format!("{} tool {}", st.server, t.tool)
            };
            look(
                "mcp_tool_text",
                format!("{what}: {}", f.title.to_lowercase()),
                format!(
                    "“{}”. Tool descriptions reach the agent as instructions; worth reading before the next session.",
                    f.excerpt.as_deref().unwrap_or("")
                ),
                None,
            );
        }
        for c in st.changes.iter().filter(|c| !c.new_flags.is_empty()) {
            look(
                "mcp_tool_changed",
                format!(
                    "{} changed {} and the new text {}",
                    st.server,
                    if c.tool == "(instructions)" {
                        "its instructions".to_string()
                    } else {
                        format!("the {} tool", c.tool)
                    },
                    c.new_flags[0].title.to_lowercase()
                ),
                format!(
                    "On {}. Now: “{}”",
                    c.changed_at
                        .as_deref()
                        .map_or("?", |t| t.get(..10).unwrap_or(t)),
                    c.after
                ),
                None,
            );
        }
    }
    for sk in &tools.skills {
        let (file, f) = &sk.flags[0];
        look(
            "skill_text",
            format!("Skill {}: {}", sk.name, f.title.to_lowercase()),
            format!("{file}: “{}”", f.excerpt.as_deref().unwrap_or("")),
            Some(sk.path.clone()),
        );
    }
    for file in &tools.instructions {
        let (_, f) = &file.flags[0];
        look(
            "instructions_text",
            format!("{}: {}", file.name, f.title.to_lowercase()),
            format!(
                "“{}”. These files load into every session.",
                f.excerpt.as_deref().unwrap_or("")
            ),
            Some(file.path.clone()),
        );
    }

    let mut info = |id: &str, title: String, detail: String| {
        checks.push(Check {
            id: id.into(),
            level: "info".into(),
            title,
            detail,
            path: None,
        })
    };
    let idle: Vec<&McpServer> = mcp.iter().filter(|m| m.calls_30d == 0).collect();
    if !idle.is_empty() {
        info(
            "mcp_unused",
            format!(
                "{} MCP server{} with no calls in 30 days",
                idle.len(),
                if idle.len() == 1 { "" } else { "s" }
            ),
            idle.iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    let unused = skills
        .iter()
        .filter(|s| s.uses_90d == 0 && s.source != "claude-plugin")
        .count();
    if unused > 0 {
        info(
            "skills_unused",
            format!(
                "{unused} skill{} not run in 90 days",
                if unused == 1 { "" } else { "s" }
            ),
            "See the Skills page for which ones and how much context they take.".into(),
        );
    }
    let unpinned: Vec<&str> = mcp
        .iter()
        .filter(|m| m.unpinned)
        .map(|m| m.name.as_str())
        .collect();
    if !unpinned.is_empty() {
        info(
            "mcp_unpinned",
            format!(
                "{} MCP server{} run{} the latest release on every start",
                unpinned.len(),
                if unpinned.len() == 1 { "" } else { "s" },
                if unpinned.len() == 1 { "s" } else { "" }
            ),
            format!(
                "{}. Pinning a version (name@1.2.3) means an update runs only when you choose.",
                unpinned.join(", ")
            ),
        );
    }
    let quiet_changes: Vec<String> = tools
        .servers
        .iter()
        .flat_map(|st| {
            st.changes
                .iter()
                .filter(|c| c.new_flags.is_empty())
                .map(move |c| format!("{} · {}", st.server, c.tool))
        })
        .collect();
    if !quiet_changes.is_empty() {
        info(
            "mcp_tool_changed",
            format!(
                "{} MCP tool description{} changed in the last 90 days",
                quiet_changes.len(),
                if quiet_changes.len() == 1 { "" } else { "s" }
            ),
            format!(
                "{}. Nothing in the new text stood out; the MCP tools list shows what changed.",
                quiet_changes
                    .iter()
                    .take(6)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    if !hooks.iter().any(|h| {
        h.event == "PreToolUse" && h.command.contains("agentcam") && h.command.contains("guard")
    }) {
        info(
            "guard_not_installed",
            "agentcam guard hook not set up".into(),
            "Guard rules only stop calls once the hook is in your settings. The Audit page has the JSON to copy.".into(),
        );
    }
    if dirs.claude_from_env || dirs.codex_from_env {
        info(
            "custom_config_dirs",
            "Custom config folders in use".into(),
            format!(
                "Reading {} and {}.",
                dirs.claude.display(),
                dirs.codex.display()
            ),
        );
    }

    Ok(Setup {
        dirs,
        settings,
        memory,
        mcp,
        hooks,
        skills,
        skill_listing_chars,
        tools,
        checks,
    })
}

fn where_(f: &SettingsFile) -> String {
    match &f.project {
        Some(p) => format!("{} in {}", basename(&f.path), basename(p)),
        None => basename(&f.path),
    }
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    Some(host.to_string())
}

fn mcp_from(
    v: Option<&Value>,
    scope: &str,
    project: Option<String>,
    file: &str,
    out: &mut Vec<McpServer>,
) {
    let Some(servers) = v.and_then(Value::as_object) else {
        return;
    };
    for (name, cfg) in servers {
        let url = cfg.get("url").and_then(Value::as_str);
        let transport = cfg
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                if url.is_some() {
                    "http".into()
                } else {
                    "stdio".into()
                }
            });
        let command = cfg.get("command").and_then(Value::as_str);
        let args: Vec<String> = cfg
            .get("args")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        let mut secret_keys: Vec<String> = ["env", "headers"]
            .iter()
            .filter_map(|k| cfg.get(*k).and_then(Value::as_object))
            .flatten()
            .filter(|(k, v)| v.as_str().is_some_and(|v| looks_secret(k, v)))
            .map(|(k, _)| k.clone())
            .collect();
        // A token pasted into the arguments (`--api-key sk-...`).
        if args
            .iter()
            .any(|a| secret_rules().mask_secrets(a) != a.as_str())
        {
            secret_keys.push("args".into());
        }
        out.push(McpServer {
            agent: "claude-code".into(),
            name: name.clone(),
            transport,
            target: url.and_then(host_of).or_else(|| command.map(basename)),
            scope: scope.into(),
            project: project.clone(),
            defined_in: file.into(),
            calls_30d: 0,
            outside_content: false,
            unpinned: command.is_some_and(|c| unpinned(&basename(c), &args)),
            secret_keys,
            plain_http: url.is_some_and(is_plain_http),
        });
    }
}

/// A remote URL over plain http (loopback is fine).
fn is_plain_http(url: &str) -> bool {
    url.starts_with("http://")
        && ![
            "http://localhost",
            "http://127.0.0.1",
            "http://[::1]",
            "http://0.0.0.0",
        ]
        .iter()
        .any(|l| url.starts_with(l))
}

/// A launcher (`npx`, `uvx`, ...) asked for a package with no version, so
/// each start runs whatever was published last.
fn unpinned(program: &str, args: &[String]) -> bool {
    let launcher = matches!(program, "npx" | "bunx" | "pnpx" | "uvx" | "pipx");
    if !launcher {
        return false;
    }
    let mut it = args
        .iter()
        .filter(|a| !a.starts_with('-') && a.as_str() != "run" && a.as_str() != "dlx");
    let Some(pkg) = it.next() else {
        return false;
    };
    if pkg.contains('/') && (pkg.starts_with('.') || pkg.starts_with('/')) {
        return false;
    }
    // npm: name@1.2.3 or @scope/name@1.2.3; Python: name==1.2.3.
    let version = pkg
        .rfind('@')
        .filter(|i| *i > 0)
        .map(|i| &pkg[i + 1..])
        .or_else(|| pkg.split_once("==").map(|(_, v)| v));
    !version.is_some_and(|v| v.starts_with(|c: char| c.is_ascii_digit()))
}

/// A config value that looks like a secret written in plain text, not a
/// reference to one (`${TOKEN}`, `$TOKEN`, `env:TOKEN`).
fn looks_secret(key: &str, value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() || v.starts_with('$') || v.starts_with("env:") || v.starts_with("op://") {
        return false;
    }
    if secret_rules().mask_secrets(v) != v {
        return true;
    }
    let k = key.to_lowercase();
    let named = [
        "token",
        "secret",
        "password",
        "passwd",
        "api_key",
        "apikey",
        "api-key",
        "auth",
        "credential",
    ]
    .iter()
    .any(|w| k.contains(w));
    let bearer = v.to_lowercase().starts_with("bearer ");
    // "Bearer abc..." headers, or a long value with no spaces under a
    // secret-ish name (a sentence isn't a token).
    named && (bearer || (v.len() >= 16 && !v.contains(' ')))
}

/// Built-in secret patterns, compiled once.
fn secret_rules() -> &'static crate::audit::Rules {
    static RULES: std::sync::LazyLock<crate::audit::Rules> =
        std::sync::LazyLock::new(crate::audit::Rules::builtin);
    &RULES
}

fn read_hooks(v: &Value, agent: &str, scope: &str, file: &str, out: &mut Vec<Hook>) {
    let Some(events) = v.get("hooks").and_then(Value::as_object) else {
        return;
    };
    for (event, groups) in events {
        for g in groups.as_array().into_iter().flatten() {
            let matcher = g.get("matcher").and_then(Value::as_str).map(str::to_string);
            for h in g
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(cmd) = h.get("command").and_then(Value::as_str) else {
                    continue;
                };
                let missing = command_program(cmd)
                    .filter(|p| p.starts_with('/') && !p.contains('$'))
                    .is_some_and(|p| !Path::new(p).exists());
                out.push(Hook {
                    agent: agent.into(),
                    event: event.clone(),
                    matcher: matcher.clone().filter(|m| !m.is_empty()),
                    command: cmd.to_string(),
                    scope: scope.into(),
                    defined_in: file.into(),
                    missing,
                    runs_30d: 0,
                    errors_30d: 0,
                });
            }
        }
    }
}

/// Instructions that disagree with each other or with the settings: two
/// package managers named for one project, or a command the instructions
/// say never to run that an allow rule lets run without asking.
fn instruction_conflicts(
    projects: &[String],
    settings: &[SettingsFile],
    look: &mut impl FnMut(&str, String, String, Option<String>),
) {
    use crate::follow::RuleKind;
    for p in projects {
        let files = crate::follow::instruction_files(Path::new(p));
        if files.is_empty() {
            continue;
        }
        let rules: Vec<crate::follow::Rule> = files
            .iter()
            .flat_map(|(f, t)| crate::follow::extract(f, t))
            .collect();
        let mut managers: BTreeMap<String, String> = BTreeMap::new();
        for r in &rules {
            if let RuleKind::PackageManager { tool } = &r.kind {
                managers
                    .entry(tool.clone())
                    .or_insert_with(|| r.file.clone());
            }
        }
        if managers.len() > 1 {
            look(
                "instructions_conflict",
                format!("{} names more than one package manager", basename(p)),
                managers
                    .iter()
                    .map(|(t, f)| format!("{t} ({})", basename(f)))
                    .collect::<Vec<_>>()
                    .join(" and "),
                None,
            );
        }
        let allows: Vec<(&String, &SettingsFile)> = settings
            .iter()
            .filter(|s| {
                s.agent == "claude-code"
                    && (s.project.is_none() || s.project.as_deref() == Some(p.as_str()))
            })
            .flat_map(|s| s.allow.iter().map(move |a| (a, s)))
            .collect();
        for r in &rules {
            let RuleKind::DontRun { command } = &r.kind else {
                continue;
            };
            let Some(key) = crate::follow::key(command) else {
                continue;
            };
            for (a, s) in &allows {
                let Some(inner) = a.strip_prefix("Bash(").and_then(|x| x.strip_suffix(')')) else {
                    continue;
                };
                let prefix = inner.trim_end_matches(":*").trim_end_matches('*').trim();
                if !prefix.is_empty() && (key.starts_with(prefix) || prefix.starts_with(&key)) {
                    look(
                        "instructions_conflict",
                        format!(
                            "{} says never run `{command}`, but {a} allows it",
                            basename(&r.file)
                        ),
                        format!(
                            "In {} for {}. The agent won't be asked before running it.",
                            basename(&s.path),
                            basename(p)
                        ),
                        Some(s.path.clone()),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_programs() {
        assert_eq!(
            host_of("https://u:p@mcp.example.com/x?t=1").as_deref(),
            Some("mcp.example.com")
        );
        assert_eq!(
            command_program("bash /opt/hooks/check.sh --x"),
            Some("/opt/hooks/check.sh")
        );
        assert_eq!(command_program("FOO=1 node x.js"), Some("x.js"));
    }

    #[test]
    fn hooks_with_missing_programs() {
        let v: Value = serde_json::from_str(
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/nowhere/at/all/notify.sh"}]}],
               "PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"agentcam guard"}]}]}}"#,
        )
        .unwrap();
        let mut out = Vec::new();
        read_hooks(&v, "claude-code", "user", "settings.json", &mut out);
        let stop = out.iter().find(|h| h.event == "Stop").unwrap();
        assert!(stop.missing);
        let pre = out.iter().find(|h| h.event == "PreToolUse").unwrap();
        assert!(!pre.missing);
        assert_eq!(pre.matcher.as_deref(), Some("Bash"));
    }

    #[test]
    fn mcp_config_flags() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(unpinned("npx", &a(&["-y", "@sentry/mcp-server"])));
        assert!(unpinned("npx", &a(&["-y", "@sentry/mcp-server@latest"])));
        assert!(!unpinned("npx", &a(&["-y", "@sentry/mcp-server@1.4.2"])));
        assert!(!unpinned("uvx", &a(&["mcp-server-git==0.6.2"])));
        assert!(!unpinned("node", &a(&["server.js"])));
        assert!(looks_secret(
            "GITHUB_TOKEN",
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789"
        ));
        assert!(looks_secret("Authorization", "Bearer abc123"));
        assert!(!looks_secret("GITHUB_TOKEN", "${GITHUB_TOKEN}"));
        assert!(!looks_secret("LOG_LEVEL", "debug-verbose-mode-on"));
        assert!(is_plain_http("http://mcp.example.dev/sse"));
        assert!(!is_plain_http("http://localhost:3000/mcp"));
    }
}
