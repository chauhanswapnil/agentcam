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

/// The schema `SCHEMA` creates, the first one released (0.1.0). Never
/// change it: add a step to `MIGRATIONS` instead, so an index from an older
/// release keeps the sessions whose logs the agents have since deleted.
const BASE_VERSION: i64 = 14;

/// One upgrade step, from `BASE_VERSION + i` to `BASE_VERSION + i + 1`.
pub struct Migration {
    /// SQL run in one transaction: `ALTER TABLE ... ADD COLUMN`, new tables,
    /// backfills from data already in the index.
    pub sql: &'static str,
    /// Re-read every log on the next run, to fill in what only the logs
    /// hold. Sessions whose logs are gone keep what they had.
    pub reparse: bool,
}

/// Upgrade steps, oldest first. Append only; never edit a released step.
const MIGRATIONS: &[Migration] = &[
    // 15: what a cost needs beyond token counts. Cache writes kept for an
    // hour cost more than 5-minute ones, and a fast or flex call is a
    // multiple of a standard one. Both come only from the logs.
    Migration {
        sql: "ALTER TABLE events ADD COLUMN tok_cache_write_1h INTEGER NOT NULL DEFAULT 0;
              ALTER TABLE events ADD COLUMN tier TEXT;",
        reparse: true,
    },
];

/// The schema this build reads and writes.
pub const SCHEMA_VERSION: i64 = BASE_VERSION + MIGRATIONS.len() as i64;

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
    excerpt     TEXT NOT NULL,
    related_seq INTEGER
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

-- MCP tool descriptions and server instructions each session was given.
-- Texts are stored once; a tool whose hash changes over time was rewritten.
CREATE TABLE tool_texts (hash TEXT PRIMARY KEY, text TEXT NOT NULL);
CREATE TABLE session_tools (
    session_id TEXT NOT NULL,
    ts         TEXT,
    server     TEXT NOT NULL,
    tool       TEXT NOT NULL,
    hash       TEXT NOT NULL
);
CREATE INDEX session_tools_session ON session_tools(session_id);
CREATE INDEX session_tools_tool ON session_tools(server, tool);

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
    #[error(
        "this index was made by a newer agentcam (schema {found}, this build reads up to {supported}). \
         Update agentcam, or pass --db to use another index file."
    )]
    NewerSchema { found: i64, supported: i64 },
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
    /// Why the user's `prices.yaml` was left out, if it couldn't be read.
    prices_error: Option<String>,
    machine_id: String,
    os_user: Option<String>,
}

/// The user's own rules: `rules.yaml` next to the index.
pub fn user_prices_path(db: &Path) -> PathBuf {
    db.with_file_name("prices.yaml")
}

pub fn user_rules_path(db: &Path) -> PathBuf {
    db.with_file_name("rules.yaml")
}

/// `~/Library/Application Support/agentcam/index.db` on macOS, the XDG data
/// folder on Linux, `%LOCALAPPDATA%` on Windows.
pub fn default_db_path() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("agentcam").join("index.db"))
}

/// The file behind the main database, if it isn't in memory.
fn path_of(conn: &Connection) -> Option<PathBuf> {
    conn.path().filter(|p| !p.is_empty()).map(PathBuf::from)
}

