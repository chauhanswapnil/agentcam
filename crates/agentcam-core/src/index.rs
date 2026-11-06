//! SQLite index of every parsed session, with full-text search (FTS5).
//!
//! The index is a cache: it can always be rebuilt from the agent logs. It
//! only re-parses files whose size or modified time changed. Sessions stay
//! in the index after their log file is deleted, so agentcam keeps history
//! the agents themselves throw away.

use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

use rayon::prelude::*;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::model::ParsedSession;
use crate::{Agent, LogSource, discover, parse_file};

/// Bump when the schema changes. A mismatch drops and rebuilds the index.
const SCHEMA_VERSION: i64 = 11;

/// Gaps between steps longer than this count as idle, not agent time.
pub const IDLE_CAP_MS: i64 = 5 * 60 * 1000;

const SCHEMA: &str = r#"
CREATE TABLE sources (
    path        TEXT PRIMARY KEY,
    agent       TEXT NOT NULL,
    size        INTEGER NOT NULL,
    mtime_ns    INTEGER NOT NULL,
    session_id  TEXT,
    bad_lines   INTEGER NOT NULL DEFAULT 0,
    error       TEXT
);

CREATE TABLE sessions (
    id                  TEXT PRIMARY KEY,
    agent               TEXT NOT NULL,
    source_path         TEXT NOT NULL,
    parent_id           TEXT,
    project_path        TEXT,
    project_root        TEXT,
    title               TEXT,
    first_prompt        TEXT,
    git_branch          TEXT,
    model               TEXT,
    agent_version       TEXT,
    started_at          TEXT,
    ended_at            TEXT,
    input_tokens        INTEGER NOT NULL DEFAULT 0,
    output_tokens       INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens   INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens  INTEGER NOT NULL DEFAULT 0,
    cost_usd            REAL,
    event_count         INTEGER NOT NULL DEFAULT 0,
    tool_call_count     INTEGER NOT NULL DEFAULT 0,
    error_count         INTEGER NOT NULL DEFAULT 0,
    file_count          INTEGER NOT NULL DEFAULT 0,
    active_ms           INTEGER NOT NULL DEFAULT 0,
    finding_count       INTEGER NOT NULL DEFAULT 0,
    max_severity        INTEGER NOT NULL DEFAULT 0,
    est_cost_usd        REAL,
    cost_unpriced       INTEGER NOT NULL DEFAULT 0,
    peak_context        INTEGER NOT NULL DEFAULT 0,
    wasted_ms           INTEGER NOT NULL DEFAULT 0,
    pattern_count       INTEGER NOT NULL DEFAULT 0,
    commit_count        INTEGER NOT NULL DEFAULT 0,
    machine_id          TEXT NOT NULL DEFAULT '',
    os_user             TEXT,
    project_remote      TEXT,
    version             TEXT NOT NULL DEFAULT '',
    indexed_at          TEXT,
    entrypoint          TEXT,
    reasoning_tokens    INTEGER NOT NULL DEFAULT 0,
    service_tier        TEXT,
    turn_count          INTEGER NOT NULL DEFAULT 0,
    skills_listed       INTEGER NOT NULL DEFAULT 0,
    skills_listing_chars INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX sessions_root ON sessions(project_root);
CREATE INDEX sessions_parent ON sessions(parent_id);
CREATE INDEX sessions_started ON sessions(started_at);
CREATE INDEX sessions_project ON sessions(project_path);
CREATE INDEX sessions_source ON sessions(source_path);

CREATE TABLE events (
    id            INTEGER PRIMARY KEY,
    session_id    TEXT NOT NULL,
    seq           INTEGER NOT NULL,
    ts            TEXT,
    kind          TEXT NOT NULL,
    tool_name     TEXT,
    category      TEXT,
    title         TEXT NOT NULL,
    body          TEXT NOT NULL,
    input         TEXT,
    is_error      INTEGER NOT NULL DEFAULT 0,
    duration_ms   INTEGER,
    source_offset INTEGER NOT NULL,
    gap_ms        INTEGER NOT NULL DEFAULT 0,
    model         TEXT,
    tok_in        INTEGER NOT NULL DEFAULT 0,
    tok_out       INTEGER NOT NULL DEFAULT 0,
    tok_cache_read  INTEGER NOT NULL DEFAULT 0,
    tok_cache_write INTEGER NOT NULL DEFAULT 0,
    context_tokens  INTEGER,
    cost_usd      REAL,
    wasted        INTEGER NOT NULL DEFAULT 0,
    tok_reasoning INTEGER NOT NULL DEFAULT 0,
    approval      TEXT,
    via           TEXT,
    UNIQUE(session_id, seq)
);
CREATE INDEX events_ts ON events(ts);
CREATE INDEX events_tool ON events(tool_name);

CREATE TABLE file_touches (
    session_id  TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    path        TEXT NOT NULL,
    action      TEXT NOT NULL,
    diff        TEXT
);
CREATE INDEX file_touches_session ON file_touches(session_id, seq);
CREATE INDEX file_touches_path ON file_touches(path);

CREATE TABLE findings (
    session_id  TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    rule_id     TEXT NOT NULL,
    title       TEXT NOT NULL,
    severity    INTEGER NOT NULL,
    excerpt     TEXT NOT NULL
);
CREATE INDEX findings_session ON findings(session_id, seq);
CREATE INDEX findings_rule ON findings(rule_id);

CREATE TABLE patterns (
    session_id    TEXT NOT NULL,
    kind          TEXT NOT NULL,
    start_seq     INTEGER NOT NULL,
    end_seq       INTEGER NOT NULL,
    title         TEXT NOT NULL,
    wasted_ms     INTEGER NOT NULL,
    wasted_tokens INTEGER NOT NULL
);
CREATE INDEX patterns_session ON patterns(session_id);

CREATE TABLE commits (
    session_id  TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    hash        TEXT,
    branch      TEXT,
    message     TEXT
);
CREATE INDEX commits_session ON commits(session_id);

CREATE TABLE limits (
    session_id      TEXT NOT NULL,
    seq             INTEGER,
    ts              TEXT,
    agent           TEXT NOT NULL,
    window          TEXT NOT NULL,
    window_minutes  INTEGER,
    used_percent    REAL,
    resets_at       TEXT,
    resets_text     TEXT,
    hit             INTEGER NOT NULL,
    plan            TEXT
);
CREATE INDEX limits_session ON limits(session_id);
CREATE INDEX limits_ts ON limits(ts);

CREATE TABLE survival (
    session_id  TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    path        TEXT NOT NULL,
    ts          TEXT NOT NULL,
    added       INTEGER NOT NULL,
    kept        INTEGER NOT NULL,
    reworked    INTEGER NOT NULL,
    changed     INTEGER NOT NULL
);
CREATE INDEX survival_session ON survival(session_id, seq);
CREATE INDEX survival_path ON survival(path);
CREATE INDEX survival_ts ON survival(ts);

CREATE TABLE survival_files (path TEXT PRIMARY KEY, sig TEXT NOT NULL);

CREATE TABLE turns (
    session_id      TEXT NOT NULL,
    n               INTEGER NOT NULL,
    seq             INTEGER NOT NULL,
    end_seq         INTEGER NOT NULL,
    started_at      TEXT,
    ended_at        TEXT,
    model           TEXT,
    effort          TEXT,
    mode            TEXT,
    approval_policy TEXT,
    sandbox         TEXT
);
CREATE INDEX turns_session ON turns(session_id, n);
CREATE INDEX turns_started ON turns(started_at);

CREATE TABLE prs (
    session_id  TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    ts          TEXT,
    number      INTEGER,
    url         TEXT NOT NULL,
    repo        TEXT
);
CREATE INDEX prs_session ON prs(session_id);

CREATE TABLE skill_uses (
    session_id    TEXT NOT NULL,
    seq           INTEGER NOT NULL,
    ts            TEXT,
    skill         TEXT NOT NULL,
    trigger       TEXT NOT NULL,
    args          TEXT,
    loaded_chars  INTEGER
);
CREATE INDEX skill_uses_session ON skill_uses(session_id);
CREATE INDEX skill_uses_skill ON skill_uses(skill);

CREATE TABLE session_skills (session_id TEXT NOT NULL, name TEXT NOT NULL);
CREATE INDEX session_skills_session ON session_skills(session_id);

CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE VIRTUAL TABLE events_fts USING fts5(
    title, body,
    content='events', content_rowid='id',
    tokenize='unicode61'
);
"#;

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, IndexError>;

/// What one `update` run did.
#[derive(Debug, Default, Clone, Serialize)]
pub struct UpdateReport {
    pub files_seen: usize,
    pub files_parsed: usize,
    pub files_failed: usize,
    pub events_written: usize,
    pub bad_lines: u64,
    /// Project files re-read to see which agent lines are still there.
    pub files_compared: usize,
    pub elapsed_ms: u128,
}

/// One full-text search hit.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub session_id: String,
    pub seq: u32,
    pub kind: String,
    pub title: String,
    pub snippet: String,
}

