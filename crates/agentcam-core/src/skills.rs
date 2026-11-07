//! Skills: which ones exist, which ones agents and you ran, what they cost
//! in context, and how turns that used them went.
//!
//! Uses come from the logs (Claude Code's Skill tool, slash commands that
//! name a skill, `invoked_skills`; either agent reading a `SKILL.md`).
//! The inventory reads skill folders on disk; only names, descriptions and
//! sizes, never anything else.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::index::Result;
use crate::model::{EventKind, ParsedSession, SkillUse};
use crate::query::{Filter, conditions};

/// Skill name from a path to a `SKILL.md`: the folder it sits in.
pub fn skill_from_path(text: &str) -> Option<String> {
    let i = text.find("/SKILL.md")?;
    let before = &text[..i];
    let start = before
        .rfind(['/', ' ', '\'', '"', '\t'])
        .map_or(0, |j| j + 1);
    let name = &before[start..];
    // Real folder names only, not shell patterns like `*` or `$S`.
    let ok = name.len() <= 80
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:-".contains(c));
    ok.then(|| name.to_string())
}

/// Skill reads both agents do by hand: a Read of, or a command that
/// prints, a `SKILL.md`. Skipped when the same skill was just run with
/// the Skill tool, which loads it anyway.
pub fn detect_reads(p: &ParsedSession) -> Vec<SkillUse> {
    let mut out: Vec<SkillUse> = Vec::new();
    for e in &p.events {
        if e.kind != EventKind::ToolCall || !e.title.contains("SKILL.md") {
            continue;
        }
        let Some(skill) = skill_from_path(&e.title) else {
            continue;
        };
        let near = p
            .skill_uses
            .iter()
            .chain(out.iter())
            .any(|u| u.skill == skill && e.seq.saturating_sub(u.seq) <= 3);
        if !near {
            out.push(SkillUse {
                seq: e.seq,
                timestamp: e.timestamp.clone(),
                skill,
                trigger: "read".into(),
                args: None,
                loaded_chars: None,
            });
        }
    }
    out
}

/// A skill found on disk.
#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub name: String,
    pub description: Option<String>,
    /// `claude-user`, `claude-plugin`, `claude-project`, `codex`.
    pub source: String,
    pub path: String,
    pub chars: u64,
}

fn frontmatter(text: &str) -> (Option<String>, Option<String>) {
    let mut name = None;
    let mut desc = None;
    let Some(rest) = text.strip_prefix("---") else {
        return (None, None);
    };
    for line in rest.lines().skip(1) {
        if line.trim() == "---" {
            break;
        }
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(v.trim().trim_matches(['"', '\'']).to_string());
        } else if let Some(v) = line.strip_prefix("description:") {
            desc = Some(v.trim().trim_matches(['"', '\'']).to_string());
        }
    }
    (name, desc)
}

/// `SKILL.md` files under `dir`, at most `depth` folders down.
fn find_skills(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, budget: &mut usize) {
    if depth == 0 || *budget == 0 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        *budget = budget.saturating_sub(1);
        if *budget == 0 {
            return;
        }
        let p = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let skip = p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == "node_modules" || n == ".git");
            if !skip {
                find_skills(&p, depth - 1, out, budget);
            }
        } else if p.file_name().is_some_and(|n| n == "SKILL.md") {
            out.push(p);
        }
    }
}

