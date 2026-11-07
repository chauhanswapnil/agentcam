//! A team baseline for agent setup, and the machines that differ from it.
//!
//! An admin says which setup flags every machine should have (say, the
//! guard hook) and which none should (approvals skipped by default, a
//! secret in an MCP config), which MCP servers are allowed, and the oldest
//! agent versions to run. Each machine's setup facts (`store::facts`) are
//! compared with that, and each difference says since when it's been there.
//!
//! It describes; it never changes a machine. Teams act on it through MDM or
//! the agents' managed settings.

use std::collections::BTreeMap;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::store::{Store, short};

/// Setup flags a baseline can ask for or rule out, with neutral words.
pub const FLAGS: &[(&str, &str)] = &[
    ("guard_hook_installed", "agentcam guard hook installed"),
    (
        "approvals_skipped_by_default",
        "Approvals skipped by default",
    ),
    ("sandbox_off", "Codex sandbox off"),
    ("broad_allow_rule", "Allow rule covers every shell command"),
    ("danger_prompt_skipped", "Bypass-mode warning turned off"),
    (
        "outside_content_open_shell",
        "Outside content can reach a shell without asking",
    ),
    ("mcp_secret_in_config", "Secret written in an MCP config"),
    ("mcp_plain_http", "MCP server over plain http"),
    ("mcp_flagged_text", "MCP tool text aimed at the agent"),
    ("skill_flagged", "Skill with text or code worth a look"),
    (
        "instructions_flagged",
        "Instruction file with text aimed at the agent",
    ),
    ("hook_missing_program", "Hook pointing at a missing file"),
];

/// Tags on MCP servers and skills a baseline can rule out.
pub const TAGS: &[(&str, &str)] = &[
    ("secret_in_config", "secret in config"),
    ("unpinned", "runs the latest release"),
    ("plain_http", "plain http"),
    ("flagged_text", "tool text aimed at the agent"),
    ("description_changed", "tool description changed"),
    ("outside_content", "brings in outside content"),
    ("flagged", "worth a look"),
];

fn flag_title(id: &str) -> String {
    FLAGS
        .iter()
        .find(|(f, _)| *f == id)
        .map_or_else(|| id.replace('_', " "), |(_, t)| t.to_string())
}

fn tag_title(id: &str) -> String {
    TAGS.iter()
        .find(|(t, _)| *t == id)
        .map_or_else(|| id.replace('_', " "), |(_, t)| t.to_string())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    /// Flags every machine should have.
    #[serde(default)]
    pub require: Vec<String>,
    /// Flags no machine should have.
    #[serde(default)]
    pub forbid: Vec<String>,
    /// MCP server or skill tags no machine should have.
    #[serde(default)]
    pub forbid_tags: Vec<String>,
    /// MCP servers allowed by name; `None` allows any.
    #[serde(default)]
    pub mcp_allowlist: Option<Vec<String>>,
    /// Oldest version to run, per agent (`claude-code`, `codex`).
    #[serde(default)]
    pub min_versions: BTreeMap<String, String>,
}

impl Baseline {
    pub fn is_empty(&self) -> bool {
        *self == Baseline::default()
    }

    /// A starting point to edit: the guard hook in, the riskier flags out.
    pub fn suggested() -> Self {
        Baseline {
            require: vec!["guard_hook_installed".into()],
            forbid: vec![
                "approvals_skipped_by_default".into(),
                "sandbox_off".into(),
                "broad_allow_rule".into(),
                "outside_content_open_shell".into(),
                "mcp_secret_in_config".into(),
            ],
            forbid_tags: vec![
                "secret_in_config".into(),
                "flagged_text".into(),
                "flagged".into(),
            ],
            mcp_allowlist: None,
            min_versions: BTreeMap::new(),
        }
    }
}