pub struct Index {
    conn: Connection,
    rules: crate::audit::Rules,
    prices: crate::prices::Prices,
    machine_id: String,
    os_user: Option<String>,
}

/// The user's own rules: `rules.yaml` next to the index.
pub fn user_rules_path(db: &Path) -> PathBuf {
    db.with_file_name("rules.yaml")
}

/// `~/Library/Application Support/agentcam/index.db` on macOS, the XDG data
/// folder on Linux, `%LOCALAPPDATA%` on Windows.
pub fn default_db_path() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("agentcam").join("index.db"))
}

impl Index {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let machine = crate::identity::machine_id(path.parent().unwrap_or(Path::new(".")));
        Self::init(Connection::open(path)?, machine)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, crate::identity::random_id())
    }

    /// This machine's id, kept next to the index.
    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    fn init(conn: Connection, machine_id: String) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = OFF;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA_VERSION {
            // Drop every table an older version made, whatever it was.
            // Virtual tables first: they take their shadow tables with them.
            let tables: Vec<String> = conn
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
                     ORDER BY sql NOT LIKE 'CREATE VIRTUAL%'",
                )?
                .query_map([], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            for t in tables {
                conn.execute_batch(&format!(
                    "DROP TABLE IF EXISTS \"{}\"",
                    t.replace('"', "\"\"")
                ))?;
            }
            conn.execute_batch(SCHEMA)?;
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        }
        Ok(Index {
            conn,
            rules: crate::audit::Rules::builtin(),
            prices: crate::prices::Prices::builtin(),
            machine_id,
            os_user: crate::identity::os_user(),
        })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Index every new or changed log file under `sources`. With `full`,
    /// re-parse everything.
    /// Use these rules from the next `update` on. Changed rules re-check
    /// every session.
    pub fn set_rules(&mut self, rules: crate::audit::Rules) {
        self.rules = rules;
    }

    /// Use these prices from the next `update` on (changes re-check all).
    pub fn set_prices(&mut self, prices: crate::prices::Prices) {
        self.prices = prices;
    }

    fn fingerprint(&self) -> String {
        format!("{}:{}", self.rules.fingerprint, self.prices.fingerprint)
    }

    pub fn rules(&self) -> &crate::audit::Rules {
        &self.rules
    }

    pub fn update(&mut self, sources: &[LogSource], full: bool) -> Result<UpdateReport> {
        let start = Instant::now();
        let stored: Option<String> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'rules'", [], |r| {
                r.get(0)
            })
            .optional()?;
        let full = full || stored.as_deref() != Some(self.fingerprint().as_str());
        let files = discover(sources);
        let mut report = UpdateReport {
            files_seen: files.len(),
            ..Default::default()
        };

        let seen: std::collections::HashSet<String> = files
            .iter()
            .map(|(_, p)| p.to_string_lossy().into_owned())
            .collect();
        let mut changed = Vec::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT size, mtime_ns FROM sources WHERE path = ?")?;
            for (agent, path) in files {
                let Ok(meta) = std::fs::metadata(&path) else {
                    continue;
                };
                let size = meta.len() as i64;
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos() as i64);
                let key = path.to_string_lossy().into_owned();
                let known: Option<(i64, i64)> = stmt
                    .query_row([&key], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?;
                if full || known != Some((size, mtime)) {
                    changed.push((agent, path, key, size, mtime));
                }
            }
        }

        let rules = &self.rules;
        let parsed: Vec<_> = changed
            .into_par_iter()
            .map(|(agent, path, key, size, mtime)| {
                let res = parse_file(agent, &path);
                let res = res.map(|p| {
                    let f = rules.check(&p);
                    (p, f)
                });
                (agent, key, size, mtime, res)
            })
            .collect();

        let fingerprint = self.fingerprint();
        let tx = self.conn.transaction()?;
        // A file that still exists but is no longer picked up (say, a type
        // of log we now skip) shouldn't linger. Deleted logs stay indexed.
        let stale: Vec<String> = {
            let mut stmt = tx.prepare("SELECT path FROM sources")?;
            let paths = stmt.query_map([], |r| r.get::<_, String>(0))?;
            paths
                .filter_map(std::result::Result::ok)
                .filter(|p| !seen.contains(p) && Path::new(p).exists())
                .collect()
        };
        for key in stale {
            remove_source(&tx, &key)?;
        }
        let indexed_at = crate::util::format_ts_ms(crate::util::now_ms());
        let mut remotes: std::collections::HashMap<String, Option<String>> = Default::default();
        for (agent, key, size, mtime, res) in parsed {
            remove_source(&tx, &key)?;
            match res {
                Ok((p, findings)) => {
                    report.files_parsed += 1;
                    report.bad_lines += u64::from(p.bad_lines);
                    report.events_written += p.events.len();
                    // Changes whenever the source or the rules change, so a
                    // sync only sends sessions that are new to the server.
                    let version = {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        (size, mtime, &fingerprint).hash(&mut h);
                        format!("{:016x}", h.finish())
                    };
                    let remote = p.session.project_path.as_deref().and_then(|path| {
                        let root = crate::project_root(path);
                        remotes
                            .entry(root.clone())
                            .or_insert_with(|| crate::identity::git_remote(Path::new(&root)))
                            .clone()
                    });
                    let ctx = WriteCtx {
                        prices: &self.prices,
                        machine_id: &self.machine_id,
                        os_user: self.os_user.as_deref(),
                        version: &version,
                        remote: remote.as_deref(),
                        indexed_at: &indexed_at,
                    };
                    write_session(&tx, &p, &findings, &ctx)?;
                    tx.execute(
                        "INSERT INTO sources (path, agent, size, mtime_ns, session_id, bad_lines)
                         VALUES (?, ?, ?, ?, ?, ?)",
                        params![key, agent.as_str(), size, mtime, p.session.id, p.bad_lines],
                    )?;
                }
                Err(e) => {
                    report.files_failed += 1;
                    tx.execute(
                        "INSERT INTO sources (path, agent, size, mtime_ns, error) VALUES (?, ?, ?, ?, ?)",
                        params![key, agent.as_str(), size, mtime, e.to_string()],
                    )?;
                }
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('rules', ?)",
            [&fingerprint],
        )?;
        tx.commit()?;
        // Files change without the logs changing, so check them every run;
        // unchanged files cost one stat each.
        let survival = crate::survival::refresh(&mut self.conn)?;
        report.files_compared = survival.files_checked;

        report.elapsed_ms = start.elapsed().as_millis();
        Ok(report)
    }

    /// Full-text search over event titles and bodies. `query` uses FTS5
    /// syntax; plain words work.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.session_id, e.seq, e.kind, e.title,
                    snippet(events_fts, 1, '[', ']', '…', 12)
             FROM events_fts JOIN events e ON e.id = events_fts.rowid
             WHERE events_fts MATCH ?1
             ORDER BY rank LIMIT ?2",
        )?;
        let hits = stmt
            .query_map(params![query, limit as i64], |r| {
                Ok(SearchHit {
                    session_id: r.get(0)?,
                    seq: r.get(1)?,
                    kind: r.get(2)?,
                    title: r.get(3)?,
                    snippet: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(hits)
    }
}

/// Drop everything indexed from the log file at `key`.
fn remove_source(tx: &rusqlite::Transaction, key: &str) -> Result<()> {
    let ids = "SELECT id FROM sessions WHERE source_path = ?1";
    tx.execute(
        &format!(
            "INSERT INTO events_fts(events_fts, rowid, title, body)
             SELECT 'delete', id, title, body FROM events WHERE session_id IN ({ids})"
        ),
        [key],
    )?;
    tx.execute(
        &format!("DELETE FROM events WHERE session_id IN ({ids})"),
        [key],
    )?;
    tx.execute(
        &format!("DELETE FROM file_touches WHERE session_id IN ({ids})"),
        [key],
    )?;
    for table in [
        "findings",
        "patterns",
        "commits",
        "limits",
        "turns",
        "prs",
        "skill_uses",
        "session_skills",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE session_id IN ({ids})"),
            [key],
        )?;
    }
    tx.execute("DELETE FROM sessions WHERE source_path = ?1", [key])?;
    tx.execute("DELETE FROM sources WHERE path = ?1", [key])?;
    Ok(())
}

/// Facts about where and when a session was indexed.
struct WriteCtx<'a> {
    prices: &'a crate::prices::Prices,
    machine_id: &'a str,
    os_user: Option<&'a str>,
    version: &'a str,
    remote: Option<&'a str>,
    indexed_at: &'a str,
}

