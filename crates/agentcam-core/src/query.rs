//! Read-side queries the web UI needs. Times are UTC RFC 3339 strings;
//! "days" are local calendar days on the machine running agentcam.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, Row, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::index::Result;

/// Narrows what a query looks at. Every field is optional.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Filter {
    /// Include steps at or after this time.
    pub since: Option<String>,
    /// Include steps before this time.
    pub until: Option<String>,
    /// `claude-code` or `codex`.
    pub agent: Option<String>,
    /// A project root path.
    pub project: Option<String>,
    /// A local day, `YYYY-MM-DD`.
    pub day: Option<String>,
}

/// SQL conditions on `e` (events) and `s` (sessions) for a filter.
pub(crate) fn conditions(f: &Filter) -> (String, Vec<String>) {
    let mut sql = vec!["1 = 1".to_string()];
    let mut args = Vec::new();
    let mut add = |cond: &str, v: &Option<String>| {
        if let Some(v) = v.as_ref().filter(|v| !v.is_empty()) {
            sql.push(cond.to_string());
            args.push(v.clone());
        }
    };
    add("e.ts >= ?", &f.since);
    add("e.ts < ?", &f.until);
    add("s.agent = ?", &f.agent);
    add("s.project_root = ?", &f.project);
    add("date(e.ts, 'localtime') = ?", &f.day);
    (sql.join(" AND "), args)
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Stats {
    pub sessions: i64,
    pub active_ms: i64,
    pub events: i64,
    pub prompts: i64,
    pub tool_calls: i64,
    pub commands: i64,
    pub errors: i64,
    pub files_changed: i64,
    pub files_created: i64,
    pub projects: i64,
    pub findings: i64,
    /// Findings rated high or critical.
    pub serious_findings: i64,
    /// Estimated dollars from the price table, over priced calls.
    pub cost_usd: f64,
    /// Model calls with tokens but no price (cost unknown).
    pub unpriced_calls: i64,
    /// Agent time spent on stuck loops, failure streaks and reverts.
    pub wasted_ms: i64,
    pub commits: i64,
}

pub fn stats(conn: &Connection, f: &Filter) -> Result<Stats> {
    let (cond, args) = conditions(f);
    let sql = format!(
        "SELECT count(DISTINCT coalesce(s.parent_id, s.id)),
                coalesce(sum(e.gap_ms), 0),
                count(*),
                coalesce(sum(e.kind = 'prompt'), 0),
                coalesce(sum(e.kind = 'tool_call'), 0),
                coalesce(sum(e.category = 'command'), 0),
                coalesce(sum(e.is_error), 0),
                count(DISTINCT s.project_root),
                coalesce(sum(e.cost_usd), 0),
                coalesce(sum(e.cost_usd IS NULL AND (e.tok_in + e.tok_out + e.tok_cache_read) > 0), 0),
                coalesce(sum(CASE WHEN e.wasted THEN coalesce(e.duration_ms, e.gap_ms) END), 0)
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE {cond}"
    );
    let mut st = conn.query_row(&sql, params_from_iter(&args), |r| {
        Ok(Stats {
            sessions: r.get(0)?,
            active_ms: r.get(1)?,
            events: r.get(2)?,
            prompts: r.get(3)?,
            tool_calls: r.get(4)?,
            commands: r.get(5)?,
            errors: r.get(6)?,
            projects: r.get(7)?,
            cost_usd: r.get(8)?,
            unpriced_calls: r.get(9)?,
            wasted_ms: r.get(10)?,
            ..Default::default()
        })
    })?;
    let files = format!(
        "SELECT count(DISTINCT ft.path), count(DISTINCT CASE WHEN ft.action = 'create' THEN ft.path END)
         FROM file_touches ft
         JOIN events e ON e.session_id = ft.session_id AND e.seq = ft.seq
         JOIN sessions s ON s.id = e.session_id
         WHERE ft.action != 'read' AND {cond}"
    );
    let fsql = format!(
        "SELECT count(*), coalesce(sum(fd.severity >= 3), 0)
         FROM findings fd
         JOIN events e ON e.session_id = fd.session_id AND e.seq = fd.seq
         JOIN sessions s ON s.id = e.session_id
         WHERE {cond}"
    );
    (st.findings, st.serious_findings) = conn.query_row(&fsql, params_from_iter(&args), |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    st.commits = conn.query_row(
        &format!(
            "SELECT count(*) FROM commits c
             JOIN events e ON e.session_id = c.session_id AND e.seq = c.seq
             JOIN sessions s ON s.id = e.session_id WHERE {cond}"
        ),
        params_from_iter(&args),
        |r| r.get(0),
    )?;
    (st.files_changed, st.files_created) =
        conn.query_row(&files, params_from_iter(&args), |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    Ok(st)
}

/// Activity on one local day.
#[derive(Debug, Clone, Serialize)]
pub struct Day {
    pub day: String,
    pub sessions: i64,
    pub active_ms: i64,
    pub events: i64,
    pub tool_calls: i64,
    pub cost_usd: f64,
}

/// Activity per local day for days with any steps. Ignores `f.day`.
pub fn activity(conn: &Connection, f: &Filter) -> Result<Vec<Day>> {
    let f = Filter {
        day: None,
        ..f.clone()
    };
    let (cond, args) = conditions(&f);
    let sql = format!(
        "SELECT date(e.ts, 'localtime') AS d,
                count(DISTINCT coalesce(s.parent_id, s.id)),
                sum(e.gap_ms), count(*), sum(e.kind = 'tool_call'), coalesce(sum(e.cost_usd), 0)
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.ts IS NOT NULL AND {cond}
         GROUP BY d ORDER BY d"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&args), |r| {
        Ok(Day {
            day: r.get(0)?,
            sessions: r.get(1)?,
            active_ms: r.get(2)?,
            events: r.get(3)?,
            tool_calls: r.get(4)?,
            cost_usd: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    /// Root folder; empty when the agent didn't record one.
    pub path: String,
    pub sessions: i64,
    pub active_ms: i64,
    pub events: i64,
    pub last_active: Option<String>,
    pub cost_usd: f64,
    pub agents: Vec<String>,
    /// Active time per week, oldest first, ending with the week before `until`.
    pub weekly_ms: Vec<i64>,
}

/// Projects with activity under `f`, busiest first. `now` anchors the
/// weekly sparkline when `f.until` is unset.
pub fn projects(conn: &Connection, f: &Filter, weeks: usize, now: &str) -> Result<Vec<Project>> {
    let f = Filter {
        project: None,
        ..f.clone()
    };
    let (cond, args) = conditions(&f);
    let sql = format!(
        "SELECT coalesce(s.project_root, ''), count(DISTINCT coalesce(s.parent_id, s.id)),
                sum(e.gap_ms), count(*), max(e.ts), group_concat(DISTINCT s.agent), coalesce(sum(e.cost_usd), 0)
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE {cond}
         GROUP BY 1 ORDER BY 3 DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut out: Vec<Project> = stmt
        .query_map(params_from_iter(&args), |r| {
            let agents: Option<String> = r.get(5)?;
            Ok(Project {
                path: r.get(0)?,
                sessions: r.get(1)?,
                active_ms: r.get(2)?,
                events: r.get(3)?,
                last_active: r.get(4)?,
                cost_usd: r.get(6)?,
                agents: agents
                    .map(|a| a.split(',').map(String::from).collect())
                    .unwrap_or_default(),
                weekly_ms: vec![0; weeks],
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    // Weekly buckets, counted back from the end of the range.
    let end = f.until.clone().unwrap_or_else(|| now.to_string());
    let spark = Filter {
        since: None,
        until: Some(end.clone()),
        ..f.clone()
    };
    let (cond, mut args) = conditions(&spark);
    args.insert(0, end);
    let sql = format!(
        "SELECT coalesce(s.project_root, ''),
                CAST((julianday(?) - julianday(e.ts)) / 7 AS INTEGER) AS wk,
                sum(e.gap_ms)
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.ts IS NOT NULL AND {cond}
         GROUP BY 1, 2"
    );
    let index: HashMap<String, usize> = out
        .iter()
        .enumerate()
        .map(|(i, p)| (p.path.clone(), i))
        .collect();
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(&args))?;
    while let Some(r) = rows.next()? {
        let (path, wk, ms): (String, i64, i64) = (r.get(0)?, r.get(1)?, r.get(2)?);
        if let (Some(&i), Ok(wk)) = (index.get(&path), usize::try_from(wk))
            && wk < weeks
        {
            out[i].weekly_ms[weeks - 1 - wk] += ms;
        }
    }
    Ok(out)
}

/// One session as the lists show it.
#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    pub id: String,
    pub agent: String,
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub project_path: Option<String>,
    pub project_root: Option<String>,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub parent_id: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub active_ms: i64,
    pub event_count: i64,
    pub tool_call_count: i64,
    pub error_count: i64,
    pub file_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: Option<f64>,
    pub subagents: i64,
    pub finding_count: i64,
    /// Worst finding: `low`, `medium`, `high`, `critical`, or null.
    pub max_severity: Option<String>,
    /// Estimated from the price table; null when no call had a price.
    pub est_cost_usd: Option<f64>,
    /// Some calls had no price, so the estimate is low.
    pub cost_unpriced: bool,
    /// Largest context window seen in one call.
    pub peak_context: i64,
    pub wasted_ms: i64,
    pub pattern_count: i64,
    pub commit_count: i64,
    /// Where it was started from (`cli`, `claude-desktop`, `vscode`, ...).
    pub entrypoint: Option<String>,
}

pub(crate) const SESSION_COLS: &str =
    "s.id, s.agent, s.title, s.first_prompt, s.project_path, s.project_root,
    s.git_branch, s.model, s.parent_id, s.started_at, s.ended_at, s.active_ms, s.event_count,
    s.tool_call_count, s.error_count, s.file_count, s.input_tokens, s.output_tokens,
    s.cache_read_tokens, s.cost_usd,
    (SELECT count(*) FROM sessions c WHERE c.parent_id = s.id),
    s.finding_count, s.max_severity, s.est_cost_usd, s.cost_unpriced, s.peak_context, s.wasted_ms,
    s.pattern_count, s.commit_count, s.entrypoint";

pub(crate) fn session_row(r: &Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        agent: r.get(1)?,
        title: r.get(2)?,
        first_prompt: r.get(3)?,
        project_path: r.get(4)?,
        project_root: r.get(5)?,
        git_branch: r.get(6)?,
        model: r.get(7)?,
        parent_id: r.get(8)?,
        started_at: r.get(9)?,
        ended_at: r.get(10)?,
        active_ms: r.get(11)?,
        event_count: r.get(12)?,
        tool_call_count: r.get(13)?,
        error_count: r.get(14)?,
        file_count: r.get(15)?,
        input_tokens: r.get(16)?,
        output_tokens: r.get(17)?,
        cache_read_tokens: r.get(18)?,
        cost_usd: r.get(19)?,
        subagents: r.get(20)?,
        finding_count: r.get(21)?,
        max_severity: severity_name(r.get(22)?),
        est_cost_usd: r.get(23)?,
        cost_unpriced: r.get(24)?,
        peak_context: r.get(25)?,
        wasted_ms: r.get(26)?,
        pattern_count: r.get(27)?,
        commit_count: r.get(28)?,
        entrypoint: r.get(29)?,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionPage {
    pub total: i64,
    pub items: Vec<SessionRow>,
}

/// Top-level sessions with at least one step matching `f` (a subagent's
/// steps count for its parent), most recently active first.
pub fn sessions(conn: &Connection, f: &Filter, limit: u32, offset: u32) -> Result<SessionPage> {
    let (cond, args) = conditions(f);
    let matching = format!(
        "SELECT coalesce(s.parent_id, s.id) AS root, max(e.ts) AS last
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE {cond} GROUP BY root"
    );
    let total = conn.query_row(
        &format!("SELECT count(*) FROM ({matching}) m JOIN sessions s ON s.id = m.root"),
        params_from_iter(&args),
        |r| r.get(0),
    )?;
    let sql = format!(
        "SELECT {SESSION_COLS} FROM ({matching}) m JOIN sessions s ON s.id = m.root
         ORDER BY m.last DESC LIMIT {limit} OFFSET {offset}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(params_from_iter(&args), session_row)?
        .collect::<std::result::Result<_, _>>()?;
    Ok(SessionPage { total, items })
}

/// One step, without its full text.
#[derive(Debug, Clone, Serialize)]
pub struct EventRow {
    pub seq: i64,
    pub ts: Option<String>,
    pub kind: String,
    pub tool_name: Option<String>,
    pub category: Option<String>,
    pub title: String,
    pub is_error: bool,
    pub duration_ms: Option<i64>,
    /// Start of the text for prompts, responses, thinking and notices
    /// (the transcript). Tool output comes from [`step`].
    pub text: Option<String>,
    /// Context window size for the model call that produced this step.
    pub context_tokens: Option<i64>,
    pub tokens_out: i64,
    pub cost_usd: Option<f64>,
    pub wasted: bool,
    /// How a tool call was allowed or stopped (see `Event::approval`).
    pub approval: Option<String>,
    /// Skill, plugin or app that produced the step.
    pub via: Option<String>,
}

/// A file a step touched.
#[derive(Debug, Clone, Serialize)]
pub struct FileRow {
    pub seq: i64,
    pub path: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionDetail {
    pub session: SessionRow,
    pub events: Vec<EventRow>,
    pub files: Vec<FileRow>,
    pub findings: Vec<FindingRow>,
    pub patterns: Vec<crate::patterns::Pattern>,
    pub commits: Vec<CommitRow>,
    pub subagents: Vec<SessionRow>,
    /// Lines this session and its subagents added, and where they are now.
    pub lasted: Option<crate::survival::Counts>,
    pub turns: Vec<crate::turns::TurnRow>,
    pub prs: Vec<crate::turns::PrRow>,
}

/// A commit a session made.
#[derive(Debug, Clone, Serialize)]
pub struct CommitRow {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub hash: Option<String>,
    pub branch: Option<String>,
    pub message: Option<String>,
    pub agent: String,
    pub project_root: Option<String>,
    pub session_title: Option<String>,
}

const COMMIT_COLS: &str =
    "c.session_id, c.seq, e.ts, c.hash, c.branch, c.message, s.agent, s.project_root,
    coalesce(r.title, r.first_prompt)";
const COMMIT_JOIN: &str = "FROM commits c
    JOIN events e ON e.session_id = c.session_id AND e.seq = c.seq
    JOIN sessions s ON s.id = c.session_id
    LEFT JOIN sessions r ON r.id = coalesce(s.parent_id, s.id)";

fn commit_row(r: &Row) -> rusqlite::Result<CommitRow> {
    Ok(CommitRow {
        session_id: r.get(0)?,
        seq: r.get(1)?,
        ts: r.get(2)?,
        hash: r.get(3)?,
        branch: r.get(4)?,
        message: r.get(5)?,
        agent: r.get(6)?,
        project_root: r.get(7)?,
        session_title: r.get(8)?,
    })
}

/// Commits made by agent sessions under `f`, newest first.
pub fn commits(conn: &Connection, f: &Filter, limit: u32) -> Result<Vec<CommitRow>> {
    let (cond, args) = conditions(f);
    let sql =
        format!("SELECT {COMMIT_COLS} {COMMIT_JOIN} WHERE {cond} ORDER BY e.ts DESC LIMIT {limit}");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&args), commit_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

#[derive(Debug, Clone, Serialize)]
pub struct FindingRow {
    pub seq: i64,
    pub rule_id: String,
    pub title: String,
    pub severity: String,
    pub excerpt: String,
}

pub fn severity_name(n: i64) -> Option<String> {
    Some(
        match n {
            4 => "critical",
            3 => "high",
            2 => "medium",
            1 => "low",
            _ => return None,
        }
        .to_string(),
    )
}

fn severity_num(name: &str) -> i64 {
    match name {
        "critical" => 4,
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}

/// Longest transcript text sent with a session; the full text is in [`step`].
const TRANSCRIPT_TEXT: i64 = 4000;

pub fn session(conn: &Connection, id: &str) -> Result<Option<SessionDetail>> {
    let Some(session) = conn
        .query_row(
            &format!("SELECT {SESSION_COLS} FROM sessions s WHERE s.id = ?"),
            [id],
            session_row,
        )
        .optional()?
    else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT seq, ts, kind, tool_name, category, title, is_error, duration_ms,
                CASE WHEN kind != 'tool_call' THEN substr(body, 1, {TRANSCRIPT_TEXT}) END,
                context_tokens, tok_out, cost_usd, wasted, approval, via
         FROM events WHERE session_id = ? ORDER BY seq"
    ))?;
    let events = stmt
        .query_map([id], |r| {
            Ok(EventRow {
                seq: r.get(0)?,
                ts: r.get(1)?,
                kind: r.get(2)?,
                tool_name: r.get(3)?,
                category: r.get(4)?,
                title: r.get(5)?,
                is_error: r.get(6)?,
                duration_ms: r.get(7)?,
                text: r.get(8)?,
                context_tokens: r.get(9)?,
                tokens_out: r.get(10)?,
                cost_usd: r.get(11)?,
                wasted: r.get(12)?,
                approval: r.get(13)?,
                via: r.get(14)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = conn
        .prepare("SELECT seq, path, action FROM file_touches WHERE session_id = ? ORDER BY seq")?;
    let files = stmt
        .query_map([id], |r| {
            Ok(FileRow {
                seq: r.get(0)?,
                path: r.get(1)?,
                action: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT seq, rule_id, title, severity, excerpt FROM findings WHERE session_id = ?
         ORDER BY seq, severity DESC",
    )?;
    let findings = stmt
        .query_map([id], |r| {
            Ok(FindingRow {
                seq: r.get(0)?,
                rule_id: r.get(1)?,
                title: r.get(2)?,
                severity: severity_name(r.get(3)?).unwrap_or_default(),
                excerpt: r.get(4)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {SESSION_COLS} FROM sessions s WHERE s.parent_id = ? ORDER BY s.started_at"
    ))?;
    let subagents = stmt
        .query_map([id], session_row)?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT kind, start_seq, end_seq, title, wasted_ms, wasted_tokens FROM patterns
         WHERE session_id = ? ORDER BY start_seq",
    )?;
    let patterns = stmt
        .query_map([id], |r| {
            let kind: String = r.get(0)?;
            Ok(crate::patterns::Pattern {
                kind: match kind.as_str() {
                    "retry_loop" => "retry_loop",
                    "error_streak" => "error_streak",
                    "revert" => "revert",
                    _ => "edit_churn",
                },
                start_seq: r.get(1)?,
                end_seq: r.get(2)?,
                title: r.get(3)?,
                wasted_ms: r.get(4)?,
                wasted_tokens: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {COMMIT_COLS} {COMMIT_JOIN} WHERE c.session_id = ? ORDER BY c.seq"
    ))?;
    let commits = stmt
        .query_map([id], commit_row)?
        .collect::<std::result::Result<_, _>>()?;
    let lasted = crate::survival::session_counts(conn, id)?;
    let turns = crate::turns::session_turns(conn, id)?;
    let prs = crate::turns::prs(conn, &Filter::default(), Some(id), 50)?;
    Ok(Some(SessionDetail {
        session,
        events,
        files,
        findings,
        patterns,
        commits,
        subagents,
        lasted,
        turns,
        prs,
    }))
}

/// A file change in full, with its diff when the log had one.
#[derive(Debug, Clone, Serialize)]
pub struct Touch {
    pub path: String,
    pub action: String,
    pub diff: Option<String>,
    /// How many of the lines this change added are still in the file.
    pub lasted: Option<crate::survival::Counts>,
}

/// Everything about one step.
#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub seq: i64,
    pub ts: Option<String>,
    pub kind: String,
    pub tool_name: Option<String>,
    pub category: Option<String>,
    pub title: String,
    pub body: String,
    /// Tool input as JSON text.
    pub input: Option<String>,
    pub is_error: bool,
    pub duration_ms: Option<i64>,
    /// Text was cut to fit the index; the log has the rest.
    pub truncated: bool,
    pub touches: Vec<Touch>,
}

pub fn step(conn: &Connection, session_id: &str, seq: i64) -> Result<Option<Step>> {
    let Some(mut st) = conn
        .query_row(
            "SELECT seq, ts, kind, tool_name, category, title, body, input, is_error, duration_ms
             FROM events WHERE session_id = ? AND seq = ?",
            rusqlite::params![session_id, seq],
            |r| {
                let body: String = r.get(6)?;
                let input: Option<String> = r.get(7)?;
                let cut = |s: &str| s.ends_with('…') && s.len() >= crate::model::MAX_TEXT;
                Ok(Step {
                    seq: r.get(0)?,
                    ts: r.get(1)?,
                    kind: r.get(2)?,
                    tool_name: r.get(3)?,
                    category: r.get(4)?,
                    title: r.get(5)?,
                    truncated: cut(&body) || input.as_deref().is_some_and(cut),
                    body,
                    input,
                    is_error: r.get(8)?,
                    duration_ms: r.get(9)?,
                    touches: Vec::new(),
                })
            },
        )
        .optional()?
    else {
        return Ok(None);
    };
    let mut stmt = conn
        .prepare(
            "SELECT t.path, t.action, t.diff, v.added, v.kept, v.reworked, v.changed
             FROM file_touches t
             LEFT JOIN survival v ON v.session_id = t.session_id AND v.seq = t.seq AND v.path = t.path
             WHERE t.session_id = ? AND t.seq = ?",
        )?;
    st.touches = stmt
        .query_map(rusqlite::params![session_id, seq], |r| {
            let added: Option<i64> = r.get(3)?;
            Ok(Touch {
                path: r.get(0)?,
                action: r.get(1)?,
                diff: r.get(2)?,
                lasted: match added {
                    Some(added) => Some(crate::survival::Counts {
                        added,
                        kept: r.get(4)?,
                        reworked: r.get(5)?,
                        changed: r.get(6)?,
                    }),
                    None => None,
                },
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(Some(st))
}

/// One full-text match, with enough about its session to show it in a list.
#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub kind: String,
    pub category: Option<String>,
    pub tool_name: Option<String>,
    pub is_error: bool,
    pub title: String,
    /// Matching text; matched words sit between `\u{1}` and `\u{2}`.
    pub snippet: String,
    pub agent: String,
    pub session_title: Option<String>,
    pub project_root: Option<String>,
}

/// Turn what a person typed into a safe FTS5 query: every word quoted (so
/// `-`, `:` or quotes can't break the syntax), the last one as a prefix.
pub fn fts_query(input: &str) -> Option<String> {
    let words: Vec<String> = input
        .split_whitespace()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\""))
        .collect();
    let (last, rest) = words.split_last()?;
    let mut q = rest.join(" ");
    if !q.is_empty() {
        q.push(' ');
    }
    q.push_str(last);
    q.push('*');
    Some(q)
}

/// Full-text search over step titles and text, best matches first.
pub fn search(
    conn: &Connection,
    input: &str,
    f: &Filter,
    kind: Option<&str>,
    category: Option<&str>,
    limit: u32,
) -> Result<Vec<Hit>> {
    let Some(q) = fts_query(input) else {
        return Ok(Vec::new());
    };
    let (cond, mut args) = conditions(f);
    let mut extra = String::new();
    for (col, v) in [("e.kind", kind), ("e.category", category)] {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            extra.push_str(&format!(" AND {col} = ?"));
            args.push(v.to_string());
        }
    }
    args.insert(0, q);
    let sql = format!(
        "SELECT e.session_id, e.seq, e.ts, e.kind, e.category, e.tool_name, e.is_error, e.title,
                snippet(events_fts, -1, char(1), char(2), '…', 16),
                s.agent, coalesce(r.title, r.first_prompt), s.project_root
         FROM events_fts
         JOIN events e ON e.id = events_fts.rowid
         JOIN sessions s ON s.id = e.session_id
         LEFT JOIN sessions r ON r.id = coalesce(s.parent_id, s.id)
         WHERE events_fts MATCH ? AND {cond}{extra}
         ORDER BY rank LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let hits = stmt
        .query_map(params_from_iter(&args), |r| {
            Ok(Hit {
                session_id: r.get(0)?,
                seq: r.get(1)?,
                ts: r.get(2)?,
                kind: r.get(3)?,
                category: r.get(4)?,
                tool_name: r.get(5)?,
                is_error: r.get(6)?,
                title: r.get(7)?,
                snippet: r.get(8)?,
                agent: r.get(9)?,
                session_title: r.get(10)?,
                project_root: r.get(11)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::fts_query;

    #[test]
    fn fts_query_is_always_safe() {
        assert_eq!(
            fts_query("liquid glass").as_deref(),
            Some("\"liquid\" \"glass\"*")
        );
        assert_eq!(
            fts_query("rm -rf \"x").as_deref(),
            Some("\"rm\" \"-rf\" \"x\"*")
        );
        assert_eq!(fts_query("   "), None);
    }
}

/// How often each rule fired.
#[derive(Debug, Clone, Serialize)]
pub struct RuleSummary {
    pub rule_id: String,
    pub title: String,
    pub severity: String,
    pub count: i64,
    pub sessions: i64,
    pub last_ts: Option<String>,
}

pub fn audit_rules(conn: &Connection, f: &Filter) -> Result<Vec<RuleSummary>> {
    let (cond, args) = conditions(f);
    let sql = format!(
        "SELECT fd.rule_id, max(fd.title), max(fd.severity), count(*),
                count(DISTINCT coalesce(s.parent_id, s.id)), max(e.ts)
         FROM findings fd
         JOIN events e ON e.session_id = fd.session_id AND e.seq = fd.seq
         JOIN sessions s ON s.id = e.session_id
         WHERE {cond}
         GROUP BY fd.rule_id ORDER BY max(fd.severity) DESC, count(*) DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&args), |r| {
        Ok(RuleSummary {
            rule_id: r.get(0)?,
            title: r.get(1)?,
            severity: severity_name(r.get(2)?).unwrap_or_default(),
            count: r.get(3)?,
            sessions: r.get(4)?,
            last_ts: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// A finding with its session, for lists across sessions.
#[derive(Debug, Clone, Serialize)]
pub struct FindingHit {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub rule_id: String,
    pub title: String,
    pub severity: String,
    pub excerpt: String,
    pub step_title: String,
    pub agent: String,
    pub session_title: Option<String>,
    pub project_root: Option<String>,
}

pub fn findings(
    conn: &Connection,
    f: &Filter,
    rule: Option<&str>,
    min_severity: Option<&str>,
    limit: u32,
) -> Result<Vec<FindingHit>> {
    let (cond, mut args) = conditions(f);
    let mut extra = String::new();
    if let Some(r) = rule.filter(|r| !r.is_empty()) {
        extra.push_str(" AND fd.rule_id = ?");
        args.push(r.to_string());
    }
    if let Some(m) = min_severity.filter(|m| !m.is_empty()) {
        extra.push_str(&format!(" AND fd.severity >= {}", severity_num(m)));
    }
    let sql = format!(
        "SELECT fd.session_id, fd.seq, e.ts, fd.rule_id, fd.title, fd.severity, fd.excerpt, e.title,
                s.agent, coalesce(r.title, r.first_prompt), s.project_root
         FROM findings fd
         JOIN events e ON e.session_id = fd.session_id AND e.seq = fd.seq
         JOIN sessions s ON s.id = e.session_id
         LEFT JOIN sessions r ON r.id = coalesce(s.parent_id, s.id)
         WHERE {cond}{extra}
         ORDER BY e.ts DESC LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&args), |r| {
        Ok(FindingHit {
            session_id: r.get(0)?,
            seq: r.get(1)?,
            ts: r.get(2)?,
            rule_id: r.get(3)?,
            title: r.get(4)?,
            severity: severity_name(r.get(5)?).unwrap_or_default(),
            excerpt: r.get(6)?,
            step_title: r.get(7)?,
            agent: r.get(8)?,
            session_title: r.get(9)?,
            project_root: r.get(10)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Use of one tool across sessions.
#[derive(Debug, Clone, Serialize)]
pub struct ToolStat {
    pub tool_name: String,
    pub category: Option<String>,
    pub calls: i64,
    pub errors: i64,
    pub total_ms: i64,
    pub sessions: i64,
    pub last_ts: Option<String>,
}

pub fn tools(conn: &Connection, f: &Filter) -> Result<Vec<ToolStat>> {
    let (cond, args) = conditions(f);
    let sql = format!(
        "SELECT e.tool_name, max(e.category), count(*), sum(e.is_error), coalesce(sum(e.duration_ms), 0),
                count(DISTINCT coalesce(s.parent_id, s.id)), max(e.ts)
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.kind = 'tool_call' AND e.tool_name IS NOT NULL AND {cond}
         GROUP BY e.tool_name ORDER BY count(*) DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&args), |r| {
        Ok(ToolStat {
            tool_name: r.get(0)?,
            category: r.get(1)?,
            calls: r.get(2)?,
            errors: r.get(3)?,
            total_ms: r.get(4)?,
            sessions: r.get(5)?,
            last_ts: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// One step, with its session, for lists across sessions.
#[derive(Debug, Clone, Serialize)]
pub struct StepRef {
    pub session_id: String,
    pub seq: i64,
    pub ts: Option<String>,
    pub title: String,
    pub is_error: bool,
    pub duration_ms: Option<i64>,
    /// File action for file history; null for tool calls.
    pub action: Option<String>,
    pub agent: String,
    pub session_title: Option<String>,
}

const STEP_REF: &str = "e.session_id, e.seq, e.ts, e.title, e.is_error, e.duration_ms";

fn step_ref(r: &Row, action: bool) -> rusqlite::Result<StepRef> {
    Ok(StepRef {
        session_id: r.get(0)?,
        seq: r.get(1)?,
        ts: r.get(2)?,
        title: r.get(3)?,
        is_error: r.get(4)?,
        duration_ms: r.get(5)?,
        action: if action { r.get(6)? } else { None },
        agent: r.get(7)?,
        session_title: r.get(8)?,
    })
}

/// Most recent calls of one tool.
pub fn tool_calls(conn: &Connection, f: &Filter, tool: &str, limit: u32) -> Result<Vec<StepRef>> {
    let (cond, mut args) = conditions(f);
    args.insert(0, tool.to_string());
    let sql = format!(
        "SELECT {STEP_REF}, NULL, s.agent, coalesce(r.title, r.first_prompt)
         FROM events e JOIN sessions s ON s.id = e.session_id
         LEFT JOIN sessions r ON r.id = coalesce(s.parent_id, s.id)
         WHERE e.tool_name = ? AND {cond}
         ORDER BY e.ts DESC LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&args), |r| step_ref(r, false))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// What happened to one file across sessions.
#[derive(Debug, Clone, Serialize)]
pub struct FileStat {
    pub path: String,
    pub reads: i64,
    pub edits: i64,
    pub creates: i64,
    pub deletes: i64,
    pub sessions: i64,
    pub last_ts: Option<String>,
}

/// Files touched under `f`, most changed first.
pub fn files(conn: &Connection, f: &Filter, limit: u32) -> Result<Vec<FileStat>> {
    let (cond, args) = conditions(f);
    let sql = format!(
        "SELECT ft.path, sum(ft.action = 'read'), sum(ft.action = 'edit'), sum(ft.action = 'create'),
                sum(ft.action = 'delete'), count(DISTINCT coalesce(s.parent_id, s.id)), max(e.ts)
         FROM file_touches ft
         JOIN events e ON e.session_id = ft.session_id AND e.seq = ft.seq
         JOIN sessions s ON s.id = e.session_id
         WHERE {cond}
         GROUP BY ft.path
         ORDER BY (sum(ft.action != 'read')) DESC, count(*) DESC LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(&args), |r| {
        Ok(FileStat {
            path: r.get(0)?,
            reads: r.get(1)?,
            edits: r.get(2)?,
            creates: r.get(3)?,
            deletes: r.get(4)?,
            sessions: r.get(5)?,
            last_ts: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Every step that touched `path`, newest first.
pub fn file_history(conn: &Connection, path: &str, limit: u32) -> Result<Vec<StepRef>> {
    let sql = format!(
        "SELECT {STEP_REF}, ft.action, s.agent, coalesce(r.title, r.first_prompt)
         FROM file_touches ft
         JOIN events e ON e.session_id = ft.session_id AND e.seq = ft.seq
         JOIN sessions s ON s.id = e.session_id
         LEFT JOIN sessions r ON r.id = coalesce(s.parent_id, s.id)
         WHERE ft.path = ?
         ORDER BY e.ts DESC LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([path], |r| step_ref(r, true))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Past steps a guard rule would have blocked: how many, and the latest few.
pub fn guard_matches(
    conn: &Connection,
    tool: &str,
    pattern: &str,
    examples: usize,
) -> Result<(i64, Vec<StepRef>)> {
    let Ok(re) = regex::Regex::new(pattern) else {
        return Ok((0, Vec::new()));
    };
    let sql = format!(
        "SELECT {STEP_REF}, NULL, s.agent, coalesce(r.title, r.first_prompt)
         FROM events e JOIN sessions s ON s.id = e.session_id
         LEFT JOIN sessions r ON r.id = coalesce(s.parent_id, s.id)
         WHERE e.kind = 'tool_call'
           AND (?1 = '*' OR e.tool_name = ?1 OR (?1 = 'Bash' AND e.category = 'command'))
         ORDER BY e.ts DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([tool])?;
    let (mut n, mut out) = (0, Vec::new());
    while let Some(r) = rows.next()? {
        let title: String = r.get(3)?;
        if re.is_match(&title) {
            n += 1;
            if out.len() < examples {
                out.push(step_ref(r, false)?);
            }
        }
    }
    Ok((n, out))
}
