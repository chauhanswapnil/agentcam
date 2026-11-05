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
fn conditions(f: &Filter) -> (String, Vec<String>) {
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
                count(DISTINCT s.project_root)
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
                sum(e.gap_ms), count(*), sum(e.kind = 'tool_call')
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
                sum(e.gap_ms), count(*), max(e.ts), group_concat(DISTINCT s.agent)
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
}

const SESSION_COLS: &str = "s.id, s.agent, s.title, s.first_prompt, s.project_path, s.project_root,
    s.git_branch, s.model, s.parent_id, s.started_at, s.ended_at, s.active_ms, s.event_count,
    s.tool_call_count, s.error_count, s.file_count, s.input_tokens, s.output_tokens,
    s.cache_read_tokens, s.cost_usd,
    (SELECT count(*) FROM sessions c WHERE c.parent_id = s.id)";

fn session_row(r: &Row) -> rusqlite::Result<SessionRow> {
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
    pub subagents: Vec<SessionRow>,
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
                CASE WHEN kind != 'tool_call' THEN substr(body, 1, {TRANSCRIPT_TEXT}) END
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
    let mut stmt = conn.prepare(&format!(
        "SELECT {SESSION_COLS} FROM sessions s WHERE s.parent_id = ? ORDER BY s.started_at"
    ))?;
    let subagents = stmt
        .query_map([id], session_row)?
        .collect::<std::result::Result<_, _>>()?;
    Ok(Some(SessionDetail {
        session,
        events,
        files,
        subagents,
    }))
}

/// A file change in full, with its diff when the log had one.
#[derive(Debug, Clone, Serialize)]
pub struct Touch {
    pub path: String,
    pub action: String,
    pub diff: Option<String>,
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
        .prepare("SELECT path, action, diff FROM file_touches WHERE session_id = ? AND seq = ?")?;
    st.touches = stmt
        .query_map(rusqlite::params![session_id, seq], |r| {
            Ok(Touch {
                path: r.get(0)?,
                action: r.get(1)?,
                diff: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(Some(st))
}
