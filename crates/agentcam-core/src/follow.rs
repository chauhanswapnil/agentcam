//! Follow-through: does the work match what a project's AGENTS.md or
//! CLAUDE.md asks? Only rules a log can check are pulled out:
//!
//! - **checks**: commands listed under a "before you finish" style heading
//!   (or a line saying they must pass) should run after the last edit;
//! - **package manager**: a project that only shows `pnpm`, `yarn` or `bun`
//!   commands shouldn't see `npm` or another manager used;
//! - **don't edit**: paths a line says never to edit, or marks generated;
//! - **don't run**: commands a line says never to run.
//!
//! The files are read as they are today; older sessions are checked
//! against today's rules. Wording stays neutral: a session "didn't run"
//! something, it didn't "fail".

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::index::Result;
use crate::query::{Filter, conditions};

/// Words a checkable command starts with.
const TOOLS: &[&str] = &[
    "cargo",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "npx",
    "pytest",
    "python",
    "python3",
    "go",
    "make",
    "just",
    "ruff",
    "eslint",
    "tsc",
    "swift",
    "xcodebuild",
    "mvn",
    "gradle",
    "./gradlew",
    "uv",
    "poetry",
    "deno",
    "flutter",
    "dotnet",
    "bundle",
    "rake",
    "rspec",
    "mix",
    "composer",
    "php",
    "black",
    "mypy",
    "prettier",
    "biome",
    "oxlint",
    "vitest",
    "jest",
    "git",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleKind {
    /// Run these after the last edit (only when files under `when` changed).
    Checks {
        commands: Vec<String>,
        when: Option<String>,
    },
    PackageManager {
        tool: String,
    },
    DontEdit {
        path: String,
    },
    DontRun {
        command: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub id: String,
    #[serde(flatten)]
    pub kind: RuleKind,
    /// The instruction line it came from.
    pub source: String,
    pub file: String,
}

/// Instruction files for a project, with `@file` imports followed once.
pub fn instruction_files(root: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for name in ["AGENTS.md", "CLAUDE.md", ".claude/CLAUDE.md"] {
        let p = root.join(name);
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        // `@AGENTS.md` in CLAUDE.md pulls that file in.
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix('@') {
                let inc = root.join(rest.trim());
                if let Ok(t) = std::fs::read_to_string(&inc) {
                    let key = inc.to_string_lossy().to_string();
                    if !out.iter().any(|(k, _)| *k == key) {
                        out.push((key, t));
                    }
                }
            }
        }
        let key = p.to_string_lossy().to_string();
        if !out.iter().any(|(k, _)| *k == key) {
            out.push((key, text));
        }
    }
    out
}

const EDIT_VERBS: &[&str] = &[
    "edit",
    "modify",
    "change",
    "touch",
    "write to",
    "hand-edit",
    "commit",
];
const RUN_VERBS: &[&str] = &["run", "use", "call", "execute"];

/// Text after each "never <verb>" / "don't <verb>" / "do not <verb>",
/// up to the end of that clause.
fn clauses_after(line: &str, verbs: &[&str]) -> Vec<String> {
    let lower = line.to_lowercase();
    let mut out = Vec::new();
    for neg in ["never ", "don't ", "do not ", "dont ", "must not "] {
        let mut from = 0;
        while let Some(i) = lower[from..].find(neg) {
            let start = from + i + neg.len();
            from = start;
            let rest = &lower[start..];
            let Some(verb) = verbs.iter().find(|v| rest.starts_with(*v)) else {
                continue;
            };
            let tail = &line[start + verb.len()..];
            let end = [". ", "; ", ", but", " instead"]
                .iter()
                .filter_map(|e| tail.find(e))
                .min()
                .unwrap_or(tail.len());
            out.push(tail[..end].to_string());
        }
    }
    out
}

/// `src/generated/`, `web/dist`, `schema.sql`: yes. `Task.sleep(for:)`: no.
fn looks_like_path(s: &str) -> bool {
    !s.contains(' ')
        && !s.contains(['(', ')', ':', '*', '$', '<', '>', '='])
        && !s.starts_with('.')
        && (s.contains('/')
            || s.rsplit_once('.').is_some_and(|(stem, ext)| {
                !stem.is_empty()
                    && (1..=5).contains(&ext.len())
                    && ext.chars().all(|c| c.is_ascii_alphanumeric())
            }))
}

/// Backticked spans in a line.
fn ticks(line: &str) -> Vec<String> {
    line.split('`')
        .enumerate()
        .filter(|(i, s)| i % 2 == 1 && !s.trim().is_empty())
        .map(|(_, s)| s.trim().to_string())
        .collect()
}

fn is_command(s: &str) -> bool {
    let first = s.split_whitespace().next().unwrap_or("");
    TOOLS.contains(&first) && s.contains(' ')
}

/// The part of a command a rule matches on: `cargo clippy --all-targets`
/// → `cargo clippy`, `npm run build` → `npm run build`.
pub fn key(cmd: &str) -> Option<String> {
    crate::approvals::command_prefix(cmd)
}

/// Keys of every simple command in a shell line (`cd x && cargo test`).
fn keys_in(cmd: &str) -> Vec<String> {
    crate::shell::command_words(cmd)
        .into_iter()
        .filter(|w| w.first().is_some_and(|c| c != "cd"))
        .filter_map(|w| key(&w.join(" ")))
        .collect()
}

pub fn extract(file: &str, text: &str) -> Vec<Rule> {
    let mut rules: Vec<Rule> = Vec::new();
    let mut heading = String::new();
    let mut in_fence = false;
    let mut fence_cmds: Vec<String> = Vec::new();
    let checklist = |h: &str| {
        let h = h.to_lowercase();
        [
            "before you finish",
            "before finishing",
            "before you commit",
            "before committing",
            "before you push",
            "checklist",
            "definition of done",
            "before submitting",
            "pre-commit",
            "verify",
        ]
        .iter()
        .any(|k| h.contains(k))
    };
    let mut all_cmds: Vec<String> = Vec::new();
    let add = |rules: &mut Vec<Rule>, kind: RuleKind, source: &str| {
        if rules.iter().any(|r| r.kind == kind) {
            return;
        }
        let id = format!("r{}", rules.len() + 1);
        rules.push(Rule {
            id,
            kind,
            source: crate::model::truncate(source.trim(), 300),
            file: file.to_string(),
        });
    };

    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("```") {
            if in_fence && checklist(&heading) && !fence_cmds.is_empty() {
                add(
                    &mut rules,
                    RuleKind::Checks {
                        commands: std::mem::take(&mut fence_cmds),
                        when: None,
                    },
                    &heading,
                );
            }
            fence_cmds.clear();
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            // `ui-dist/   built web UI (generated, gitignored)` in a layout block.
            if line.to_lowercase().contains("generated")
                && let Some(first) = line
                    .split_whitespace()
                    .next()
                    .filter(|w| w.ends_with('/') && w.len() > 2)
            {
                add(
                    &mut rules,
                    RuleKind::DontEdit {
                        path: first.trim_matches('/').to_string(),
                    },
                    line,
                );
                continue;
            }
            let cmd = line.split(" #").next().unwrap_or(line).trim();
            if is_command(cmd) {
                all_cmds.push(cmd.to_string());
                fence_cmds.push(cmd.to_string());
            }
            continue;
        }
        if let Some(h) = line.strip_prefix('#') {
            heading = h.trim_start_matches('#').trim().to_string();
            continue;
        }
        let lower = line.to_lowercase();
        let spans = ticks(line);
        let cmds: Vec<String> = spans.iter().filter(|s| is_command(s)).cloned().collect();
        all_cmds.extend(cmds.iter().cloned());

        // Only what follows "never edit" / "don't run" in the same clause.
        let mut negative = false;
        for (verbs, edit) in [(EDIT_VERBS, true), (RUN_VERBS, false)] {
            for clause in clauses_after(line, verbs) {
                negative = true;
                for t in ticks(&clause) {
                    if edit && looks_like_path(&t) {
                        add(
                            &mut rules,
                            RuleKind::DontEdit {
                                path: t.trim_matches('/').to_string(),
                            },
                            line,
                        );
                    } else if !edit && is_command(&t) {
                        add(&mut rules, RuleKind::DontRun { command: t.clone() }, line);
                    }
                }
            }
        }
        if negative {
            continue;
        }
        if cmds.is_empty() {
            continue;
        }
        let must = [
            "pass",
            "clean",
            "must run",
            "run before",
            "before you finish",
            "should pass",
            "green",
        ]
        .iter()
        .any(|k| lower.contains(k));
        if checklist(&heading) || must {
            // "If you touched `web/`: ..." limits the rule to that folder.
            let when = lower
                .find("if you touched")
                .or_else(|| lower.find("if you changed"))
                .and(
                    spans
                        .iter()
                        .find(|s| !is_command(s) && (s.ends_with('/') || !s.contains(' ')))
                        .cloned(),
                )
                .map(|p| p.trim_matches('/').to_string());
            add(
                &mut rules,
                RuleKind::Checks {
                    commands: cmds,
                    when,
                },
                line,
            );
        }
    }

    // A project that only ever shows one non-npm package manager uses it.
    let managers = ["pnpm", "yarn", "bun"];
    let used: HashSet<&str> = all_cmds
        .iter()
        .filter_map(|c| c.split_whitespace().next())
        .filter(|w| managers.contains(w) || *w == "npm" || *w == "npx")
        .collect();
    if used.len() == 1
        && let Some(m) = used.iter().find(|w| managers.contains(*w))
    {
        let src = all_cmds
            .iter()
            .find(|c| c.starts_with(*m))
            .cloned()
            .unwrap_or_default();
        add(
            &mut rules,
            RuleKind::PackageManager {
                tool: m.to_string(),
            },
            &format!("Commands use `{src}`"),
        );
    }
    rules
}

