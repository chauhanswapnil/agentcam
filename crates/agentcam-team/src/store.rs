//! The team server's storage: teams, invites, devices, viewers, and the
//! session metadata laptops send.
//!
//! SQLite today. Every statement is plain SQL that Postgres also runs
//! (`INSERT ... ON CONFLICT ... DO UPDATE`, no SQLite-only functions in
//! queries), and the schema only moves forward through numbered
//! migrations, so a Postgres store can be a second implementation.

use std::collections::HashSet;
use std::path::Path;

use agentcam_core::fleet::Inventory;
use agentcam_core::share::SessionMeta;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::{Error, Result, hash_token, new_token, now};

/// Numbered migrations. Never edit one that has shipped; add the next.
const MIGRATIONS: &[&str] = &[
    // 1: the first schema.
    "
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    CREATE TABLE teams (
        id         INTEGER PRIMARY KEY,
        name       TEXT NOT NULL UNIQUE,
        created_at TEXT NOT NULL
    );
    CREATE TABLE invites (
        code_hash  TEXT PRIMARY KEY,
        team_id    INTEGER NOT NULL REFERENCES teams(id),
        created_at TEXT NOT NULL,
        expires_at TEXT NOT NULL,
        uses_left  INTEGER NOT NULL
    );
    CREATE TABLE devices (
        machine_id       TEXT PRIMARY KEY,
        token_hash       TEXT NOT NULL UNIQUE,
        team_id          INTEGER NOT NULL REFERENCES teams(id),
        os_user          TEXT,
        joined_at        TEXT NOT NULL,
        last_sync        TEXT,
        agentcam_version TEXT,
        os               TEXT
    );
    CREATE TABLE viewers (
        token_hash TEXT PRIMARY KEY,
        name       TEXT NOT NULL,
        role       TEXT NOT NULL,
        created_at TEXT NOT NULL,
        last_seen  TEXT
    );
    CREATE TABLE sessions (
        machine_id    TEXT NOT NULL,
        session_id    TEXT NOT NULL,
        version       TEXT NOT NULL,
        team_id       INTEGER NOT NULL,
        parent_id     TEXT,
        agent         TEXT NOT NULL,
        agent_version TEXT,
        model         TEXT,
        project       TEXT,
        branch        TEXT,
        started_at    TEXT,
        ended_at      TEXT,
        active_ms     INTEGER NOT NULL,
        retry_ms      INTEGER NOT NULL,
        steps         INTEGER NOT NULL,
        tool_calls    INTEGER NOT NULL,
        errored_steps INTEGER NOT NULL,
        turns         INTEGER NOT NULL,
        tok_input     INTEGER NOT NULL,
        tok_output    INTEGER NOT NULL,
        tok_cache_read  INTEGER NOT NULL,
        tok_cache_write INTEGER NOT NULL,
        est_cost_usd  REAL,
        peak_context  INTEGER NOT NULL,
        lasted_added    INTEGER,
        lasted_kept     INTEGER,
        lasted_reworked INTEGER,
        lasted_changed  INTEGER,
        follow_ok     INTEGER,
        follow_missed INTEGER,
        commits       INTEGER NOT NULL,
        prs           INTEGER NOT NULL,
        files         INTEGER NOT NULL,
        received_at   TEXT NOT NULL,
        PRIMARY KEY (machine_id, session_id)
    );
    CREATE INDEX sessions_ended ON sessions(ended_at);
    CREATE TABLE findings (
        machine_id TEXT NOT NULL,
        session_id TEXT NOT NULL,
        rule_id    TEXT NOT NULL,
        severity   TEXT NOT NULL,
        step       INTEGER NOT NULL,
        excerpt    TEXT NOT NULL
    );
    CREATE INDEX findings_session ON findings(machine_id, session_id);
    CREATE TABLE inventory (
        machine_id TEXT NOT NULL,
        kind       TEXT NOT NULL,
        agent      TEXT NOT NULL,
        name       TEXT NOT NULL,
        detail     TEXT,
        scope      TEXT NOT NULL
    );
    CREATE INDEX inventory_machine ON inventory(machine_id);
    CREATE TABLE agent_versions (
        machine_id TEXT NOT NULL,
        agent      TEXT NOT NULL,
        version    TEXT,
        sessions   INTEGER NOT NULL,
        last_used  TEXT
    );
    CREATE TABLE audit_log (
        id     INTEGER PRIMARY KEY,
        ts     TEXT NOT NULL,
        actor  TEXT NOT NULL,
        action TEXT NOT NULL,
        detail TEXT
    );
    ",
    // 2: tags on inventory items, and when each fact about a machine's setup
    // was first seen, for baseline drift.
    "
    ALTER TABLE inventory ADD COLUMN tags TEXT;
    CREATE TABLE machine_facts (
        machine_id TEXT NOT NULL,
        fact       TEXT NOT NULL,
        first_seen TEXT NOT NULL,
        PRIMARY KEY (machine_id, fact)
    );
    CREATE TABLE fact_changes (
        id         INTEGER PRIMARY KEY,
        ts         TEXT NOT NULL,
        machine_id TEXT NOT NULL,
        fact       TEXT NOT NULL,
        change     TEXT NOT NULL
    );
    CREATE INDEX fact_changes_machine ON fact_changes(machine_id, ts);
    ",
];

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone, Serialize)]
pub struct Team {
    pub id: i64,
    pub name: String,
    pub machines: i64,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub machine_id: String,
    pub team_id: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Viewer {
    pub name: String,
    /// `admin` or `viewer`.
    pub role: String,
}

/// A joined machine, for `agentcam team machines`.
#[derive(Debug, Clone, Serialize)]
pub struct Machine {
    pub machine_id: String,
    pub team: String,
    pub os_user: Option<String>,
    pub joined_at: String,
    pub last_sync: Option<String>,
}

/// A viewer token's record (never the token), for `agentcam team viewers`.
#[derive(Debug, Clone, Serialize)]
pub struct ViewerRow {
    pub id: i64,
    pub name: String,
    pub role: String,
    pub created_at: String,
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub ts: String,
    pub actor: String,
    pub action: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Joined {
    pub device_token: String,
    pub team: String,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
            [],
        )?;
        let mut s = Store { conn };
        s.migrate()?;
        Ok(s)
    }

