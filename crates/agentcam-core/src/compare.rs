//! Compare two groups of turns: before and after a model, agent version,
//! effort, mode, skill or date. Every turn is a row; a group is the turns
//! whose value matches. Differences are reported both ways, never judged.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::index::Result;
use crate::query::{Filter, conditions};
use crate::util::{format_ts_ms, parse_ts_ms};

/// What a turn did, with the values it can be grouped by.
#[derive(Debug, Clone, Default)]
struct TurnFacts {
    session_id: String,
    seq: i64,
    title: Option<String>,
    started_at: String,
    duration_ms: Option<i64>,
    agent: String,
    agent_version: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    mode: Option<String>,
    project: Option<String>,
    skills: Vec<String>,
    steps: i64,
    tool_calls: i64,
    errors: i64,
    cost: Option<f64>,
    thinking_ms: i64,
    wasted: i64,
    stopped: i64,
    added: i64,
    kept: i64,
}

fn load(conn: &Connection, f: &Filter, now_ms: i64) -> Result<Vec<TurnFacts>> {
    let (cond, cond_args) = conditions(f);
    // Survival of edits from the last day isn't settled yet. It's `?1`;
    // the filter's own `?` placeholders follow it in order.
    let mut args = vec![format_ts_ms(now_ms - 86_400_000)];
    args.extend(cond_args);
    let range = "x.session_id = t.session_id AND x.seq BETWEEN t.seq AND t.end_seq";
    let sql = format!(
        "SELECT t.session_id, t.started_at,
            coalesce(t.ended_at, (SELECT max(x.ts) FROM events x WHERE {range})),
            s.agent, s.agent_version, coalesce(t.model, s.model), t.effort, t.mode, s.project_root,
            (SELECT count(*) FROM events x WHERE {range}),
            (SELECT count(*) FROM events x WHERE {range} AND x.kind = 'tool_call'),
            (SELECT coalesce(sum(x.is_error), 0) FROM events x WHERE {range}),
            (SELECT sum(x.cost_usd) FROM events x WHERE {range}),
            (SELECT coalesce(sum(x.duration_ms), 0) FROM events x WHERE {range} AND x.kind = 'thinking'),
            (SELECT coalesce(sum(x.wasted), 0) FROM events x WHERE {range}),
            (SELECT count(*) FROM events x WHERE {range}
                AND (x.approval = 'rejected' OR x.approval LIKE 'blocked%')),
            (SELECT coalesce(sum(v.added), 0) FROM survival v
                WHERE v.session_id = t.session_id AND v.seq BETWEEN t.seq AND t.end_seq AND v.ts < ?1),
            (SELECT coalesce(sum(v.kept), 0) FROM survival v
                WHERE v.session_id = t.session_id AND v.seq BETWEEN t.seq AND t.end_seq AND v.ts < ?1),
            (SELECT group_concat(DISTINCT u.skill) FROM skill_uses u
                WHERE u.session_id = t.session_id AND u.seq BETWEEN t.seq AND t.end_seq),
            t.seq, s.title
         FROM turns t JOIN events e ON e.session_id = t.session_id AND e.seq = t.seq
         JOIN sessions s ON s.id = t.session_id
         WHERE s.parent_id IS NULL AND t.started_at IS NOT NULL AND {cond}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&args), |r| {
        let started_at: String = r.get(1)?;
        let ended: Option<String> = r.get(2)?;
        let skills: Option<String> = r.get(18)?;
        Ok(TurnFacts {
            session_id: r.get(0)?,
            seq: r.get(19)?,
            title: r.get(20)?,
            duration_ms: parse_ts_ms(&started_at)
                .zip(ended.as_deref().and_then(parse_ts_ms))
                .map(|(a, b)| (b - a).max(0)),
            started_at,
            agent: r.get(3)?,
            agent_version: r.get(4)?,
            model: r.get(5)?,
            effort: r.get(6)?,
            mode: r.get(7)?,
            project: r.get(8)?,
            steps: r.get(9)?,
            tool_calls: r.get(10)?,
            errors: r.get(11)?,
            cost: r.get(12)?,
            thinking_ms: r.get(13)?,
            wasted: r.get(14)?,
            stopped: r.get(15)?,
            added: r.get(16)?,
            kept: r.get(17)?,
            skills: skills
                .map(|s| s.split(',').map(String::from).collect())
                .unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// What to group turns by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Dim {
    Model,
    Agent,
    AgentVersion,
    Effort,
    Mode,
    Skill,
    /// `before` / `after` a time given as `at`.
    Date,
}

/// The values a turn has for `dim` (a turn can use several skills).
fn values(t: &TurnFacts, dim: Dim, at: Option<&str>) -> Vec<String> {
    let one = |v: &Option<String>| vec![v.clone().unwrap_or_else(|| "not recorded".into())];
    match dim {
        Dim::Model => one(&t.model),
        Dim::Agent => vec![t.agent.clone()],
        Dim::AgentVersion => vec![format!(
            "{} {}",
            t.agent,
            t.agent_version.as_deref().unwrap_or("?")
        )],
        Dim::Effort => one(&t.effort),
        Dim::Mode => one(&t.mode),
        Dim::Skill if t.skills.is_empty() => vec!["no skill".into()],
        Dim::Skill => t.skills.clone(),
        Dim::Date => match at {
            Some(at) if t.started_at.as_str() < at => vec!["before".into()],
            Some(_) => vec!["after".into()],
            None => vec![],
        },
    }
}

/// One group's numbers. Rates are per turn or per 100 steps so groups of
/// different sizes compare fairly.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Cohort {
    pub value: String,
    pub turns: i64,
    pub sessions: i64,
    pub first: Option<String>,
    pub last: Option<String>,
    pub median_turn_ms: i64,
    pub median_steps: f64,
    pub tool_calls_per_turn: f64,
    pub avg_cost_usd: Option<f64>,
    pub errored_per_100_steps: f64,
    /// Share of steps spent in retry loops and error streaks, 0–100.
    pub retry_pct: f64,
    pub stopped_per_100_calls: f64,
    pub thinking_ms_per_turn: f64,
    /// Lines added in settled edits, and the share still in the code.
    pub lines_added: i64,
    pub lasted_pct: Option<f64>,
    /// A few turns near the group's typical length, to open in the replay.
    pub examples: Vec<TurnExample>,
}

/// A turn to open in the replay.
#[derive(Debug, Clone, Serialize)]
pub struct TurnExample {
    pub session_id: String,
    /// The turn's first step.
    pub seq: i64,
    pub started_at: String,
    pub title: Option<String>,
    pub duration_ms: Option<i64>,
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let m = v.len() / 2;
    if v.len() % 2 == 0 {
        (v[m - 1] + v[m]) / 2.0
    } else {
        v[m]
    }
}

/// Up to three turns whose length is nearest the group's median.
fn examples(turns: &[&TurnFacts]) -> Vec<TurnExample> {
    let mut timed: Vec<&&TurnFacts> = turns.iter().filter(|t| t.duration_ms.is_some()).collect();
    let mid = median(
        timed
            .iter()
            .map(|t| t.duration_ms.unwrap_or(0) as f64)
            .collect(),
    );
    timed.sort_by(|a, b| {
        let d = |t: &TurnFacts| (t.duration_ms.unwrap_or(0) as f64 - mid).abs();
        d(a).total_cmp(&d(b))
            .then_with(|| (&a.session_id, a.seq).cmp(&(&b.session_id, b.seq)))
    });
    timed
        .into_iter()
        .take(3)
        .map(|t| TurnExample {
            session_id: t.session_id.clone(),
            seq: t.seq,
            started_at: t.started_at.clone(),
            title: t.title.clone(),
            duration_ms: t.duration_ms,
        })
        .collect()
}

fn cohort(value: &str, turns: &[&TurnFacts]) -> Cohort {
    let n = turns.len() as f64;
    let steps: i64 = turns.iter().map(|t| t.steps).sum();
    let calls: i64 = turns.iter().map(|t| t.tool_calls).sum();
    let costs: Vec<f64> = turns.iter().filter_map(|t| t.cost).collect();
    let added: i64 = turns.iter().map(|t| t.added).sum();
    let kept: i64 = turns.iter().map(|t| t.kept).sum();
    let per = |a: i64, b: i64| {
        if b > 0 {
            100.0 * a as f64 / b as f64
        } else {
            0.0
        }
    };
    let mut sessions: Vec<&str> = turns.iter().map(|t| t.session_id.as_str()).collect();
    sessions.sort_unstable();
    sessions.dedup();
    Cohort {
        value: value.to_string(),
        turns: turns.len() as i64,
        sessions: sessions.len() as i64,
        first: turns.iter().map(|t| t.started_at.clone()).min(),
        last: turns.iter().map(|t| t.started_at.clone()).max(),
        median_turn_ms: median(
            turns
                .iter()
                .filter_map(|t| t.duration_ms)
                .map(|d| d as f64)
                .collect(),
        ) as i64,
        median_steps: median(turns.iter().map(|t| t.steps as f64).collect()),
        tool_calls_per_turn: if n > 0.0 { calls as f64 / n } else { 0.0 },
        avg_cost_usd: (!costs.is_empty()).then(|| costs.iter().sum::<f64>() / costs.len() as f64),
        errored_per_100_steps: per(turns.iter().map(|t| t.errors).sum(), steps),
        retry_pct: per(turns.iter().map(|t| t.wasted).sum(), steps),
        stopped_per_100_calls: per(turns.iter().map(|t| t.stopped).sum(), calls),
        thinking_ms_per_turn: if n > 0.0 {
            turns.iter().map(|t| t.thinking_ms as f64).sum::<f64>() / n
        } else {
            0.0
        },
        lines_added: added,
        lasted_pct: (added > 0).then(|| 100.0 * kept as f64 / added as f64),
        examples: examples(turns),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    pub dim: Dim,
    pub a: Cohort,
    pub b: Cohort,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub dim: Dim,
    pub a: String,
    pub b: String,
    /// For `Dim::Date`: the time that splits before from after.
    pub at: Option<String>,
}

pub fn compare(conn: &Connection, f: &Filter, req: &Request, now_ms: i64) -> Result<Comparison> {
    let all = load(conn, f, now_ms)?;
    let pick = |v: &str| -> Vec<&TurnFacts> {
        all.iter()
            .filter(|t| values(t, req.dim, req.at.as_deref()).iter().any(|x| x == v))
            .collect()
    };
    Ok(Comparison {
        dim: req.dim,
        a: cohort(&req.a, &pick(&req.a)),
        b: cohort(&req.b, &pick(&req.b)),
    })
}

/// The values each dimension takes, most turns first.
#[derive(Debug, Clone, Serialize)]
pub struct DimValues {
    pub dim: Dim,
    pub values: Vec<(String, i64)>,
}

/// A comparison worth looking at, found in the data.
#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub title: String,
    pub dim: Dim,
    pub a: String,
    pub b: String,
    pub at: Option<String>,
    /// Project root the comparison is limited to.
    pub project: Option<String>,
    pub turns_a: i64,
    pub turns_b: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Options {
    pub dims: Vec<DimValues>,
    pub suggestions: Vec<Suggestion>,
}

const MIN: i64 = 8;

pub fn options(conn: &Connection, f: &Filter, now_ms: i64) -> Result<Options> {
    let all = load(conn, f, now_ms)?;
    let mut dims = Vec::new();
    let mut first_seen: HashMap<(Dim, String), String> = HashMap::new();
    for dim in [
        Dim::Model,
        Dim::Agent,
        Dim::AgentVersion,
        Dim::Effort,
        Dim::Mode,
        Dim::Skill,
    ] {
        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        for t in &all {
            for v in values(t, dim, None) {
                *counts.entry(v.clone()).or_default() += 1;
                let e = first_seen
                    .entry((dim, v))
                    .or_insert_with(|| t.started_at.clone());
                if t.started_at < *e {
                    *e = t.started_at.clone();
                }
            }
        }
        let mut v: Vec<(String, i64)> = counts.into_iter().collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        v.truncate(30);
        dims.push(DimValues { dim, values: v });
    }

    let mut suggestions = Vec::new();
    // Two most used values, older first, for these dimensions.
    for (dim, what) in [
        (Dim::Model, "Model"),
        (Dim::Effort, "Effort"),
        (Dim::Mode, "Mode"),
    ] {
        let vals = &dims.iter().find(|d| d.dim == dim).unwrap().values;
        let usable: Vec<&(String, i64)> = vals
            .iter()
            .filter(|(v, n)| *n >= MIN && v != "not recorded")
            .take(2)
            .collect();
        if let [x, y] = usable[..] {
            let (a, b) =
                if first_seen.get(&(dim, x.0.clone())) <= first_seen.get(&(dim, y.0.clone())) {
                    (x, y)
                } else {
                    (y, x)
                };
            suggestions.push(Suggestion {
                title: format!("{what}: {} vs {}", a.0, b.0),
                dim,
                a: a.0.clone(),
                b: b.0.clone(),
                at: None,
                project: None,
                turns_a: a.1,
                turns_b: b.1,
            });
        }
    }
    // Each agent's newest version with enough turns against the one before.
    let versions = &dims
        .iter()
        .find(|d| d.dim == Dim::AgentVersion)
        .unwrap()
        .values;
    for agent in ["claude-code", "codex"] {
        let mut vs: Vec<&(String, i64)> = versions
            .iter()
            .filter(|(v, n)| v.starts_with(agent) && *n >= MIN)
            .collect();
        vs.sort_by_key(|(v, _)| first_seen.get(&(Dim::AgentVersion, v.clone())).cloned());
        if let [.., a, b] = vs[..] {
            suggestions.push(Suggestion {
                title: format!(
                    "{} {} vs {}",
                    agent_label(agent),
                    short_version(&a.0),
                    short_version(&b.0)
                ),
                dim: Dim::AgentVersion,
                a: a.0.clone(),
                b: b.0.clone(),
                at: None,
                project: None,
                turns_a: a.1,
                turns_b: b.1,
            });
        }
    }
    // Before and after the latest edit to a project's AGENTS.md or CLAUDE.md.
    let (cond, args) = conditions(f);
    let mut stmt = conn.prepare(&format!(
        "SELECT s.project_root, max(e.ts), max(t.path) FROM file_touches t
         JOIN events e ON e.session_id = t.session_id AND e.seq = t.seq
         JOIN sessions s ON s.id = t.session_id
         WHERE t.action != 'read' AND (t.path LIKE '%/AGENTS.md' OR t.path LIKE '%/CLAUDE.md')
           AND s.project_root IS NOT NULL AND {cond}
         GROUP BY s.project_root"
    ))?;
    let changes: Vec<(String, String, String)> = stmt
        .query_map(params_from_iter(&args), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    for (project, at, path) in changes {
        let before = all
            .iter()
            .filter(|t| t.project.as_deref() == Some(project.as_str()) && t.started_at < at)
            .count() as i64;
        let after = all
            .iter()
            .filter(|t| t.project.as_deref() == Some(project.as_str()) && t.started_at >= at)
            .count() as i64;
        if before >= MIN && after >= MIN {
            let file = path.rsplit('/').next().unwrap_or("AGENTS.md");
            suggestions.push(Suggestion {
                title: format!(
                    "{}: before and after {file} changed on {}",
                    crate::query_basename(&project),
                    at.get(..10).unwrap_or(&at)
                ),
                dim: Dim::Date,
                a: "before".into(),
                b: "after".into(),
                at: Some(at),
                project: Some(project),
                turns_a: before,
                turns_b: after,
            });
        }
    }
    // The most used skill against turns without one.
    let skills = &dims.iter().find(|d| d.dim == Dim::Skill).unwrap().values;
    let none = skills
        .iter()
        .find(|(v, _)| v == "no skill")
        .map_or(0, |x| x.1);
    if let Some((top, n)) = skills.iter().find(|(v, n)| v != "no skill" && *n >= MIN) {
        suggestions.push(Suggestion {
            title: format!("Turns with the {top} skill vs without a skill"),
            dim: Dim::Skill,
            a: "no skill".into(),
            b: top.clone(),
            at: None,
            project: None,
            turns_a: none,
            turns_b: *n,
        });
    }
    Ok(Options { dims, suggestions })
}

fn agent_label(agent: &str) -> &str {
    match agent {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        a => a,
    }
}

fn short_version(v: &str) -> &str {
    v.rsplit(' ').next().unwrap_or(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(model: &str, ms: i64, steps: i64, errors: i64) -> TurnFacts {
        TurnFacts {
            session_id: model.into(),
            started_at: "2026-11-01T00:00:00.000Z".into(),
            duration_ms: Some(ms),
            model: Some(model.into()),
            steps,
            tool_calls: steps,
            errors,
            ..Default::default()
        }
    }

    #[test]
    fn cohorts_use_medians_and_rates() {
        let ts = [
            turn("a", 10, 4, 1),
            turn("a", 30, 6, 0),
            turn("a", 1000, 10, 1),
        ];
        let refs: Vec<&TurnFacts> = ts.iter().collect();
        let c = cohort("a", &refs);
        assert_eq!(c.turns, 3);
        assert_eq!(
            c.median_turn_ms, 30,
            "one long turn doesn't drag the median"
        );
        assert_eq!(c.median_steps, 6.0);
        assert!((c.errored_per_100_steps - 10.0).abs() < 1e-9);
        assert_eq!(c.lasted_pct, None);
        // Examples: the turn at the median first, the long one last.
        let lens: Vec<_> = c.examples.iter().map(|e| e.duration_ms).collect();
        assert_eq!(lens, vec![Some(30), Some(10), Some(1000)]);
    }

    #[test]
    fn date_split_and_skills() {
        let mut t = turn("a", 1, 1, 0);
        assert_eq!(values(&t, Dim::Date, Some("2026-12-01")), vec!["before"]);
        assert_eq!(values(&t, Dim::Date, Some("2026-10-01")), vec!["after"]);
        assert_eq!(values(&t, Dim::Skill, None), vec!["no skill"]);
        t.skills = vec!["dataviz".into(), "pdf".into()];
        assert_eq!(values(&t, Dim::Skill, None), vec!["dataviz", "pdf"]);
    }
}