fn write_session(
    tx: &rusqlite::Transaction,
    p: &ParsedSession,
    findings: &[crate::audit::Finding],
    ctx: &WriteCtx,
) -> Result<()> {
    let prices = ctx.prices;
    let s = &p.session;
    // Another file may have claimed this id before (a moved or copied log).
    remove_session_rows(tx, &s.id)?;

    let tool_calls = p.events.iter().filter(|e| e.category.is_some()).count();
    let errors = p.events.iter().filter(|e| e.is_error).count();
    let mut paths: Vec<&str> = p
        .touches
        .iter()
        .filter(|t| t.action != crate::model::FileAction::Read)
        .map(|t| t.path.as_str())
        .collect();
    paths.sort_unstable();
    paths.dedup();

    // Time since the previous step, capped so idle stretches don't count.
    let mut prev: Option<i64> = None;
    let gaps: Vec<i64> = p
        .events
        .iter()
        .map(|e| {
            let t = e.timestamp.as_deref().and_then(crate::util::parse_ts_ms);
            let gap = match (prev, t) {
                (Some(a), Some(b)) => (b - a).clamp(0, IDLE_CAP_MS),
                _ => 0,
            };
            if t.is_some() {
                prev = t;
            }
            gap
        })
        .collect();
    let active_ms: i64 = gaps.iter().sum();

    // Cost per step from the price table; a model without a price leaves
    // the session's estimate marked incomplete.
    let costs: Vec<Option<f64>> = p
        .events
        .iter()
        .map(|e| e.usage.as_ref().and_then(|u| prices.cost(u)))
        .collect();
    let unpriced = p
        .events
        .iter()
        .zip(&costs)
        .any(|(e, c)| e.usage.is_some() && c.is_none());
    let priced: Vec<f64> = costs.iter().flatten().copied().collect();
    let est_cost = (!priced.is_empty()).then(|| priced.iter().sum::<f64>());
    let peak_context = p
        .events
        .iter()
        .filter_map(|e| e.usage.as_ref())
        .map(|u| u.context())
        .max()
        .unwrap_or(0);
    let patterns = crate::patterns::detect(p, &gaps);
    let wasted = crate::patterns::wasted_ms(p, &gaps, &patterns);
    let wasted_steps = crate::patterns::wasted_steps(p, &patterns);
    let commits = crate::commits::detect(p);
    let root = s.project_path.as_deref().map(crate::project_root);

    tx.execute(
        "INSERT INTO sessions (id, agent, source_path, parent_id, project_path, title, first_prompt,
            git_branch, model, agent_version, started_at, ended_at, input_tokens, output_tokens,
            cache_read_tokens, cache_write_tokens, cost_usd, event_count, tool_call_count,
            error_count, file_count, project_root, active_ms, finding_count, max_severity,
            est_cost_usd, cost_unpriced, peak_context, wasted_ms, pattern_count, commit_count,
            machine_id, os_user, project_remote, version, indexed_at, entrypoint,
            reasoning_tokens, service_tier, turn_count, skills_listed, skills_listing_chars)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
                 ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42)",
        params![
            s.id,
            s.agent.map(Agent::as_str).unwrap_or("unknown"),
            s.source_path,
            s.parent_id,
            s.project_path,
            s.title,
            s.first_prompt,
            s.git_branch,
            s.model,
            s.agent_version,
            s.started_at,
            s.ended_at,
            s.input_tokens as i64,
            s.output_tokens as i64,
            s.cache_read_tokens as i64,
            s.cache_write_tokens as i64,
            s.cost_usd,
            p.events.len() as i64,
            tool_calls as i64,
            errors as i64,
            paths.len() as i64,
            root,
            active_ms,
            findings.len() as i64,
            findings.iter().map(|f| f.severity as i64).max().unwrap_or(0),
            est_cost,
            unpriced,
            peak_context as i64,
            wasted,
            patterns.len() as i64,
            commits.len() as i64,
            ctx.machine_id,
            ctx.os_user,
            ctx.remote,
            ctx.version,
            ctx.indexed_at,
            s.entrypoint,
            s.reasoning_tokens as i64,
            s.service_tier,
            p.turns.len() as i64,
            p.skills_available.len() as i64,
            p.skills_listing_chars as i64,
        ],
    )?;

    let mut ev = tx.prepare_cached(
        "INSERT INTO events (session_id, seq, ts, kind, tool_name, category, title, body, input,
            is_error, duration_ms, source_offset, gap_ms, model, tok_in, tok_out, tok_cache_read,
            tok_cache_write, context_tokens, cost_usd, wasted, tok_reasoning, approval, via)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                 ?22, ?23, ?24)",
    )?;
    let mut fts =
        tx.prepare_cached("INSERT INTO events_fts (rowid, title, body) VALUES (?, ?, ?)")?;
    for ((e, gap), cost) in p.events.iter().zip(&gaps).zip(&costs) {
        let input = e
            .input
            .as_ref()
            .map(|v| crate::model::truncate(&v.to_string(), crate::model::MAX_TEXT));
        ev.execute(params![
            s.id,
            e.seq,
            e.timestamp,
            e.kind.as_str(),
            e.tool_name,
            e.category.map(|c| c.as_str()),
            e.title,
            e.body,
            input,
            e.is_error,
            e.duration_ms.map(|d| d as i64),
            e.source_offset as i64,
            gap,
            e.usage.as_ref().and_then(|u| u.model.clone()),
            e.usage.as_ref().map_or(0, |u| u.input as i64),
            e.usage.as_ref().map_or(0, |u| u.output as i64),
            e.usage.as_ref().map_or(0, |u| u.cache_read as i64),
            e.usage.as_ref().map_or(0, |u| u.cache_write as i64),
            e.usage.as_ref().map(|u| u.context() as i64),
            cost,
            wasted_steps.contains(&e.seq),
            e.usage.as_ref().map_or(0, |u| u.reasoning as i64),
            e.approval,
            e.via,
        ])?;
        fts.execute(params![tx.last_insert_rowid(), e.title, e.body])?;
    }

    let mut ft = tx.prepare_cached(
        "INSERT INTO file_touches (session_id, seq, path, action, diff) VALUES (?, ?, ?, ?, ?)",
    )?;
    for t in &p.touches {
        ft.execute(params![s.id, t.seq, t.path, t.action.as_str(), t.diff])?;
    }

    let mut pt = tx.prepare_cached(
        "INSERT INTO patterns (session_id, kind, start_seq, end_seq, title, wasted_ms, wasted_tokens)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )?;
    for x in &patterns {
        pt.execute(params![
            s.id,
            x.kind,
            x.start_seq,
            x.end_seq,
            x.title,
            x.wasted_ms,
            x.wasted_tokens
        ])?;
    }
    let mut cm = tx.prepare_cached(
        "INSERT INTO commits (session_id, seq, hash, branch, message) VALUES (?, ?, ?, ?, ?)",
    )?;
    for c in &commits {
        cm.execute(params![s.id, c.seq, c.hash, c.branch, c.message])?;
    }

    let mut tn = tx.prepare_cached(
        "INSERT INTO turns (session_id, n, seq, end_seq, started_at, ended_at, model, effort, mode,
            approval_policy, sandbox) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )?;
    for (n, t) in p.turns.iter().enumerate() {
        // A turn runs up to the step before the next one starts.
        let end = p
            .turns
            .get(n + 1)
            .map_or(p.events.len() as u32, |x| x.seq)
            .saturating_sub(1)
            .max(t.seq);
        tn.execute(params![
            s.id,
            n as i64,
            t.seq,
            end,
            t.started_at,
            t.ended_at,
            t.model,
            t.effort,
            t.mode,
            t.approval_policy,
            t.sandbox
        ])?;
    }
    let mut su = tx.prepare_cached(
        "INSERT INTO skill_uses (session_id, seq, ts, skill, trigger, args, loaded_chars)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )?;
    for u in p
        .skill_uses
        .iter()
        .chain(crate::skills::detect_reads(p).iter())
    {
        su.execute(params![
            s.id,
            u.seq,
            u.timestamp,
            u.skill,
            u.trigger,
            u.args,
            u.loaded_chars.map(|c| c as i64)
        ])?;
    }
    let mut sk =
        tx.prepare_cached("INSERT INTO session_skills (session_id, name) VALUES (?, ?)")?;
    for name in &p.skills_available {
        sk.execute(params![s.id, name])?;
    }
    let mut pr = tx.prepare_cached(
        "INSERT INTO prs (session_id, seq, ts, number, url, repo) VALUES (?, ?, ?, ?, ?, ?)",
    )?;
    for x in &p.prs {
        pr.execute(params![s.id, x.seq, x.timestamp, x.number, x.url, x.repo])?;
    }

    let mut lm = tx.prepare_cached(
        "INSERT INTO limits (session_id, seq, ts, agent, window, window_minutes, used_percent,
            resets_at, resets_text, hit, plan) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )?;
    for l in &p.limits {
        lm.execute(params![
            s.id,
            l.seq,
            l.timestamp,
            s.agent.map(Agent::as_str).unwrap_or("unknown"),
            l.window,
            l.window_minutes,
            l.used_percent,
            l.resets_at,
            l.resets_text,
            l.hit,
            l.plan
        ])?;
    }

    let mut fi = tx.prepare_cached(
        "INSERT INTO findings (session_id, seq, rule_id, title, severity, excerpt) VALUES (?, ?, ?, ?, ?, ?)",
    )?;
    for f in findings {
        fi.execute(params![
            s.id,
            f.seq,
            f.rule_id,
            f.title,
            f.severity as i64,
            f.excerpt
        ])?;
    }
    Ok(())
}

fn remove_session_rows(tx: &rusqlite::Transaction, id: &str) -> Result<()> {
    crate::survival::forget_session(tx, id)?;
    tx.execute(
        "INSERT INTO events_fts(events_fts, rowid, title, body)
         SELECT 'delete', id, title, body FROM events WHERE session_id = ?1",
        [id],
    )?;
    tx.execute("DELETE FROM events WHERE session_id = ?1", [id])?;
    tx.execute("DELETE FROM file_touches WHERE session_id = ?1", [id])?;
    for table in [
        "findings",
        "patterns",
        "commits",
        "limits",
        "turns",
        "prs",
        "skill_uses",
        "session_skills",
    ] {
        tx.execute(&format!("DELETE FROM {table} WHERE session_id = ?1"), [id])?;
    }
    tx.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
    Ok(())
}
