//! What the team UI shows, computed from the stored metadata.
//!
//! Numbers describe; they don't rank people. Machines are labelled by a
//! short id unless an admin turns on `show_people`, and no view sorts
//! people by output.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::params;
use serde::Serialize;

use crate::Result;
use crate::store::{Store, short};

#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// Days back from now; `None` for all time.
    pub days: Option<i64>,
    pub team: Option<i64>,
}

impl Scope {
    fn since(&self, now_ms: i64) -> Option<String> {
        self.days
            .map(|d| agentcam_core::util::format_ts_ms(now_ms - d * 86_400_000))
    }
}

#[derive(Debug, Clone)]
struct Row {
    machine_id: String,
    team: String,
    parent: bool,
    agent: String,
    agent_version: Option<String>,
    model: Option<String>,
    project: Option<String>,
    ended_at: Option<String>,
    active_ms: i64,
    retry_ms: i64,
    steps: i64,
    errored: i64,
    cost: Option<f64>,
    lasted: Option<[i64; 4]>,
    follow: Option<[i64; 2]>,
    commits: i64,
    prs: i64,
    findings: i64,
}

fn rows(
    store: &Store,
    from: Option<&str>,
    to: Option<&str>,
    team: Option<i64>,
) -> Result<Vec<Row>> {
    let mut stmt = store.conn().prepare(
        "SELECT s.machine_id, t.name, s.parent_id, s.agent, s.agent_version, s.model, s.project,
                s.ended_at, s.active_ms, s.retry_ms, s.steps, s.errored_steps, s.est_cost_usd,
                s.lasted_added, s.lasted_kept, s.lasted_reworked, s.lasted_changed,
                s.follow_ok, s.follow_missed, s.commits, s.prs,
                (SELECT count(*) FROM findings f WHERE f.machine_id = s.machine_id AND f.session_id = s.session_id)
         FROM sessions s JOIN teams t ON t.id = s.team_id
         WHERE (?1 IS NULL OR s.ended_at >= ?1) AND (?2 IS NULL OR s.ended_at < ?2)
           AND (?3 IS NULL OR s.team_id = ?3)",
    )?;
    let out = stmt
        .query_map(params![from, to, team], |r| {
            let lasted: Option<i64> = r.get(13)?;
            let follow: Option<i64> = r.get(17)?;
            Ok(Row {
                machine_id: r.get(0)?,
                team: r.get(1)?,
                parent: r.get::<_, Option<String>>(2)?.is_none(),
                agent: r.get(3)?,
                agent_version: r.get(4)?,
                model: r.get(5)?,
                project: r.get(6)?,
                ended_at: r.get(7)?,
                active_ms: r.get(8)?,
                retry_ms: r.get(9)?,
                steps: r.get(10)?,
                errored: r.get(11)?,
                cost: r.get(12)?,
                lasted: match lasted {
                    Some(a) => Some([a, r.get(14)?, r.get(15)?, r.get(16)?]),
                    None => None,
                },
                follow: match follow {
                    Some(f) => Some([f, r.get(18)?]),
                    None => None,
                },
                commits: r.get(19)?,
                prs: r.get(20)?,
                findings: r.get(21)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(out)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Lasted {
    pub added: i64,
    pub kept: i64,
    pub reworked: i64,
    pub changed: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Sum {
    /// Top-level sessions; subagents count toward their parent.
    pub sessions: i64,
    pub machines: i64,
    pub active_ms: i64,
    pub retry_ms: i64,
    pub steps: i64,
    pub errored_steps: i64,
    pub cost_usd: f64,
    /// Sessions whose cost couldn't be priced.
    pub unpriced: i64,
    pub commits: i64,
    pub prs: i64,
    pub lasted: Lasted,
    pub followed: i64,
    pub not_followed: i64,
    pub findings: i64,
    #[serde(skip)]
    seen: BTreeSet<String>,
}

impl Sum {
    fn add(&mut self, r: &Row) {
        if r.parent {
            self.sessions += 1;
        }
        if self.seen.insert(r.machine_id.clone()) {
            self.machines += 1;
        }
        self.active_ms += r.active_ms;
        self.retry_ms += r.retry_ms;
        self.steps += r.steps;
        self.errored_steps += r.errored;
        match r.cost {
            Some(c) => self.cost_usd += c,
            None if r.parent => self.unpriced += 1,
            None => {}
        }
        self.commits += r.commits;
        self.prs += r.prs;
        if let Some(l) = r.lasted {
            self.lasted.added += l[0];
            self.lasted.kept += l[1];
            self.lasted.reworked += l[2];
            self.lasted.changed += l[3];
        }
        if let Some(f) = r.follow {
            self.followed += f[0];
            self.not_followed += f[1];
        }
        self.findings += r.findings;
    }
}

fn sum<'a>(rows: impl IntoIterator<Item = &'a Row>) -> Sum {
    let mut s = Sum::default();
    for r in rows {
        s.add(r);
    }
    s
}

/// Groups rows by `key`, biggest (most agent time) first.
fn group<K: Ord + Clone>(rows: &[Row], key: impl Fn(&Row) -> K) -> Vec<(K, Sum)> {
    let mut m: BTreeMap<K, Sum> = BTreeMap::new();
    for r in rows {
        m.entry(key(r)).or_default().add(r);
    }
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by_key(|(_, s)| std::cmp::Reverse(s.active_ms));
    v
}

/// The Monday of the week `ts` falls in, as `YYYY-MM-DD`.
fn week_of(ts: &str) -> Option<String> {
    let ms = agentcam_core::util::parse_ts_ms(ts)?;
    let days = ms.div_euclid(86_400_000);
    // 1970-01-01 was a Thursday.
    let monday = days - (days + 3).rem_euclid(7);
    Some(agentcam_core::util::format_ts_ms(monday * 86_400_000)[..10].to_string())
}

#[derive(Debug, Serialize)]
pub struct Named {
    pub name: String,
    #[serde(flatten)]
    pub sum: Sum,
}

#[derive(Debug, Serialize)]
pub struct AgentModel {
    pub agent: String,
    pub model: Option<String>,
    #[serde(flatten)]
    pub sum: Sum,
}

#[derive(Debug, Serialize)]
pub struct Week {
    pub week: String,
    #[serde(flatten)]
    pub sum: Sum,
}

#[derive(Debug, Serialize)]
pub struct Insights {
    pub days: Option<i64>,
    pub totals: Sum,
    /// The same span just before, for "vs before".
    pub previous: Option<Sum>,
    pub weeks: Vec<Week>,
    pub by_team: Vec<Named>,
    pub by_project: Vec<Named>,
    pub by_agent_model: Vec<AgentModel>,
}

pub fn insights(store: &Store, scope: &Scope, now_ms: i64) -> Result<Insights> {
    let since = scope.since(now_ms);
    let rs = rows(store, since.as_deref(), None, scope.team)?;
    let previous = match scope.days {
        Some(d) => {
            let from = agentcam_core::util::format_ts_ms(now_ms - 2 * d * 86_400_000);
            Some(sum(&rows(
                store,
                Some(&from),
                since.as_deref(),
                scope.team,
            )?))
        }
        None => None,
    };
    let mut weeks: Vec<Week> = group(&rs, |r| r.ended_at.as_deref().and_then(week_of))
        .into_iter()
        .filter_map(|(w, sum)| Some(Week { week: w?, sum }))
        .collect();
    weeks.sort_by(|a, b| a.week.cmp(&b.week));
    Ok(Insights {
        days: scope.days,
        totals: sum(&rs),
        previous,
        weeks,
        by_team: group(&rs, |r| r.team.clone())
            .into_iter()
            .map(|(name, sum)| Named { name, sum })
            .collect(),
        by_project: group(&rs, |r| {
            r.project.clone().unwrap_or_else(|| "(no project)".into())
        })
        .into_iter()
        .take(50)
        .map(|(name, sum)| Named { name, sum })
        .collect(),
        by_agent_model: group(&rs, |r| (r.agent.clone(), r.model.clone()))
            .into_iter()
            .map(|((agent, model), sum)| AgentModel { agent, model, sum })
            .collect(),
    })
}

#[derive(Debug, Serialize)]
pub struct Cohort {
    pub agent: String,
    pub label: String,
    pub sessions: i64,
    pub machines: i64,
    pub median_active_ms: Option<i64>,
    pub median_cost_usd: Option<f64>,
    /// Errored steps per 100 steps.
    pub errors_per_100: Option<f64>,
    /// Share of agent time spent in retries, 0–1.
    pub retry_share: Option<f64>,
    /// Share of added lines still in the code, 0–1.
    pub kept_share: Option<f64>,
    /// Share of checked rules followed, 0–1.
    pub followed_share: Option<f64>,
}

fn median_i(mut v: Vec<i64>) -> Option<i64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

fn median_f(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

fn ratio(a: i64, b: i64) -> Option<f64> {
    (b > 0).then(|| a as f64 / b as f64)
}

fn cohorts(rows: &[Row], key: impl Fn(&Row) -> Option<String>) -> Vec<Cohort> {
    let mut m: BTreeMap<(String, String), Vec<&Row>> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.parent) {
        if let Some(k) = key(r) {
            m.entry((r.agent.clone(), k)).or_default().push(r);
        }
    }
    let mut out: Vec<Cohort> = m
        .into_iter()
        .map(|((agent, label), rs)| {
            let s = sum(rs.iter().copied());
            Cohort {
                agent,
                label,
                sessions: rs.len() as i64,
                machines: s.machines,
                median_active_ms: median_i(rs.iter().map(|r| r.active_ms).collect()),
                median_cost_usd: median_f(rs.iter().filter_map(|r| r.cost).collect()),
                errors_per_100: ratio(s.errored_steps * 100, s.steps),
                retry_share: ratio(s.retry_ms, s.active_ms),
                kept_share: ratio(s.lasted.kept, s.lasted.added),
                followed_share: ratio(s.followed, s.followed + s.not_followed),
            }
        })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.sessions));
    out.truncate(12);
    out
}

#[derive(Debug, Serialize)]
pub struct Seats {
    /// Monthly price per seat the admin entered; `None` if not set.
    pub price_usd: Option<f64>,
    /// Machines with a session in range (a stand-in for people).
    pub seats: i64,
    /// Seat price spread over the range.
    pub seat_cost_usd: Option<f64>,
    /// What the same use would cost at API list prices.
    pub api_value_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct Adoption {
    pub team: String,
    /// Machines active per week, oldest first, one per `weeks` entry.
    pub machines: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct Value {
    pub days: Option<i64>,
    pub totals: Sum,
    pub cost_per_commit: Option<f64>,
    pub cost_per_pr: Option<f64>,
    pub kept_lines_per_dollar: Option<f64>,
    pub seats: Seats,
    pub weeks: Vec<String>,
    pub adoption: Vec<Adoption>,
    pub by_model: Vec<Cohort>,
    pub by_version: Vec<Cohort>,
}

pub fn value(store: &Store, scope: &Scope, now_ms: i64) -> Result<Value> {
    let since = scope.since(now_ms);
    let rs = rows(store, since.as_deref(), None, scope.team)?;
    let t = sum(&rs);
    let per = |n: i64| (n > 0 && t.cost_usd > 0.0).then(|| t.cost_usd / n as f64);
    let price = store
        .setting("seat_price_usd")
        .and_then(|p| p.parse::<f64>().ok())
        .filter(|p| *p > 0.0);
    let months = scope.days.map_or_else(
        || {
            // All time: from the oldest session to now.
            let first = rs
                .iter()
                .filter_map(|r| r.ended_at.as_deref())
                .min()
                .and_then(agentcam_core::util::parse_ts_ms)
                .unwrap_or(now_ms);
            ((now_ms - first) as f64 / (30.0 * 86_400_000.0)).max(1.0)
        },
        |d| d as f64 / 30.0,
    );
    let mut weeks: BTreeSet<String> = BTreeSet::new();
    let mut active: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
    for r in &rs {
        if let Some(w) = r.ended_at.as_deref().and_then(week_of) {
            weeks.insert(w.clone());
            active
                .entry((r.team.clone(), w))
                .or_default()
                .insert(r.machine_id.clone());
        }
    }
    let weeks: Vec<String> = weeks.into_iter().collect();
    let teams: BTreeSet<String> = rs.iter().map(|r| r.team.clone()).collect();
    let adoption = teams
        .into_iter()
        .map(|team| Adoption {
            machines: weeks
                .iter()
                .map(|w| {
                    active
                        .get(&(team.clone(), w.clone()))
                        .map_or(0, |s| s.len() as i64)
                })
                .collect(),
            team,
        })
        .collect();
    Ok(Value {
        days: scope.days,
        cost_per_commit: per(t.commits),
        cost_per_pr: per(t.prs),
        kept_lines_per_dollar: (t.cost_usd > 0.0 && t.lasted.added > 0)
            .then(|| t.lasted.kept as f64 / t.cost_usd),
        seats: Seats {
            price_usd: price,
            seats: t.machines,
            seat_cost_usd: price.map(|p| p * t.machines as f64 * months),
            api_value_usd: t.cost_usd,
        },
        weeks,
        adoption,
        by_model: cohorts(&rs, |r| r.model.clone()),
        by_version: cohorts(&rs, |r| r.agent_version.clone()),
        totals: t,
    })
}

#[derive(Debug, Serialize)]
pub struct RuleCount {
    pub rule_id: String,
    pub severity: String,
    pub count: i64,
    pub sessions: i64,
    pub machines: i64,
    pub projects: i64,
}

#[derive(Debug, Serialize)]
pub struct FindingRow {
    pub ts: Option<String>,
    pub rule_id: String,
    pub severity: String,
    pub project: Option<String>,
    pub team: String,
    pub machine: String,
    pub step: i64,
    pub excerpt: String,
}

#[derive(Debug, Serialize)]
pub struct Governance {
    pub days: Option<i64>,
    pub by_severity: BTreeMap<String, i64>,
    pub rules: Vec<RuleCount>,
    pub latest: Vec<FindingRow>,
    pub show_people: bool,
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "critical" => 4,
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}

pub fn show_people(store: &Store) -> bool {
    store.setting("show_people").as_deref() == Some("true")
}

fn machine_label(people: bool, machine_id: &str, os_user: Option<&str>) -> String {
    match os_user.filter(|_| people) {
        Some(u) => u.to_string(),
        None => format!("Machine {}", short(machine_id)),
    }
}

pub fn governance(store: &Store, scope: &Scope, now_ms: i64) -> Result<Governance> {
    let since = scope.since(now_ms);
    let people = show_people(store);
    let mut stmt = store.conn().prepare(
        "SELECT f.rule_id, f.severity, f.step, f.excerpt, s.ended_at, s.project, t.name,
                s.machine_id, d.os_user, s.session_id
         FROM findings f
         JOIN sessions s ON s.machine_id = f.machine_id AND s.session_id = f.session_id
         JOIN teams t ON t.id = s.team_id
         LEFT JOIN devices d ON d.machine_id = s.machine_id
         WHERE (?1 IS NULL OR s.ended_at >= ?1) AND (?2 IS NULL OR s.team_id = ?2)
         ORDER BY s.ended_at DESC",
    )?;
    type F = (
        String,
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
        String,
    );
    let all: Vec<F> = stmt
        .query_map(params![since, scope.team], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut by_severity = BTreeMap::new();
    struct Acc {
        severity: String,
        count: i64,
        sessions: BTreeSet<(String, String)>,
        machines: BTreeSet<String>,
        projects: BTreeSet<String>,
    }
    let mut rules: BTreeMap<String, Acc> = BTreeMap::new();
    for f in &all {
        *by_severity.entry(f.1.clone()).or_insert(0) += 1;
        let a = rules.entry(f.0.clone()).or_insert_with(|| Acc {
            severity: f.1.clone(),
            count: 0,
            sessions: BTreeSet::new(),
            machines: BTreeSet::new(),
            projects: BTreeSet::new(),
        });
        a.count += 1;
        a.sessions.insert((f.7.clone(), f.9.clone()));
        a.machines.insert(f.7.clone());
        if let Some(p) = &f.5 {
            a.projects.insert(p.clone());
        }
    }
    let mut rules: Vec<RuleCount> = rules
        .into_iter()
        .map(|(rule_id, a)| RuleCount {
            rule_id,
            severity: a.severity,
            count: a.count,
            sessions: a.sessions.len() as i64,
            machines: a.machines.len() as i64,
            projects: a.projects.len() as i64,
        })
        .collect();
    rules.sort_by(|a, b| {
        severity_rank(&b.severity)
            .cmp(&severity_rank(&a.severity))
            .then(b.count.cmp(&a.count))
    });
    let mut latest: Vec<&F> = all.iter().collect();
    latest.sort_by_key(|f| std::cmp::Reverse(severity_rank(&f.1) >= 3));
    let latest = latest
        .into_iter()
        .take(60)
        .map(|f| FindingRow {
            ts: f.4.clone(),
            rule_id: f.0.clone(),
            severity: f.1.clone(),
            project: f.5.clone(),
            team: f.6.clone(),
            machine: machine_label(people, &f.7, f.8.as_deref()),
            step: f.2,
            excerpt: f.3.clone(),
        })
        .collect();
    Ok(Governance {
        days: scope.days,
        by_severity,
        rules,
        latest,
        show_people: people,
    })
}

#[derive(Debug, Serialize)]
pub struct MachineRow {
    pub label: String,
    pub team: String,
    pub os: Option<String>,
    pub agentcam_version: Option<String>,
    pub joined_at: String,
    pub last_sync: Option<String>,
    pub agents: Vec<(String, Option<String>)>,
    pub flags: Vec<String>,
    pub sessions: i64,
}

#[derive(Debug, Serialize)]
pub struct VersionRow {
    pub agent: String,
    pub version: Option<String>,
    pub machines: i64,
    pub sessions: i64,
}

#[derive(Debug, Serialize)]
pub struct ItemRow {
    /// Tags any machine reported for it, with how many machines did.
    pub tags: Vec<(String, i64)>,
    pub kind: String,
    pub agent: String,
    pub name: String,
    pub detail: Option<String>,
    pub machines: i64,
}

#[derive(Debug, Serialize)]
pub struct FlagRow {
    pub id: String,
    pub agent: String,
    pub machines: i64,
}

#[derive(Debug, Serialize)]
pub struct Fleet {
    pub machines: Vec<MachineRow>,
    pub versions: Vec<VersionRow>,
    pub items: Vec<ItemRow>,
    pub flags: Vec<FlagRow>,
    pub show_people: bool,
}

pub fn fleet(store: &Store, scope: &Scope) -> Result<Fleet> {
    let c = store.conn();
    let people = show_people(store);
    let team = scope.team;
    let mut stmt = c.prepare(
        "SELECT d.machine_id, d.os_user, t.name, d.os, d.agentcam_version, d.joined_at, d.last_sync,
                (SELECT count(*) FROM sessions s WHERE s.machine_id = d.machine_id AND s.parent_id IS NULL)
         FROM devices d JOIN teams t ON t.id = d.team_id
         WHERE ?1 IS NULL OR d.team_id = ?1
         ORDER BY t.name, d.last_sync DESC",
    )?;
    let mut machines: Vec<(String, MachineRow)> = stmt
        .query_map(params![team], |r| {
            let id: String = r.get(0)?;
            let user: Option<String> = r.get(1)?;
            Ok((
                id.clone(),
                MachineRow {
                    label: machine_label(people, &id, user.as_deref()),
                    team: r.get(2)?,
                    os: r.get(3)?,
                    agentcam_version: r.get(4)?,
                    joined_at: r.get(5)?,
                    last_sync: r.get(6)?,
                    agents: Vec::new(),
                    flags: Vec::new(),
                    sessions: r.get(7)?,
                },
            ))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut av = c.prepare(
        "SELECT agent, version FROM agent_versions WHERE machine_id = ?1 ORDER BY agent, last_used DESC",
    )?;
    let mut fl = c.prepare(
        "SELECT DISTINCT agent || ':' || name FROM inventory WHERE machine_id = ?1 AND kind = 'flag' ORDER BY 1",
    )?;
    for (id, m) in &mut machines {
        let mut seen = BTreeSet::new();
        m.agents = av
            .query_map([&*id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<(String, Option<String>)>, _>>()?
            .into_iter()
            .filter(|(a, _)| seen.insert(a.clone()))
            .collect();
        m.flags = fl
            .query_map([&*id], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
    }
    let in_team =
        "(?1 IS NULL OR machine_id IN (SELECT machine_id FROM devices WHERE team_id = ?1))";
    let mut stmt = c.prepare(&format!(
        "SELECT agent, version, count(DISTINCT machine_id), sum(sessions) FROM agent_versions
         WHERE {in_team} GROUP BY agent, version ORDER BY agent, 3 DESC"
    ))?;
    let versions = stmt
        .query_map(params![team], |r| {
            Ok(VersionRow {
                agent: r.get(0)?,
                version: r.get(1)?,
                machines: r.get(2)?,
                sessions: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = c.prepare(&format!(
        "SELECT kind, agent, name, detail, count(DISTINCT machine_id) FROM inventory
         WHERE kind <> 'flag' AND {in_team}
         GROUP BY kind, agent, name, detail ORDER BY kind, 5 DESC, name"
    ))?;
    let items = stmt
        .query_map(params![team], |r| {
            Ok(ItemRow {
                tags: Vec::new(),
                kind: r.get(0)?,
                agent: r.get(1)?,
                name: r.get(2)?,
                detail: r.get(3)?,
                machines: r.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<ItemRow>, _>>()?;
    // Tags are stored comma-joined per machine; count each tag per item here
    // (plain SQL, so the same queries run on Postgres).
    let mut items = items;
    {
        let mut stmt = c.prepare(&format!(
            "SELECT kind, agent, name, tags FROM inventory
             WHERE kind <> 'flag' AND tags IS NOT NULL AND {in_team}"
        ))?;
        let rows: Vec<(String, String, String, String)> = stmt
            .query_map(params![team], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<std::result::Result<_, _>>()?;
        for (kind, agent, name, tags) in rows {
            if let Some(it) = items
                .iter_mut()
                .find(|i| i.kind == kind && i.agent == agent && i.name == name)
            {
                for t in tags.split(',').filter(|t| !t.is_empty()) {
                    match it.tags.iter_mut().find(|(x, _)| x == t) {
                        Some((_, n)) => *n += 1,
                        None => it.tags.push((t.to_string(), 1)),
                    }
                }
            }
        }
    }
    let mut stmt = c.prepare(&format!(
        "SELECT name, agent, count(DISTINCT machine_id) FROM inventory
         WHERE kind = 'flag' AND {in_team} GROUP BY name, agent ORDER BY 3 DESC"
    ))?;
    let flags = stmt
        .query_map(params![team], |r| {
            Ok(FlagRow {
                id: r.get(0)?,
                agent: r.get(1)?,
                machines: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(Fleet {
        machines: machines.into_iter().map(|(_, m)| m).collect(),
        versions,
        items,
        flags,
        show_people: people,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_start_on_monday() {
        // 2026-11-06 is a Friday.
        assert_eq!(
            week_of("2026-11-06T10:00:00Z").as_deref(),
            Some("2026-11-02")
        );
        assert_eq!(
            week_of("2026-11-02T00:00:00Z").as_deref(),
            Some("2026-11-02")
        );
        assert_eq!(
            week_of("2026-11-01T23:59:00Z").as_deref(),
            Some("2026-10-26")
        );
    }

    #[test]
    fn medians_and_ratios() {
        assert_eq!(median_i(vec![5, 1, 3]), Some(3));
        assert_eq!(median_i(vec![]), None);
        assert_eq!(ratio(1, 0), None);
        assert_eq!(ratio(1, 4), Some(0.25));
    }
}