pub fn load(store: &Store) -> Baseline {
    store
        .setting("baseline")
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(store: &Store, b: &Baseline) -> Result<()> {
    store.set_setting("baseline", &serde_json::to_string(b).unwrap_or_default())
}

#[derive(Debug, Clone, Serialize)]
pub struct Difference {
    /// `require:<flag>`, `forbid:<flag>`, `tag:<tag>`, `mcp:<name>`,
    /// `version:<agent>`.
    pub rule: String,
    pub title: String,
    pub since: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MachineDrift {
    pub machine: String,
    pub team: String,
    pub last_sync: Option<String>,
    pub differences: Vec<Difference>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub ts: String,
    pub machine: String,
    pub what: String,
    pub change: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineView {
    pub baseline: Baseline,
    pub set: bool,
    pub suggested: Baseline,
    pub flags: Vec<(String, String)>,
    pub tags: Vec<(String, String)>,
    pub machines: usize,
    pub matching: usize,
    /// Each rule and how many machines differ on it.
    pub by_rule: Vec<(String, usize)>,
    pub drift: Vec<MachineDrift>,
    /// Setup changes across machines in the last 30 days, newest first.
    pub changes: Vec<Change>,
}

/// `2.10.1` > `2.9`; anything that isn't a number compares as 0.
fn version_lt(a: &str, b: &str) -> bool {
    let parts = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parts(a), parts(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x < y;
        }
    }
    false
}

/// A fact in words, for the change list.
fn describe_fact(fact: &str) -> String {
    let p: Vec<&str> = fact.splitn(5, ':').collect();
    match p.as_slice() {
        ["flag", _, id] => flag_title(id),
        ["item", kind, _, name] => format!("{} {name}", kind.replace('_', " ")),
        ["tag", _, _, name, tag] => format!("{name}: {}", tag_title(tag)),
        ["version", agent, v] => format!("{agent} {v}"),
        _ => fact.to_string(),
    }
}

/// Machines in `team` (or all) compared with the saved baseline.
pub fn view(store: &Store, team: Option<i64>, now_ms: i64) -> Result<BaselineView> {
    let baseline = load(store);
    let people = crate::views::show_people(store);
    let conn = store.conn();
    let mut stmt = conn.prepare(
        "SELECT d.machine_id, t.name, d.os_user, d.last_sync, d.joined_at
         FROM devices d JOIN teams t ON t.id = d.team_id
         WHERE (?1 IS NULL OR d.team_id = ?1) ORDER BY d.machine_id",
    )?;
    type Row = (String, String, Option<String>, Option<String>, String);
    let devices: Vec<Row> = stmt
        .query_map(params![team], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let label = |id: &str, user: Option<&str>| match user.filter(|_| people) {
        Some(u) => u.to_string(),
        None => format!("Machine {}", short(id)),
    };
    let mut facts_stmt =
        conn.prepare("SELECT fact, first_seen FROM machine_facts WHERE machine_id = ?1")?;
    let mut removed_stmt = conn.prepare(
        "SELECT max(ts) FROM fact_changes WHERE machine_id = ?1 AND change = 'removed' AND fact LIKE ?2",
    )?;
    let mut drift = Vec::new();
    let mut by_rule: BTreeMap<String, usize> = BTreeMap::new();
    let mut machines = 0;
    for (id, team_name, user, last_sync, joined) in &devices {
        let facts: BTreeMap<String, String> = facts_stmt
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        // A machine that never sent its setup can't be compared.
        if facts.is_empty() {
            continue;
        }
        machines += 1;
        let mut diffs = Vec::new();
        let flag_fact = |flag: &str| {
            facts
                .iter()
                .find(|(f, _)| f.starts_with("flag:") && f.ends_with(&format!(":{flag}")))
        };
        for flag in &baseline.require {
            if flag_fact(flag).is_none() {
                let since: Option<String> =
                    removed_stmt.query_row(params![id, format!("flag:%:{flag}")], |r| r.get(0))?;
                diffs.push(Difference {
                    rule: format!("require:{flag}"),
                    title: format!("Missing: {}", flag_title(flag).to_lowercase()),
                    since: since.or_else(|| Some(joined.clone())),
                });
            }
        }
        for flag in &baseline.forbid {
            if let Some((_, first)) = flag_fact(flag) {
                diffs.push(Difference {
                    rule: format!("forbid:{flag}"),
                    title: flag_title(flag),
                    since: Some(first.clone()),
                });
            }
        }
        for (fact, first) in &facts {
            let p: Vec<&str> = fact.splitn(5, ':').collect();
            match p.as_slice() {
                ["tag", _, _, name, tag] if baseline.forbid_tags.iter().any(|t| t == tag) => {
                    diffs.push(Difference {
                        rule: format!("tag:{tag}"),
                        title: format!("{name}: {}", tag_title(tag)),
                        since: Some(first.clone()),
                    });
                }
                ["item", "mcp_server", _, name] => {
                    if let Some(allow) = &baseline.mcp_allowlist
                        && !allow.iter().any(|a| a.eq_ignore_ascii_case(name))
                    {
                        diffs.push(Difference {
                            rule: format!("mcp:{name}"),
                            title: format!("MCP server not on the list: {name}"),
                            since: Some(first.clone()),
                        });
                    }
                }
                _ => {}
            }
        }
        for (agent, min) in &baseline.min_versions {
            let versions: Vec<(&str, &String)> = facts
                .iter()
                .filter_map(|(f, first)| {
                    f.strip_prefix(&format!("version:{agent}:"))
                        .map(|v| (v, first))
                })
                .collect();
            // Every version in use is older than the minimum.
            if !versions.is_empty() && versions.iter().all(|(v, _)| version_lt(v, min)) {
                let newest = versions
                    .iter()
                    .max_by(|a, b| {
                        if version_lt(a.0, b.0) {
                            std::cmp::Ordering::Less
                        } else {
                            std::cmp::Ordering::Greater
                        }
                    })
                    .expect("not empty");
                diffs.push(Difference {
                    rule: format!("version:{agent}"),
                    title: format!("{agent} {} (oldest to run: {min})", newest.0),
                    since: Some(newest.1.clone()),
                });
            }
        }
        for d in &diffs {
            let key = d.rule.split(':').next().unwrap_or("").to_string();
            let name = match key.as_str() {
                "tag" => format!(
                    "MCP server or skill: {}",
                    tag_title(d.rule.trim_start_matches("tag:"))
                ),
                "mcp" => "MCP server not on the list".to_string(),
                "version" => format!(
                    "{} below the oldest version",
                    d.rule.trim_start_matches("version:")
                ),
                _ => d.title.clone(),
            };
            *by_rule.entry(name).or_default() += 1;
        }
        if !diffs.is_empty() {
            drift.push(MachineDrift {
                machine: label(id, user.as_deref()),
                team: team_name.clone(),
                last_sync: last_sync.clone(),
                differences: diffs,
            });
        }
    }
    // Most differences first, then by name: a list to work through, not a ranking of people.
    drift.sort_by(|a, b| {
        b.differences
            .len()
            .cmp(&a.differences.len())
            .then(a.machine.cmp(&b.machine))
    });
    let mut by_rule: Vec<(String, usize)> = by_rule.into_iter().collect();
    by_rule.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let since = agentcam_core::util::format_ts_ms(now_ms - 30 * 86_400_000);
    let mut stmt = conn.prepare(
        "SELECT c.ts, c.machine_id, d.os_user, c.fact, c.change FROM fact_changes c
         JOIN devices d ON d.machine_id = c.machine_id
         WHERE c.ts >= ?1 AND (?2 IS NULL OR d.team_id = ?2)
           AND c.fact NOT LIKE 'version:%'
         ORDER BY c.ts DESC LIMIT 60",
    )?;
    let changes = stmt
        .query_map(params![since, team], |r| {
            let id: String = r.get(1)?;
            let user: Option<String> = r.get(2)?;
            let fact: String = r.get(3)?;
            Ok(Change {
                ts: r.get(0)?,
                machine: label(&id, user.as_deref()),
                what: describe_fact(&fact),
                change: r.get(4)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let matching = machines - drift.len();
    Ok(BaselineView {
        set: !baseline.is_empty(),
        baseline,
        suggested: Baseline::suggested(),
        flags: FLAGS
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
        tags: TAGS
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
        machines,
        matching,
        by_rule,
        drift,
        changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(version_lt("2.4.0", "2.5.0"));
        assert!(version_lt("2.9.9", "2.10.0"));
        assert!(!version_lt("2.10.0", "2.9.9"));
        assert!(!version_lt("0.50.0", "0.50"));
    }

    #[test]
    fn facts_in_words() {
        assert_eq!(
            describe_fact("flag:any:mcp_secret_in_config"),
            "Secret written in an MCP config"
        );
        assert_eq!(
            describe_fact("tag:mcp_server:claude-code:github:unpinned"),
            "github: runs the latest release"
        );
        assert_eq!(
            describe_fact("item:mcp_server:codex:docs"),
            "mcp server docs"
        );
    }
}