/// Skills installed for Claude Code (user, plugins, the given projects)
/// and Codex, read from their folders under `home`.
pub fn installed(home: &Path, projects: &[String]) -> Vec<Installed> {
    let d = crate::ConfigDirs::new(home);
    let mut roots: Vec<(PathBuf, &str, usize)> = vec![
        (d.claude.join("skills"), "claude-user", 3),
        (d.claude.join("plugins"), "claude-plugin", 8),
        (d.codex.join("skills"), "codex", 3),
    ];
    for p in projects {
        roots.push((Path::new(p).join(".claude/skills"), "claude-project", 3));
    }
    let mut out: Vec<Installed> = Vec::new();
    for (dir, source, depth) in roots {
        let mut files = Vec::new();
        let mut budget = 5000;
        find_skills(&dir, depth, &mut files, &mut budget);
        for f in files {
            let Ok(text) = std::fs::read_to_string(&f) else {
                continue;
            };
            let folder = f
                .parent()
                .and_then(|d| d.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let (name, description) = frontmatter(&text);
            let name = name.unwrap_or(folder);
            // The same plugin skill can sit in a cache more than once.
            if out.iter().any(|x| x.name == name && x.source == source) {
                continue;
            }
            out.push(Installed {
                name,
                description: description.map(|d| crate::model::truncate(&d, 300)),
                source: source.to_string(),
                path: f.to_string_lossy().to_string(),
                chars: text.chars().count() as u64,
            });
        }
    }
    out
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SkillRow {
    pub name: String,
    pub description: Option<String>,
    /// Where it's installed; empty when only seen in logs.
    pub sources: Vec<String>,
    pub file_chars: Option<u64>,
    /// Uses by trigger: `agent`, `you`, `read`.
    pub uses: BTreeMap<String, i64>,
    pub sessions: i64,
    /// Steps the agent took under the skill (Claude Code attribution).
    pub steps: i64,
    pub errored_steps: i64,
    pub avg_loaded_chars: Option<i64>,
    /// Sessions where the agent said it was available.
    pub listed_in: i64,
    pub last_used: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillUseRow {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub skill: String,
    pub trigger: String,
    pub args: Option<String>,
    pub agent: String,
    pub title: Option<String>,
    pub project_root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub skills: Vec<SkillRow>,
    /// Sessions that listed skills, and the average listing size.
    pub listing_sessions: i64,
    pub avg_listed: f64,
    pub avg_listing_chars: i64,
    pub recent: Vec<SkillUseRow>,
}

pub fn report(conn: &Connection, f: &Filter, home: &Path) -> Result<Report> {
    let (cond, args) = conditions(f);
    let mut rows: HashMap<String, SkillRow> = HashMap::new();
    let mut stmt = conn.prepare(&format!(
        "SELECT u.skill, u.trigger, count(*), count(DISTINCT coalesce(s.parent_id, s.id)), max(u.ts),
                avg(u.loaded_chars)
         FROM skill_uses u JOIN sessions s ON s.id = u.session_id
         JOIN events e ON e.session_id = u.session_id AND e.seq = u.seq
         WHERE {cond} GROUP BY 1, 2"
    ))?;
    let mut q = stmt.query(params_from_iter(&args))?;
    while let Some(r) = q.next()? {
        let name: String = r.get(0)?;
        let x = rows.entry(name.clone()).or_insert_with(|| SkillRow {
            name,
            ..Default::default()
        });
        *x.uses.entry(r.get(1)?).or_default() += r.get::<_, i64>(2)?;
        x.sessions += r.get::<_, i64>(3)?;
        let last: Option<String> = r.get(4)?;
        if last > x.last_used {
            x.last_used = last;
        }
        if let Some(avg) = r.get::<_, Option<f64>>(5)? {
            x.avg_loaded_chars = Some(avg.round() as i64);
        }
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT substr(e.via, 7), count(*), sum(e.is_error), max(e.ts)
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.via LIKE 'skill:%' AND {cond} GROUP BY 1"
    ))?;
    let mut q = stmt.query(params_from_iter(&args))?;
    while let Some(r) = q.next()? {
        let name: String = r.get(0)?;
        let x = rows.entry(name.clone()).or_insert_with(|| SkillRow {
            name,
            ..Default::default()
        });
        x.steps += r.get::<_, i64>(1)?;
        x.errored_steps += r.get::<_, i64>(2)?;
        let last: Option<String> = r.get(3)?;
        if last > x.last_used {
            x.last_used = last;
        }
    }

    // Availability: sessions in range that listed each skill.
    let mut stmt = conn.prepare(&format!(
        "SELECT k.name, count(DISTINCT k.session_id) FROM session_skills k
         JOIN sessions s ON s.id = k.session_id
         WHERE k.session_id IN (SELECT DISTINCT e.session_id FROM events e JOIN sessions s ON s.id = e.session_id
                                WHERE {cond})
         GROUP BY 1"
    ))?;
    let mut q = stmt.query(params_from_iter(&args))?;
    while let Some(r) = q.next()? {
        let name: String = r.get(0)?;
        let x = rows.entry(name.clone()).or_insert_with(|| SkillRow {
            name,
            ..Default::default()
        });
        x.listed_in = r.get(1)?;
    }

    let (listing_sessions, avg_listed, avg_listing_chars): (i64, Option<f64>, Option<f64>) = conn
        .query_row(
            &format!(
                "SELECT count(*), avg(s.skills_listed), avg(s.skills_listing_chars) FROM sessions s
                 WHERE s.skills_listed > 0 AND s.id IN (
                    SELECT DISTINCT e.session_id FROM events e JOIN sessions s ON s.id = e.session_id WHERE {cond})"
            ),
            params_from_iter(&args),
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;

    let projects: Vec<String> = conn
        .prepare("SELECT DISTINCT project_root FROM sessions WHERE project_root IS NOT NULL")?
        .query_map([], |r| r.get(0))?
        .filter_map(std::result::Result::ok)
        .collect();
    for inst in installed(home, &projects) {
        // Plugin skills are listed as `plugin:skill`; match either form.
        let key = rows
            .keys()
            .find(|k| **k == inst.name || k.rsplit(':').next() == Some(inst.name.as_str()))
            .cloned()
            .unwrap_or_else(|| inst.name.clone());
        let x = rows.entry(key.clone()).or_insert_with(|| SkillRow {
            name: key,
            ..Default::default()
        });
        if x.description.is_none() {
            x.description = inst.description.clone();
        }
        if !x.sources.contains(&inst.source) {
            x.sources.push(inst.source.clone());
        }
        x.file_chars = Some(inst.chars);
    }

    let mut skills: Vec<SkillRow> = rows.into_values().collect();
    skills.sort_by(|a, b| {
        let n = |x: &SkillRow| x.uses.values().sum::<i64>() + x.steps;
        n(b).cmp(&n(a))
            .then(b.listed_in.cmp(&a.listed_in))
            .then(a.name.cmp(&b.name))
    });

    let mut stmt = conn.prepare(&format!(
        "SELECT u.session_id, u.seq, u.ts, u.skill, u.trigger, u.args, s.agent,
                coalesce(r.title, r.first_prompt), s.project_root
         FROM skill_uses u JOIN sessions s ON s.id = u.session_id
         JOIN events e ON e.session_id = u.session_id AND e.seq = u.seq
         LEFT JOIN sessions r ON r.id = coalesce(s.parent_id, s.id)
         WHERE {cond} ORDER BY u.ts DESC LIMIT 40"
    ))?;
    let recent = stmt
        .query_map(params_from_iter(&args), |r| {
            Ok(SkillUseRow {
                session_id: r.get(0)?,
                seq: r.get(1)?,
                ts: r.get(2)?,
                skill: r.get(3)?,
                trigger: r.get(4)?,
                args: r.get(5)?,
                agent: r.get(6)?,
                title: r.get(7)?,
                project_root: r.get(8)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    Ok(Report {
        skills,
        listing_sessions,
        avg_listed: avg_listed.unwrap_or(0.0),
        avg_listing_chars: avg_listing_chars.unwrap_or(0.0).round() as i64,
        recent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_names_from_paths() {
        assert_eq!(
            skill_from_path("cat /Users/dev/.codex/skills/brag/SKILL.md").as_deref(),
            Some("brag")
        );
        assert_eq!(
            skill_from_path("sed -n '1,80p' '/x/skills/write-like-me/SKILL.md'").as_deref(),
            Some("write-like-me")
        );
        assert_eq!(skill_from_path("README.md"), None);
        assert_eq!(skill_from_path("cat skills/*/SKILL.md"), None);
        assert_eq!(skill_from_path("cat \"$S/SKILL.md\""), None);
    }

    #[test]
    fn reads_frontmatter() {
        let (n, d) =
            frontmatter("---\nname: brag\ndescription: \"Keep a brag doc\"\n---\n# Brag\n");
        assert_eq!(n.as_deref(), Some("brag"));
        assert_eq!(d.as_deref(), Some("Keep a brag doc"));
    }
}