/// One rule checked against one session.
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub rule_id: String,
    /// `followed`, `not_followed` or `not_applicable`.
    pub status: &'static str,
    /// What was missing or what went against the rule, in plain words.
    pub detail: Option<String>,
    /// The step to look at.
    pub seq: Option<i64>,
    pub session_id: String,
}

struct Step {
    session_id: String,
    seq: i64,
    ts: String,
    command: Option<String>,
    edit: Option<String>,
}

fn steps(conn: &Connection, id: &str) -> Result<Vec<Step>> {
    let mut stmt = conn.prepare(
        "SELECT e.session_id, e.seq, e.ts, CASE WHEN e.category = 'command' THEN e.title END,
                (SELECT t.path FROM file_touches t WHERE t.session_id = e.session_id AND t.seq = e.seq
                    AND t.action != 'read' LIMIT 1)
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE (s.id = ?1 OR s.parent_id = ?1) AND e.ts IS NOT NULL ORDER BY e.ts, e.seq",
    )?;
    let rows = stmt.query_map([id], |r| {
        Ok(Step {
            session_id: r.get(0)?,
            seq: r.get(1)?,
            ts: r.get(2)?,
            command: r.get(3)?,
            edit: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn rel<'a>(path: &'a str, root: &str) -> &'a str {
    path.strip_prefix(root)
        .map(|p| p.trim_start_matches('/'))
        .unwrap_or(path)
}

fn check(rule: &Rule, root: &str, steps: &[Step], session_id: &str) -> Outcome {
    let out = |status, detail: Option<String>, seq: Option<i64>, sid: Option<&str>| Outcome {
        rule_id: rule.id.clone(),
        status,
        detail,
        seq,
        session_id: sid.unwrap_or(session_id).to_string(),
    };
    match &rule.kind {
        RuleKind::Checks { commands, when } => {
            let edits: Vec<&Step> = steps
                .iter()
                .filter(|s| {
                    s.edit.as_deref().is_some_and(|p| {
                        let r = rel(p, root);
                        // Docs-only edits don't call for the checks.
                        !r.ends_with(".md") && when.as_deref().is_none_or(|w| r.starts_with(w))
                    })
                })
                .collect();
            let Some(last) = edits.last() else {
                return out("not_applicable", None, None, None);
            };
            let after: Vec<String> = steps
                .iter()
                .filter(|s| s.ts >= last.ts)
                .filter_map(|s| s.command.as_deref())
                .flat_map(keys_in)
                .collect();
            let missing: Vec<&String> = commands
                .iter()
                .filter(|c| key(c).is_none_or(|k| !after.contains(&k)))
                .collect();
            if missing.is_empty() {
                out("followed", None, None, None)
            } else {
                let list = missing
                    .iter()
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                out(
                    "not_followed",
                    Some(format!("Didn't run {list} after the last edit")),
                    Some(last.seq),
                    Some(&last.session_id),
                )
            }
        }
        RuleKind::PackageManager { tool } => {
            let other = steps.iter().find_map(|s| {
                let cmd = s.command.as_deref()?;
                let k = keys_in(cmd).into_iter().find(|k| {
                    let first = k.split(' ').next().unwrap_or("");
                    ["npm", "npx", "yarn", "pnpm", "bun"].contains(&first) && first != tool
                })?;
                Some((s, k))
            });
            let used = steps.iter().any(|s| s.command.is_some());
            match other {
                Some((s, k)) => out(
                    "not_followed",
                    Some(format!("Used `{k}` where the project uses {tool}")),
                    Some(s.seq),
                    Some(&s.session_id),
                ),
                None if used => out("followed", None, None, None),
                None => out("not_applicable", None, None, None),
            }
        }
        RuleKind::DontEdit { path } => {
            let hit = steps.iter().find(|s| {
                s.edit.as_deref().is_some_and(|p| {
                    let r = rel(p, root);
                    r == path
                        || r.starts_with(&format!("{path}/"))
                        || r.contains(&format!("/{path}/"))
                })
            });
            match hit {
                Some(s) => out(
                    "not_followed",
                    Some(format!(
                        "Changed {}",
                        rel(s.edit.as_deref().unwrap_or(""), root)
                    )),
                    Some(s.seq),
                    Some(&s.session_id),
                ),
                None if steps.iter().any(|s| s.edit.is_some()) => out("followed", None, None, None),
                None => out("not_applicable", None, None, None),
            }
        }
        RuleKind::DontRun { command } => {
            let k = key(command);
            let hit = steps.iter().find(|s| {
                s.command
                    .as_deref()
                    .is_some_and(|c| k.as_ref().is_some_and(|k| keys_in(c).contains(k)))
            });
            match hit {
                Some(s) => out(
                    "not_followed",
                    Some(format!("Ran `{}`", s.command.as_deref().unwrap_or(""))),
                    Some(s.seq),
                    Some(&s.session_id),
                ),
                None if steps.iter().any(|s| s.command.is_some()) => {
                    out("followed", None, None, None)
                }
                None => out("not_applicable", None, None, None),
            }
        }
    }
}

/// Rule results for one session (and its subagents).
#[derive(Debug, Clone, Serialize)]
pub struct SessionFollow {
    pub files: Vec<String>,
    pub rules: Vec<Rule>,
    pub outcomes: Vec<Outcome>,
}

pub fn session(conn: &Connection, id: &str) -> Result<Option<SessionFollow>> {
    let root: Option<String> = conn
        .query_row(
            "SELECT project_root FROM sessions WHERE id = ?",
            [id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let Some(root) = root else {
        return Ok(None);
    };
    let files = instruction_files(Path::new(&root));
    if files.is_empty() {
        return Ok(None);
    }
    let rules: Vec<Rule> = number(files.iter().flat_map(|(f, t)| extract(f, t)).collect());
    if rules.is_empty() {
        return Ok(None);
    }
    let st = steps(conn, id)?;
    let outcomes = rules.iter().map(|r| check(r, &root, &st, id)).collect();
    Ok(Some(SessionFollow {
        files: files.into_iter().map(|(f, _)| f).collect(),
        rules,
        outcomes,
    }))
}

/// Ids unique across files.
fn number(mut rules: Vec<Rule>) -> Vec<Rule> {
    for (i, r) in rules.iter_mut().enumerate() {
        r.id = format!("r{}", i + 1);
    }
    rules
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleStats {
    pub rule: Rule,
    pub followed: i64,
    pub not_followed: i64,
    /// Latest sessions that didn't follow it, newest first.
    pub examples: Vec<Example>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Example {
    pub session_id: String,
    pub seq: Option<i64>,
    pub title: Option<String>,
    pub ts: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectFollow {
    pub project: String,
    pub files: Vec<String>,
    pub sessions: i64,
    pub rules: Vec<RuleStats>,
}

/// Every project with instruction files and sessions in range.
pub fn report(conn: &Connection, f: &Filter) -> Result<Vec<ProjectFollow>> {
    let (cond, args) = conditions(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT s.id, s.project_root, coalesce(s.title, s.first_prompt), s.ended_at FROM sessions s
         WHERE s.parent_id IS NULL AND s.project_root IS NOT NULL AND s.id IN (
            SELECT DISTINCT coalesce(s.parent_id, s.id) FROM events e JOIN sessions s ON s.id = e.session_id
            WHERE {cond})
         ORDER BY s.ended_at DESC"
    ))?;
    let sessions: Vec<(String, String, Option<String>, Option<String>)> = stmt
        .query_map(params_from_iter(&args), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<std::result::Result<_, _>>()?;

    let mut out: Vec<ProjectFollow> = Vec::new();
    let mut roots: Vec<String> = sessions.iter().map(|s| s.1.clone()).collect();
    roots.sort();
    roots.dedup();
    for root in roots {
        let files = instruction_files(Path::new(&root));
        let rules = number(files.iter().flat_map(|(f, t)| extract(f, t)).collect());
        if rules.is_empty() {
            continue;
        }
        let mut stats: Vec<RuleStats> = rules
            .iter()
            .map(|r| RuleStats {
                rule: r.clone(),
                followed: 0,
                not_followed: 0,
                examples: Vec::new(),
            })
            .collect();
        let mine: Vec<&(String, String, Option<String>, Option<String>)> =
            sessions.iter().filter(|s| s.1 == root).collect();
        for (id, _, title, ts) in &mine {
            let st = steps(conn, id)?;
            for (r, s) in rules.iter().zip(stats.iter_mut()) {
                let o = check(r, &root, &st, id);
                match o.status {
                    "followed" => s.followed += 1,
                    "not_followed" => {
                        s.not_followed += 1;
                        if s.examples.len() < 8 {
                            s.examples.push(Example {
                                session_id: o.session_id,
                                seq: o.seq,
                                title: title.clone(),
                                ts: ts.clone(),
                                detail: o.detail,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        out.push(ProjectFollow {
            project: root,
            files: files.into_iter().map(|(f, _)| f).collect(),
            sessions: mine.len() as i64,
            rules: stats,
        });
    }
    out.sort_by_key(|p| std::cmp::Reverse(p.sessions));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_rules_need_the_verb_right_there() {
        let doc = "README screenshots must come from `python3 scripts/demo-data.py`, never real logs.\n\
            Don't add a fourth hue; put new kinds in `web/src/lib/kinds.ts`.\n\
            Prefer `Task.sleep(for:)`; never edit `src/schema.rs` by hand.\n\
            Never run `git push --force` on main.\n";
        let rules = extract("AGENTS.md", doc);
        let kinds: Vec<&RuleKind> = rules.iter().map(|r| &r.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &RuleKind::DontEdit {
                    path: "src/schema.rs".into()
                },
                &RuleKind::DontRun {
                    command: "git push --force".into()
                },
            ]
        );
    }

    const DOC: &str = "# Project\n\nUse `pnpm install` to set up.\n\
        Never edit `src/generated/` by hand.\n\
        Don't run `git push --force`.\n\n\
        ## Before you finish\n\n\
        1. `pnpm test` and `pnpm lint` pass.\n\
        2. If you touched `docs/`: `pnpm docs:build` passes.\n";

    #[test]
    fn extracts_checkable_rules() {
        let rules = extract("AGENTS.md", DOC);
        let kinds: Vec<&RuleKind> = rules.iter().map(|r| &r.kind).collect();
        assert!(kinds.contains(&&RuleKind::DontEdit {
            path: "src/generated".into()
        }));
        assert!(kinds.contains(&&RuleKind::DontRun {
            command: "git push --force".into()
        }));
        assert!(kinds.contains(&&RuleKind::Checks {
            commands: vec!["pnpm test".into(), "pnpm lint".into()],
            when: None
        }));
        assert!(kinds.contains(&&RuleKind::Checks {
            commands: vec!["pnpm docs:build".into()],
            when: Some("docs".into())
        }));
        assert!(kinds.contains(&&RuleKind::PackageManager {
            tool: "pnpm".into()
        }));
    }

    fn step(seq: i64, cmd: Option<&str>, edit: Option<&str>) -> Step {
        Step {
            session_id: "s".into(),
            seq,
            ts: format!("2026-11-01T00:00:{seq:02}.000Z"),
            command: cmd.map(String::from),
            edit: edit.map(String::from),
        }
    }

    #[test]
    fn checks_run_after_the_last_edit() {
        let rules = extract("AGENTS.md", DOC);
        let checks = rules
            .iter()
            .find(|r| matches!(&r.kind, RuleKind::Checks { when: None, .. }))
            .unwrap();
        let ok = [
            step(1, None, Some("/p/src/a.ts")),
            step(2, Some("cd /p && pnpm test -- --run"), None),
            step(3, Some("pnpm lint"), None),
        ];
        assert_eq!(check(checks, "/p", &ok, "s").status, "followed");
        let late_edit = [
            step(1, Some("pnpm test"), None),
            step(2, Some("pnpm lint"), None),
            step(3, None, Some("/p/src/a.ts")),
        ];
        let o = check(checks, "/p", &late_edit, "s");
        assert_eq!(o.status, "not_followed");
        assert_eq!(o.seq, Some(3));
        let docs_only = [step(1, None, Some("/p/README.md"))];
        assert_eq!(
            check(checks, "/p", &docs_only, "s").status,
            "not_applicable"
        );

        let pm = rules
            .iter()
            .find(|r| matches!(r.kind, RuleKind::PackageManager { .. }))
            .unwrap();
        assert_eq!(
            check(
                pm,
                "/p",
                &[step(1, Some("npm install left-pad"), None)],
                "s"
            )
            .status,
            "not_followed"
        );
        let ne = rules
            .iter()
            .find(|r| matches!(r.kind, RuleKind::DontEdit { .. }))
            .unwrap();
        assert_eq!(
            check(
                ne,
                "/p",
                &[step(1, None, Some("/p/src/generated/api.ts"))],
                "s"
            )
            .status,
            "not_followed"
        );
    }
}