    fn migrate(&mut self) -> Result<()> {
        let done: i64 = self.conn.query_row(
            "SELECT coalesce(max(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?;
        for (i, sql) in MIGRATIONS.iter().enumerate() {
            let v = i as i64 + 1;
            if v <= done {
                continue;
            }
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                params![v, now()],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn log(&self, actor: &str, action: &str, detail: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit_log (ts, actor, action, detail) VALUES (?1, ?2, ?3, ?4)",
            params![now(), actor, action, detail],
        )?;
        Ok(())
    }

    pub fn audit_log(&self, limit: u32) -> Result<Vec<LogEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT ts, actor, action, detail FROM audit_log ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt
            .query_map([limit], |r| {
                Ok(LogEntry {
                    ts: r.get(0)?,
                    actor: r.get(1)?,
                    action: r.get(2)?,
                    detail: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// A team by name, created if new.
    pub fn team(&self, name: &str) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(Error::Bad("team names are 1 to 80 characters".into()));
        }
        self.conn.execute(
            "INSERT INTO teams (name, created_at) VALUES (?1, ?2) ON CONFLICT (name) DO NOTHING",
            params![name, now()],
        )?;
        Ok(self
            .conn
            .query_row("SELECT id FROM teams WHERE name = ?1", [name], |r| r.get(0))?)
    }

    pub fn teams(&self) -> Result<Vec<Team>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, (SELECT count(*) FROM devices d WHERE d.team_id = t.id)
             FROM teams t ORDER BY t.name",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Team {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    machines: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// A new invite code for `team`. Only its hash is stored, so the code
    /// is shown once.
    pub fn create_invite(&self, team_id: i64, uses: i64, days: i64) -> Result<String> {
        let code = new_token("inv");
        let expires = agentcam_core::util::format_ts_ms(
            agentcam_core::util::now_ms() + days.clamp(1, 365) * 86_400_000,
        );
        self.conn.execute(
            "INSERT INTO invites (code_hash, team_id, created_at, expires_at, uses_left)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                hash_token(&code),
                team_id,
                now(),
                expires,
                uses.clamp(1, 10_000)
            ],
        )?;
        Ok(code)
    }

    /// Trade an invite for a device token. Joining again from the same
    /// machine replaces its old token.
    pub fn redeem_invite(
        &mut self,
        code: &str,
        machine_id: &str,
        os_user: Option<&str>,
    ) -> Result<Joined> {
        if machine_id.len() < 16 || machine_id.len() > 64 {
            return Err(Error::Bad("bad machine id".into()));
        }
        let tx = self.conn.transaction()?;
        let row: Option<(i64, String, i64)> = tx
            .query_row(
                "SELECT team_id, expires_at, uses_left FROM invites WHERE code_hash = ?1",
                [hash_token(code)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((team_id, expires, uses)) = row else {
            return Err(Error::Denied("unknown invite".into()));
        };
        if uses <= 0 || expires < now() {
            return Err(Error::Denied("this invite has expired".into()));
        }
        tx.execute(
            "UPDATE invites SET uses_left = uses_left - 1 WHERE code_hash = ?1",
            [hash_token(code)],
        )?;
        let token = new_token("dev");
        tx.execute(
            "INSERT INTO devices (machine_id, token_hash, team_id, os_user, joined_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (machine_id) DO UPDATE SET token_hash = excluded.token_hash,
                team_id = excluded.team_id, os_user = excluded.os_user",
            params![machine_id, hash_token(&token), team_id, os_user, now()],
        )?;
        let team: String =
            tx.query_row("SELECT name FROM teams WHERE id = ?1", [team_id], |r| {
                r.get(0)
            })?;
        tx.execute(
            "INSERT INTO audit_log (ts, actor, action, detail) VALUES (?1, 'device', 'joined', ?2)",
            params![now(), format!("{} joined {team}", short(machine_id))],
        )?;
        tx.commit()?;
        Ok(Joined {
            device_token: token,
            team,
        })
    }

    pub fn device(&self, token: &str) -> Result<Option<Device>> {
        Ok(self
            .conn
            .query_row(
                "SELECT machine_id, team_id FROM devices WHERE token_hash = ?1",
                [hash_token(token)],
                |r| {
                    Ok(Device {
                        machine_id: r.get(0)?,
                        team_id: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    /// Remove a machine and everything it sent.
    pub fn remove_device(&mut self, machine_id: &str) -> Result<bool> {
        let tx = self.conn.transaction()?;
        let n = tx.execute("DELETE FROM devices WHERE machine_id = ?1", [machine_id])?;
        for t in [
            "sessions",
            "findings",
            "inventory",
            "agent_versions",
            "machine_facts",
            "fact_changes",
        ] {
            tx.execute(
                &format!("DELETE FROM {t} WHERE machine_id = ?1"),
                [machine_id],
            )?;
        }
        tx.commit()?;
        Ok(n > 0)
    }

    pub fn machines(&self) -> Result<Vec<Machine>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.machine_id, t.name, d.os_user, d.joined_at, d.last_sync
             FROM devices d JOIN teams t ON t.id = d.team_id ORDER BY t.name, d.joined_at",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Machine {
                    machine_id: r.get(0)?,
                    team: r.get(1)?,
                    os_user: r.get(2)?,
                    joined_at: r.get(3)?,
                    last_sync: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// The one machine id starting with `prefix` (at least 6 characters,
    /// as `Machine abc123` shows it).
    pub fn find_machine(&self, prefix: &str) -> Result<String> {
        if prefix.len() < 6 {
            return Err(Error::Bad(
                "give at least 6 characters of the machine id".into(),
            ));
        }
        let ids: Vec<String> = self
            .machines()?
            .into_iter()
            .map(|m| m.machine_id)
            .filter(|id| id.starts_with(prefix))
            .collect();
        match ids.as_slice() {
            [one] => Ok(one.clone()),
            [] => Err(Error::Bad(format!("no machine starts with {prefix}"))),
            _ => Err(Error::Bad(format!(
                "more than one machine starts with {prefix}"
            ))),
        }
    }

    pub fn viewers(&self) -> Result<Vec<ViewerRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT rowid, name, role, created_at, last_seen FROM viewers ORDER BY rowid",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ViewerRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    role: r.get(2)?,
                    created_at: r.get(3)?,
                    last_seen: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// Revoke a viewer token by its id from [`Store::viewers`]. The last
    /// admin can't be revoked, so the UI never locks everyone out.
    pub fn revoke_viewer(&self, id: i64) -> Result<ViewerRow> {
        let all = self.viewers()?;
        let Some(v) = all.iter().find(|v| v.id == id).cloned() else {
            return Err(Error::Bad(format!("no viewer with id {id}")));
        };
        if v.role == "admin" && all.iter().filter(|x| x.role == "admin").count() == 1 {
            return Err(Error::Bad(
                "that's the last admin; make another admin token first".into(),
            ));
        }
        self.conn
            .execute("DELETE FROM viewers WHERE rowid = ?1", [id])?;
        Ok(v)
    }

    /// A new token for the team UI.
    pub fn create_viewer(&self, name: &str, role: &str) -> Result<String> {
        if !matches!(role, "admin" | "viewer") {
            return Err(Error::Bad("role is admin or viewer".into()));
        }
        let token = new_token("view");
        self.conn.execute(
            "INSERT INTO viewers (token_hash, name, role, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![hash_token(&token), name, role, now()],
        )?;
        Ok(token)
    }

    pub fn viewer(&self, token: &str) -> Result<Option<Viewer>> {
        let h = hash_token(token);
        let v = self
            .conn
            .query_row(
                "SELECT name, role FROM viewers WHERE token_hash = ?1",
                [&h],
                |r| {
                    Ok(Viewer {
                        name: r.get(0)?,
                        role: r.get(1)?,
                    })
                },
            )
            .optional()?;
        if v.is_some() {
            self.conn.execute(
                "UPDATE viewers SET last_seen = ?1 WHERE token_hash = ?2",
                params![now(), h],
            )?;
        }
        Ok(v)
    }

    pub fn viewer_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM viewers", [], |r| r.get(0))?)
    }

    /// Store what a laptop sent. Sessions whose version is unchanged are
    /// skipped; changed ones replace the old rows. Returns the ids of the
    /// sessions written.
    pub fn ingest(
        &mut self,
        device: &Device,
        agentcam_version: Option<&str>,
        sessions: &[SessionMeta],
        inventory: Option<&Inventory>,
    ) -> Result<Vec<String>> {
        let tx = self.conn.transaction()?;
        let mut written = Vec::new();
        for m in sessions {
            // A laptop only speaks for its own sessions.
            if m.machine_id != device.machine_id {
                return Err(Error::Bad("session from another machine".into()));
            }
            let old: Option<String> = tx
                .query_row(
                    "SELECT version FROM sessions WHERE machine_id = ?1 AND session_id = ?2",
                    params![device.machine_id, m.session_id],
                    |r| r.get(0),
                )
                .optional()?;
            if old.as_deref() == Some(m.version.as_str()) {
                continue;
            }
            tx.execute(
                "DELETE FROM findings WHERE machine_id = ?1 AND session_id = ?2",
                params![device.machine_id, m.session_id],
            )?;
            let steps: i64 = m.steps.values().sum();
            let calls: i64 = m.tool_calls.values().sum();
            let l = m.lasted.unwrap_or_default();
            let has_l = m.lasted.is_some();
            let project = m.project_remote.clone().or(m.project_name.clone());
            tx.execute(
                "INSERT INTO sessions (machine_id, session_id, version, team_id, parent_id, agent,
                    agent_version, model, project, branch, started_at, ended_at, active_ms, retry_ms,
                    steps, tool_calls, errored_steps, turns, tok_input, tok_output, tok_cache_read,
                    tok_cache_write, est_cost_usd, peak_context, lasted_added, lasted_kept,
                    lasted_reworked, lasted_changed, follow_ok, follow_missed, commits, prs, files,
                    received_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                    ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34)
                 ON CONFLICT (machine_id, session_id) DO UPDATE SET
                    version = excluded.version, team_id = excluded.team_id,
                    parent_id = excluded.parent_id, agent = excluded.agent,
                    agent_version = excluded.agent_version, model = excluded.model,
                    project = excluded.project, branch = excluded.branch,
                    started_at = excluded.started_at, ended_at = excluded.ended_at,
                    active_ms = excluded.active_ms, retry_ms = excluded.retry_ms,
                    steps = excluded.steps, tool_calls = excluded.tool_calls,
                    errored_steps = excluded.errored_steps, turns = excluded.turns,
                    tok_input = excluded.tok_input, tok_output = excluded.tok_output,
                    tok_cache_read = excluded.tok_cache_read, tok_cache_write = excluded.tok_cache_write,
                    est_cost_usd = excluded.est_cost_usd, peak_context = excluded.peak_context,
                    lasted_added = excluded.lasted_added, lasted_kept = excluded.lasted_kept,
                    lasted_reworked = excluded.lasted_reworked, lasted_changed = excluded.lasted_changed,
                    follow_ok = excluded.follow_ok, follow_missed = excluded.follow_missed,
                    commits = excluded.commits, prs = excluded.prs, files = excluded.files,
                    received_at = excluded.received_at",
                params![
                    device.machine_id,
                    m.session_id,
                    m.version,
                    device.team_id,
                    m.parent_id,
                    m.agent,
                    m.agent_version,
                    m.model,
                    project,
                    m.branch,
                    m.started_at,
                    m.ended_at,
                    m.active_ms,
                    m.retry_ms,
                    steps,
                    calls,
                    m.errored_steps,
                    m.turns,
                    m.tokens.fresh_input,
                    m.tokens.output,
                    m.tokens.cache_read,
                    m.tokens.cache_write,
                    m.est_cost_usd,
                    m.peak_context,
                    has_l.then_some(l.added),
                    has_l.then_some(l.kept),
                    has_l.then_some(l.reworked),
                    has_l.then_some(l.changed),
                    m.follow.map(|f| f.followed),
                    m.follow.map(|f| f.not_followed),
                    m.commits.len() as i64,
                    m.prs.len() as i64,
                    m.files.len() as i64,
                    now(),
                ],
            )?;
            for f in &m.findings {
                tx.execute(
                    "INSERT INTO findings (machine_id, session_id, rule_id, severity, step, excerpt)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        device.machine_id,
                        m.session_id,
                        f.rule_id,
                        f.severity,
                        f.step,
                        f.excerpt
                    ],
                )?;
            }
            written.push(m.session_id.clone());
        }
        if let Some(inv) = inventory {
            tx.execute(
                "DELETE FROM inventory WHERE machine_id = ?1",
                [&device.machine_id],
            )?;
            tx.execute(
                "DELETE FROM agent_versions WHERE machine_id = ?1",
                [&device.machine_id],
            )?;
            for i in &inv.items {
                tx.execute(
                    "INSERT INTO inventory (machine_id, kind, agent, name, detail, scope, tags)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        device.machine_id,
                        i.kind,
                        i.agent,
                        i.name,
                        i.detail,
                        i.scope,
                        (!i.tags.is_empty()).then(|| i.tags.join(","))
                    ],
                )?;
            }
            record_facts(&tx, &device.machine_id, &facts(inv))?;
            for f in &inv.flags {
                tx.execute(
                    "INSERT INTO inventory (machine_id, kind, agent, name, detail, scope)
                     VALUES (?1, 'flag', ?2, ?3, NULL, ?4)",
                    params![device.machine_id, f.agent, f.id, f.scope],
                )?;
            }
            for a in &inv.agents {
                tx.execute(
                    "INSERT INTO agent_versions (machine_id, agent, version, sessions, last_used)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        device.machine_id,
                        a.agent,
                        a.version,
                        a.sessions,
                        a.last_used
                    ],
                )?;
            }
            tx.execute(
                "UPDATE devices SET os = ?1 WHERE machine_id = ?2",
                params![format!("{} {}", inv.os, inv.arch), device.machine_id],
            )?;
        }
        tx.execute(
            "UPDATE devices SET last_sync = ?1, agentcam_version = coalesce(?2, agentcam_version)
             WHERE machine_id = ?3",
            params![now(), agentcam_version, device.machine_id],
        )?;
        tx.commit()?;
        Ok(written)
    }
}

/// The first characters of a machine id, for labels.
/// What a machine's setup says, as plain strings that compare across syncs:
/// `flag:<agent>:<id>`, `item:<kind>:<agent>:<name>`,
/// `tag:<kind>:<agent>:<name>:<tag>`, `version:<agent>:<version>`.
pub fn facts(inv: &Inventory) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in &inv.flags {
        out.push(format!("flag:{}:{}", f.agent, f.id));
    }
    for i in &inv.items {
        out.push(format!("item:{}:{}:{}", i.kind, i.agent, i.name));
        for t in &i.tags {
            out.push(format!("tag:{}:{}:{}:{t}", i.kind, i.agent, i.name));
        }
    }
    for a in &inv.agents {
        if let Some(v) = &a.version {
            out.push(format!("version:{}:{v}", a.agent));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Keep `machine_facts` in step with `now`, logging what was added or
/// removed. The first sync only sets the starting point.
fn record_facts(tx: &rusqlite::Transaction, machine: &str, now_facts: &[String]) -> Result<()> {
    let mut stmt = tx.prepare("SELECT fact FROM machine_facts WHERE machine_id = ?1")?;
    let old: HashSet<String> = stmt
        .query_map([machine], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    drop(stmt);
    let first = old.is_empty();
    let ts = now();
    let new: HashSet<&String> = now_facts.iter().collect();
    for f in now_facts.iter().filter(|f| !old.contains(*f)) {
        tx.execute(
            "INSERT INTO machine_facts (machine_id, fact, first_seen) VALUES (?1, ?2, ?3)
             ON CONFLICT (machine_id, fact) DO NOTHING",
            params![machine, f, ts],
        )?;
        if !first {
            tx.execute(
                "INSERT INTO fact_changes (ts, machine_id, fact, change) VALUES (?1, ?2, ?3, 'added')",
                params![ts, machine, f],
            )?;
        }
    }
    for f in old.iter().filter(|f| !new.contains(f)) {
        tx.execute(
            "DELETE FROM machine_facts WHERE machine_id = ?1 AND fact = ?2",
            params![machine, f],
        )?;
        tx.execute(
            "INSERT INTO fact_changes (ts, machine_id, fact, change) VALUES (?1, ?2, ?3, 'removed')",
            params![ts, machine, f],
        )?;
    }
    Ok(())
}

pub fn short(machine_id: &str) -> String {
    machine_id.chars().take(6).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_machines_and_revoke_viewers() {
        let mut s = Store::open_in_memory().unwrap();
        let team = s.team("Platform").unwrap();
        let invite = s.create_invite(team, 5, 7).unwrap();
        let j = s
            .redeem_invite(&invite, "abcdef0123456789abcdef", Some("dev"))
            .unwrap();
        s.redeem_invite(&invite, "abcd99aa23456789abcdef", None)
            .unwrap();
        assert!(s.find_machine("abcd").is_err(), "too short");
        assert!(s.find_machine("abcdef").is_ok());
        let id = s.find_machine("abcdef").unwrap();
        assert!(s.remove_device(&id).unwrap());
        assert!(
            s.device(&j.device_token).unwrap().is_none(),
            "token stops working"
        );
        assert_eq!(s.machines().unwrap().len(), 1);

        let admin = s.create_viewer("Admin", "admin").unwrap();
        s.create_viewer("Finance", "viewer").unwrap();
        let rows = s.viewers().unwrap();
        let first = rows.iter().find(|v| v.role == "admin").unwrap().id;
        assert!(s.revoke_viewer(first).is_err(), "never the last admin");
        let fin = rows.iter().find(|v| v.name == "Finance").unwrap().id;
        s.revoke_viewer(fin).unwrap();
        assert_eq!(s.viewers().unwrap().len(), 1);
        assert!(s.viewer(&admin).unwrap().is_some());
    }
}