/// Bring the index at `db` up to the schema `migrations` lead to.
///
/// - Empty: create the schema.
/// - Older than the first release (a development build): drop and rebuild,
///   as before migrations existed.
/// - Older release: copy the file to `index.db.bak-v<N>`, then run each
///   step in its own transaction. A step that needs the logs clears the
///   rules fingerprint, so the next `update` re-reads every log.
/// - Newer than this build: refuse, rather than drop someone's history.
fn upgrade(conn: &Connection, db: Option<&Path>, migrations: &[Migration]) -> Result<()> {
    let target = BASE_VERSION + migrations.len() as i64;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version == target {
        return Ok(());
    }
    if version > target {
        return Err(IndexError::NewerSchema {
            found: version,
            supported: target,
        });
    }
    let has_tables: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%')",
        [],
        |r| r.get(0),
    )?;
    if version < BASE_VERSION {
        if has_tables {
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
        }
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch(&format!("PRAGMA user_version = {BASE_VERSION}"))?;
        if migrations.is_empty() {
            return Ok(());
        }
        // A fresh schema runs the steps too, so new and upgraded indexes
        // end up the same.
        return upgrade(conn, None, migrations);
    }
    if let Some(db) = db {
        let backup = db.with_file_name(format!(
            "{}.bak-v{version}",
            db.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("index.db")
        ));
        let _ = std::fs::remove_file(&backup);
        conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
    }
    let mut reparse = false;
    for (i, step) in migrations
        .iter()
        .enumerate()
        .skip((version - BASE_VERSION) as usize)
    {
        let next = BASE_VERSION + i as i64 + 1;
        conn.execute_batch(&format!(
            "BEGIN; {} PRAGMA user_version = {next}; COMMIT;",
            step.sql
        ))
        .inspect_err(|_| {
            let _ = conn.execute_batch("ROLLBACK");
        })?;
        reparse |= step.reparse;
    }
    if reparse {
        conn.execute("DELETE FROM meta WHERE key = 'rules'", [])?;
    }
    Ok(())
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
        let db = path_of(&conn);
        upgrade(&conn, db.as_deref(), MIGRATIONS)?;
        // The user's own prices sit next to the index. A file that doesn't
        // parse must not stop agentcam: fall back to the built-in list.
        let mut prices_error = None;
        let prices = match db.as_deref().map(user_prices_path) {
            Some(path) => crate::prices::Prices::with_user_file(&path).unwrap_or_else(|e| {
                prices_error = Some(format!("{}: {e}", path.display()));
                crate::prices::Prices::builtin()
            }),
            None => crate::prices::Prices::builtin(),
        };
        Ok(Index {
            conn,
            rules: crate::audit::Rules::builtin(),
            prices,
            prices_error,
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

    pub fn prices(&self) -> &crate::prices::Prices {
        &self.prices
    }

    /// Why the user's `prices.yaml` was left out, if it couldn't be read.
    pub fn prices_error(&self) -> Option<&str> {
        self.prices_error.as_deref()
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
        if full {
            // Sessions whose logs are gone can't be read again, but their
            // token counts are stored: price those with today's list too.
            reprice_kept(&tx, &self.prices, &seen)?;
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
        "session_tools",
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

/// Price again the steps of sessions whose log file is gone, from the token
/// counts in the index, and bring each session's total in line.
fn reprice_kept(
    tx: &rusqlite::Transaction,
    prices: &crate::prices::Prices,
    seen: &std::collections::HashSet<String>,
) -> Result<()> {
    let kept: Vec<String> = {
        let mut stmt = tx.prepare("SELECT id, source_path FROM sessions")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.filter_map(std::result::Result::ok)
            .filter(|(_, path)| !seen.contains(path))
            .map(|(id, _)| id)
            .collect()
    };
    let mut read = tx.prepare(
        "SELECT id, model, tok_in, tok_out, tok_cache_read, tok_cache_write, tok_cache_write_1h, tier
         FROM events WHERE session_id = ?1
           AND tok_in + tok_out + tok_cache_read + tok_cache_write > 0",
    )?;
    let mut write = tx.prepare("UPDATE events SET cost_usd = ?2 WHERE id = ?1")?;
    let mut total =
        tx.prepare("UPDATE sessions SET est_cost_usd = ?2, cost_unpriced = ?3 WHERE id = ?1")?;
    for id in kept {
        let rows: Vec<(i64, crate::model::TokenUsage)> = read
            .query_map([&id], |r| Ok((r.get(0)?, usage_from_row(r, 1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        if rows.is_empty() {
            continue;
        }
        let mut sum = None;
        let mut unpriced = false;
        for (event, u) in rows {
            let cost = prices.cost(&u);
            match cost {
                Some(c) => sum = Some(sum.unwrap_or(0.0) + c),
                None => unpriced = true,
            }
            write.execute(params![event, cost])?;
        }
        total.execute(params![id, sum, unpriced])?;
    }
    Ok(())
}

/// One call's tokens from columns `model, tok_in, tok_out, tok_cache_read,
/// tok_cache_write, tok_cache_write_1h, tier`, starting at column `at`.
pub(crate) fn usage_from_row(
    r: &rusqlite::Row,
    at: usize,
) -> rusqlite::Result<crate::model::TokenUsage> {
    let n = |i: usize| r.get::<_, i64>(at + i).map(|v| v.max(0) as u64);
    Ok(crate::model::TokenUsage {
        model: r.get(at)?,
        input: n(1)?,
        output: n(2)?,
        cache_read: n(3)?,
        cache_write: n(4)?,
        cache_write_1h: n(5)?,
        reasoning: 0,
        tier: r
            .get::<_, Option<String>>(at + 6)?
            .as_deref()
            .map(crate::model::Tier::parse)
            .unwrap_or_default(),
    })
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
            tok_cache_write, context_tokens, cost_usd, wasted, tok_reasoning, approval, via,
            tok_cache_write_1h, tier)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                 ?22, ?23, ?24, ?25, ?26)",
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
            e.usage.as_ref().map_or(0, |u| u.cache_write_1h as i64),
            e.usage
                .as_ref()
                .filter(|u| u.tier != crate::model::Tier::Standard)
                .map(|u| u.tier.as_str()),
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
    let mut tt =
        tx.prepare_cached("INSERT OR IGNORE INTO tool_texts (hash, text) VALUES (?, ?)")?;
    let mut st = tx.prepare_cached(
        "INSERT INTO session_tools (session_id, ts, server, tool, hash) VALUES (?, ?, ?, ?, ?)",
    )?;
    for d in &p.tool_defs {
        let hash = crate::toolcheck::text_hash(&d.text);
        tt.execute(params![hash, d.text])?;
        st.execute(params![
            s.id,
            d.timestamp.as_deref().or(s.started_at.as_deref()),
            d.server,
            d.tool,
            hash
        ])?;
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
        "INSERT INTO findings (session_id, seq, rule_id, title, severity, excerpt, related_seq) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )?;
    for f in findings {
        fi.execute(params![
            s.id,
            f.seq,
            f.rule_id,
            f.title,
            f.severity as i64,
            f.excerpt,
            f.related_seq
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
        "session_tools",
    ] {
        tx.execute(&format!("DELETE FROM {table} WHERE session_id = ?1"), [id])?;
    }
    tx.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("agentcam-migrate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("index.db")
    }

    /// An index as 0.1.0 made it (before any step in `MIGRATIONS`), with
    /// one session whose log is gone.
    fn released_index(db: &Path) {
        let conn = Connection::open(db).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(&format!("PRAGMA user_version = {BASE_VERSION}"))
            .unwrap();
        conn.execute_batch(
                "INSERT INTO sessions (id, agent, source_path) VALUES ('kept', 'claude-code', '/Users/dev/gone.jsonl');
                 INSERT INTO meta (key, value) VALUES ('rules', 'old');",
            )
            .unwrap();
    }

    #[test]
    fn upgrade_keeps_sessions_whose_logs_are_gone() {
        let db = temp_db("keep");
        released_index(&db);
        let steps = [Migration {
            sql: "ALTER TABLE sessions ADD COLUMN note TEXT; UPDATE sessions SET note = 'upgraded';",
            reparse: true,
        }];
        let conn = Connection::open(&db).unwrap();
        upgrade(&conn, Some(&db), &steps).unwrap();
        let note: String = conn
            .query_row("SELECT note FROM sessions WHERE id = 'kept'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(note, "upgraded");
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, BASE_VERSION + 1);
        // The step needed the logs, so the next run re-reads them all.
        let rules: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'rules'", [], |r| {
                r.get(0)
            })
            .optional()
            .unwrap();
        assert_eq!(rules, None);
        // The old file was copied first, and still opens at the old version.
        let backup = db.with_file_name(format!("index.db.bak-v{BASE_VERSION}"));
        let old = Connection::open(&backup).unwrap();
        let v: i64 = old
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, BASE_VERSION);
        // Running again does nothing.
        upgrade(&conn, Some(&db), &steps).unwrap();
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn a_released_index_gains_the_cost_columns_and_keeps_its_sessions() {
        let db = temp_db("real");
        released_index(&db);
        let idx = Index::open(&db).unwrap();
        let v: i64 = idx
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let cols: i64 = idx
            .conn()
            .query_row(
                "SELECT count(*) FROM pragma_table_info('events')
                 WHERE name IN ('tok_cache_write_1h', 'tier')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cols, 2);
        let kept: i64 = idx
            .conn()
            .query_row("SELECT count(*) FROM sessions WHERE id = 'kept'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kept, 1);
        drop(idx);
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn a_failed_step_leaves_the_index_as_it_was() {
        let db = temp_db("fail");
        released_index(&db);
        let steps = [Migration {
            sql: "ALTER TABLE sessions ADD COLUMN x TEXT; SELECT * FROM no_such_table;",
            reparse: false,
        }];
        let conn = Connection::open(&db).unwrap();
        assert!(upgrade(&conn, Some(&db), &steps).is_err());
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, BASE_VERSION);
        let cols: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_table_info('sessions') WHERE name = 'x'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cols, 0, "the half-run step was rolled back");
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn a_newer_index_is_refused_not_wiped() {
        let db = temp_db("newer");
        released_index(&db);
        Connection::open(&db)
            .unwrap()
            .execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION + 3))
            .unwrap();
        let err = Index::open(&db).err().expect("refused");
        assert!(matches!(err, IndexError::NewerSchema { .. }), "{err}");
        let n: i64 = Connection::open(&db)
            .unwrap()
            .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn a_development_index_is_rebuilt() {
        let db = temp_db("dev");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE sessions (id TEXT); PRAGMA user_version = 9;")
            .unwrap();
        drop(conn);
        let idx = Index::open(&db).unwrap();
        let v: i64 = idx
            .conn()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        let n: i64 = idx
            .conn()
            .query_row(
                "SELECT count(*) FROM pragma_table_info('sessions')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(n > 1, "the current sessions table replaced the old one");
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }
}
