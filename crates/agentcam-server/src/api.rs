//! JSON API over the index.

use std::path::Path;
use std::sync::{Arc, Mutex};

use agentcam_core::index::UpdateReport;
use agentcam_core::query::{self, Filter, SessionDetail, SessionPage, Stats};
use agentcam_core::util::{format_ts_ms, now_ms};
use agentcam_core::{Index, LogSource};
use axum::{
    Json,
    extract::{Path as UrlPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

/// Shared server state. Reads and writes use separate SQLite connections,
/// so the UI stays responsive while a re-index runs.
pub struct AppState {
    /// Where `guard.yaml` lives; `None` turns the guard API off.
    guard_path: Option<std::path::PathBuf>,
    /// Where the user's `rules.yaml` lives.
    rules_path: Option<std::path::PathBuf>,
    db_path: Option<std::path::PathBuf>,
    writer: Arc<Mutex<Index>>,
    reader: Arc<Mutex<Index>>,
    sources: Vec<LogSource>,
    status: Mutex<IndexStatus>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IndexStatus {
    pub indexing: bool,
    pub last_report: Option<UpdateReport>,
    pub last_error: Option<String>,
    /// Bumped after every finished index run, so the UI knows to refetch.
    pub generation: u64,
}

impl AppState {
    /// Open the index at `db` for reading and writing.
    pub fn open(
        db: &Path,
        sources: Vec<LogSource>,
        rules: agentcam_core::audit::Rules,
    ) -> agentcam_core::index::Result<Arc<Self>> {
        let mut writer = Index::open(db)?;
        writer.set_rules(rules);
        let reader = Index::open(db)?;
        Ok(Arc::new(AppState {
            guard_path: Some(agentcam_core::guard::guard_path(db)),
            rules_path: Some(agentcam_core::index::user_rules_path(db)),
            db_path: Some(db.to_path_buf()),
            writer: Arc::new(Mutex::new(writer)),
            reader: Arc::new(Mutex::new(reader)),
            sources,
            status: Mutex::default(),
        }))
    }

    /// One shared connection for both, for tests and in-memory indexes.
    pub fn from_index(
        index: Index,
        sources: Vec<LogSource>,
        guard_path: Option<std::path::PathBuf>,
    ) -> Arc<Self> {
        let index = Arc::new(Mutex::new(index));
        Arc::new(AppState {
            rules_path: guard_path.as_ref().map(|g| g.with_file_name("rules.yaml")),
            guard_path,
            db_path: None,
            writer: index.clone(),
            reader: index,
            sources,
            status: Mutex::default(),
        })
    }

    pub fn sources(&self) -> &[LogSource] {
        &self.sources
    }

    /// Re-index whenever an agent writes to its logs. Returns false if the
    /// folders couldn't be watched (the UI's re-index button still works).
    pub fn watch(self: &Arc<Self>) -> bool {
        crate::watch::start(self.clone())
    }

    pub fn status(&self) -> IndexStatus {
        self.status.lock().unwrap().clone()
    }

    /// Start indexing in the background unless a run is already going.
    /// Returns false if one was.
    pub fn spawn_index(self: &Arc<Self>, full: bool) -> bool {
        {
            let mut st = self.status.lock().unwrap();
            if st.indexing {
                return false;
            }
            st.indexing = true;
        }
        let me = self.clone();
        tokio::task::spawn_blocking(move || {
            let result = me.writer.lock().unwrap().update(&me.sources, full);
            let mut st = me.status.lock().unwrap();
            st.indexing = false;
            st.generation += 1;
            match result {
                Ok(r) => {
                    tracing::info!(
                        "indexed {} of {} files in {}ms",
                        r.files_parsed,
                        r.files_seen,
                        r.elapsed_ms
                    );
                    st.last_report = Some(r);
                    st.last_error = None;
                }
                Err(e) => {
                    tracing::error!("indexing failed: {e}");
                    st.last_error = Some(e.to_string());
                }
            }
        });
        true
    }

    /// Run `f` against the read connection off the async runtime.
    async fn read<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&Index) -> agentcam_core::index::Result<T> + Send + 'static,
    ) -> Result<T, ApiError> {
        let me = self.clone();
        tokio::task::spawn_blocking(move || f(&me.reader.lock().unwrap()))
            .await
            .map_err(|e| ApiError(e.to_string()))?
            .map_err(|e| ApiError(e.to_string()))
    }
}

pub struct ApiError(String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::error!("api error: {}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": self.0 })),
        )
            .into_response()
    }
}

type Shared = State<Arc<AppState>>;

/// Query string shared by the list endpoints.
#[derive(Debug, Default, Deserialize)]
pub struct Params {
    /// `7d`, `30d`, `90d`, `365d` or `all` (default).
    range: Option<String>,
    agent: Option<String>,
    project: Option<String>,
    day: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    /// Audit: one rule id.
    rule: Option<String>,
    /// Audit: lowest severity to include.
    severity: Option<String>,
    /// Tool calls: one tool name.
    tool: Option<String>,
    /// File history: one path.
    path: Option<String>,
}

const DAY_MS: i64 = 86_400_000;

fn range_days(range: Option<&str>) -> Option<i64> {
    range?.strip_suffix('d')?.parse().ok().filter(|d| *d > 0)
}

impl Params {
    fn filter(&self, now: i64) -> Filter {
        let since = range_days(self.range.as_deref()).map(|d| format_ts_ms(now - d * DAY_MS));
        Filter {
            since,
            until: None,
            agent: self.agent.clone(),
            project: self.project.clone(),
            day: self.day.clone(),
        }
    }

    /// The same-length period just before this one, for comparisons.
    fn previous(&self, now: i64) -> Option<Filter> {
        let d = range_days(self.range.as_deref())?;
        Some(Filter {
            since: Some(format_ts_ms(now - 2 * d * DAY_MS)),
            until: Some(format_ts_ms(now - d * DAY_MS)),
            ..self.filter(now)
        })
    }
}

pub async fn status(State(st): Shared) -> Json<IndexStatus> {
    Json(st.status())
}

pub async fn reindex(State(st): Shared) -> StatusCode {
    if st.spawn_index(false) {
        StatusCode::ACCEPTED
    } else {
        StatusCode::CONFLICT
    }
}

#[derive(Serialize)]
pub struct Overview {
    since: Option<String>,
    stats: Stats,
    previous: Option<Stats>,
    projects: Vec<query::Project>,
}

pub async fn overview(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Overview>, ApiError> {
    let now = now_ms();
    let f = p.filter(now);
    let prev = p.previous(now);
    let weeks = match range_days(p.range.as_deref()) {
        Some(d) if d <= 30 => 8,
        _ => 12,
    };
    st.read(move |idx| {
        let c = idx.conn();
        Ok(Overview {
            since: f.since.clone(),
            stats: query::stats(c, &f)?,
            previous: prev.map(|pf| query::stats(c, &pf)).transpose()?,
            projects: query::projects(c, &f, weeks, &format_ts_ms(now))?,
        })
    })
    .await
    .map(Json)
}

/// Daily activity for the last year (371 days fills a 53-week grid).
pub async fn activity(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<query::Day>>, ApiError> {
    let f = Filter {
        since: Some(format_ts_ms(now_ms() - 371 * DAY_MS)),
        agent: p.agent,
        project: p.project,
        ..Default::default()
    };
    st.read(move |idx| query::activity(idx.conn(), &f))
        .await
        .map(Json)
}

pub async fn sessions(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<SessionPage>, ApiError> {
    let f = p.filter(now_ms());
    let limit = p.limit.unwrap_or(50).min(200);
    let offset = p.offset.unwrap_or(0);
    st.read(move |idx| query::sessions(idx.conn(), &f, limit, offset))
        .await
        .map(Json)
}

#[derive(Deserialize)]
pub struct SearchParams {
    q: String,
    range: Option<String>,
    agent: Option<String>,
    project: Option<String>,
    kind: Option<String>,
    category: Option<String>,
    limit: Option<u32>,
}

pub async fn search(
    State(st): Shared,
    Query(p): Query<SearchParams>,
) -> Result<Json<Vec<query::Hit>>, ApiError> {
    let f = Params {
        range: p.range,
        agent: p.agent,
        project: p.project,
        ..Default::default()
    }
    .filter(now_ms());
    let limit = p.limit.unwrap_or(50).min(200);
    st.read(move |idx| {
        query::search(
            idx.conn(),
            &p.q,
            &f,
            p.kind.as_deref(),
            p.category.as_deref(),
            limit,
        )
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct StepParams {
    session: String,
    seq: i64,
}

pub async fn step(State(st): Shared, Query(p): Query<StepParams>) -> Result<Response, ApiError> {
    let found = st
        .read(move |idx| query::step(idx.conn(), &p.session, p.seq))
        .await?;
    Ok(match found {
        Some(s) => Json(s).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

pub async fn session(
    State(st): Shared,
    UrlPath(id): UrlPath<String>,
) -> Result<Response, ApiError> {
    let found: Option<SessionDetail> = st.read(move |idx| query::session(idx.conn(), &id)).await?;
    Ok(match found {
        Some(d) => Json(d).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(range_days(Some("30d")), Some(30));
        assert_eq!(range_days(Some("all")), None);
        assert_eq!(range_days(Some("0d")), None);
        assert_eq!(range_days(None), None);
    }
}

#[derive(Serialize)]
pub struct Audit {
    rules: Vec<query::RuleSummary>,
    findings: Vec<query::FindingHit>,
}

pub async fn audit(State(st): Shared, Query(p): Query<Params>) -> Result<Json<Audit>, ApiError> {
    let f = p.filter(now_ms());
    let limit = p.limit.unwrap_or(200).min(500);
    st.read(move |idx| {
        let c = idx.conn();
        Ok(Audit {
            rules: query::audit_rules(c, &f)?,
            findings: query::findings(c, &f, p.rule.as_deref(), p.severity.as_deref(), limit)?,
        })
    })
    .await
    .map(Json)
}

pub async fn tools(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<query::ToolStat>>, ApiError> {
    let f = p.filter(now_ms());
    st.read(move |idx| query::tools(idx.conn(), &f))
        .await
        .map(Json)
}

pub async fn tool_calls(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<query::StepRef>>, ApiError> {
    let f = p.filter(now_ms());
    let tool = p.tool.clone().unwrap_or_default();
    let limit = p.limit.unwrap_or(100).min(500);
    st.read(move |idx| query::tool_calls(idx.conn(), &f, &tool, limit))
        .await
        .map(Json)
}

pub async fn files(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<query::FileStat>>, ApiError> {
    let f = p.filter(now_ms());
    let limit = p.limit.unwrap_or(500).min(2000);
    st.read(move |idx| query::files(idx.conn(), &f, limit))
        .await
        .map(Json)
}

pub async fn file_history(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<query::StepRef>>, ApiError> {
    let path = p.path.clone().unwrap_or_default();
    let limit = p.limit.unwrap_or(200).min(1000);
    st.read(move |idx| query::file_history(idx.conn(), &path, limit))
        .await
        .map(Json)
}

// ── Guard ("block next time") ─────────────────────────────────────────

use agentcam_core::guard::{self, GuardRule};

#[derive(Serialize)]
pub struct HookSetup {
    agent: &'static str,
    /// The file the snippet goes in.
    settings_path: Option<String>,
    /// That file already calls `agentcam guard`.
    installed: bool,
    /// JSON to merge into the file.
    snippet: String,
    /// Anything else the user has to do.
    note: &'static str,
}

#[derive(Serialize)]
pub struct GuardInfo {
    rules: Vec<GuardRule>,
    path: Option<String>,
    hooks: Vec<HookSetup>,
}

/// Claude Code and Codex use the same hook shape: a `PreToolUse` command
/// that blocks with exit code 2 and a reason on stderr.
fn hook_setups(db: Option<&Path>) -> Vec<HookSetup> {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "agentcam".into());
    let q = |s: &str| format!("\"{}\"", s.replace('"', "\\\""));
    let mut command = q(&exe);
    if let Some(db) = db {
        command.push_str(&format!(" --db {}", q(&db.display().to_string())));
    }
    command.push_str(" guard");
    let snippet = |matcher: &str| {
        serde_json::to_string_pretty(&serde_json::json!({
            "hooks": { "PreToolUse": [ { "matcher": matcher, "hooks": [ { "type": "command", "command": command } ] } ] }
        }))
        .unwrap_or_default()
    };
    let home = dirs::home_dir();
    let installed = |files: &[std::path::PathBuf]| {
        files.iter().any(|p| {
            std::fs::read_to_string(p).is_ok_and(|t| t.contains("agentcam") && t.contains(" guard"))
        })
    };
    let claude = home
        .as_ref()
        .map(|h| h.join(".claude").join("settings.json"));
    let codex_json = home.as_ref().map(|h| h.join(".codex").join("hooks.json"));
    let codex_toml = home.as_ref().map(|h| h.join(".codex").join("config.toml"));
    vec![
        HookSetup {
            agent: "claude-code",
            installed: installed(&claude.iter().cloned().collect::<Vec<_>>()),
            settings_path: claude.map(|p| p.display().to_string()),
            snippet: snippet("*"),
            note: "Merge with any hooks you already have, then restart Claude Code.",
        },
        HookSetup {
            agent: "codex",
            installed: installed(
                &codex_json
                    .iter()
                    .chain(codex_toml.iter())
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            settings_path: codex_json.map(|p| p.display().to_string()),
            snippet: snippet(".*"),
            note: "Merge with any hooks you already have, then open /hooks in Codex and trust the new hook. Codex skips hooks until you trust them.",
        },
    ]
}

fn guard_off() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, "guard is off").into_response()
}

fn bad(e: impl std::fmt::Display) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
        .into_response()
}

pub async fn guard_info(State(st): Shared) -> Response {
    let Some(path) = st.guard_path.as_deref() else {
        return guard_off();
    };
    match guard::load(path) {
        Ok(rules) => Json(GuardInfo {
            rules,
            path: Some(path.display().to_string()),
            hooks: hook_setups(st.db_path.as_deref()),
        })
        .into_response(),
        Err(e) => bad(e),
    }
}

#[derive(Deserialize)]
pub struct NewGuardRule {
    tool: String,
    pattern: String,
    reason: String,
    from_session: Option<String>,
    from_seq: Option<i64>,
}

pub async fn guard_add(State(st): Shared, Json(n): Json<NewGuardRule>) -> Response {
    let Some(path) = st.guard_path.as_deref() else {
        return guard_off();
    };
    let now = now_ms();
    let rule = GuardRule {
        id: format!("block-{}", now),
        tool: if n.tool.trim().is_empty() {
            "*".into()
        } else {
            n.tool.trim().into()
        },
        pattern: n.pattern,
        reason: if n.reason.trim().is_empty() {
            "Blocked by an agentcam guard rule.".into()
        } else {
            n.reason.trim().into()
        },
        created: Some(format_ts_ms(now)),
        from_session: n.from_session,
        from_seq: n.from_seq,
    };
    if let Err(e) = guard::validate(&rule) {
        return bad(e);
    }
    let result = guard::load(path).and_then(|mut rules| {
        rules.push(rule);
        guard::save(path, &rules).map(|()| rules)
    });
    match result {
        Ok(rules) => (StatusCode::CREATED, Json(rules)).into_response(),
        Err(e) => bad(e),
    }
}

pub async fn guard_delete(State(st): Shared, UrlPath(id): UrlPath<String>) -> Response {
    let Some(path) = st.guard_path.as_deref() else {
        return guard_off();
    };
    let result = guard::load(path).and_then(|mut rules| {
        rules.retain(|r| r.id != id);
        guard::save(path, &rules).map(|()| rules)
    });
    match result {
        Ok(rules) => Json(rules).into_response(),
        Err(e) => bad(e),
    }
}

#[derive(Deserialize)]
pub struct GuardTest {
    tool: String,
    pattern: String,
}

#[derive(Serialize)]
pub struct GuardTestResult {
    valid: bool,
    error: Option<String>,
    matches: i64,
    examples: Vec<query::StepRef>,
}

/// How many past steps a rule would have blocked.
pub async fn guard_test(
    State(st): Shared,
    Query(t): Query<GuardTest>,
) -> Result<Json<GuardTestResult>, ApiError> {
    if let Err(e) = regex::Regex::new(&t.pattern) {
        return Ok(Json(GuardTestResult {
            valid: false,
            error: Some(e.to_string()),
            matches: 0,
            examples: vec![],
        }));
    }
    let (matches, examples) = st
        .read(move |idx| query::guard_matches(idx.conn(), &t.tool, &t.pattern, 5))
        .await?;
    Ok(Json(GuardTestResult {
        valid: true,
        error: None,
        matches,
        examples,
    }))
}

#[derive(Deserialize)]
pub struct SuggestParams {
    tool: String,
    title: String,
}

pub async fn guard_suggest(Query(p): Query<SuggestParams>) -> Json<serde_json::Value> {
    let tool = guard::hook_tool(&p.tool);
    Json(serde_json::json!({ "tool": tool, "pattern": guard::suggest(tool, &p.title) }))
}

// ── Rule editor ───────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RulesInfo {
    rules: Vec<agentcam_core::audit::RuleDef>,
    builtin_ids: Vec<String>,
    user_yaml: String,
    user_path: Option<String>,
}

pub async fn rules_info(State(st): Shared) -> Response {
    let Some(path) = st.rules_path.as_deref() else {
        return guard_off();
    };
    let yaml = std::fs::read_to_string(path).unwrap_or_default();
    match agentcam_core::audit::Rules::with_user_text(&yaml) {
        Ok(r) => Json(RulesInfo {
            rules: r.defs().into_iter().cloned().collect(),
            builtin_ids: agentcam_core::audit::Rules::builtin_ids(),
            user_yaml: yaml,
            user_path: Some(path.display().to_string()),
        })
        .into_response(),
        Err(e) => bad(e),
    }
}

#[derive(Deserialize)]
pub struct SaveRules {
    yaml: String,
    /// Check only; don't write.
    #[serde(default)]
    dry_run: bool,
}

/// Validate and save the user's rules, then re-check every session.
pub async fn rules_save(State(st): Shared, Json(b): Json<SaveRules>) -> Response {
    let Some(path) = st.rules_path.clone() else {
        return guard_off();
    };
    let rules = match agentcam_core::audit::Rules::with_user_text(&b.yaml) {
        Ok(r) => r,
        Err(e) => return bad(e),
    };
    let count = rules.len();
    if b.dry_run {
        return Json(serde_json::json!({ "ok": true, "rules": count })).into_response();
    }
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return bad(e);
    }
    if let Err(e) = std::fs::write(&path, &b.yaml) {
        return bad(e);
    }
    let me = st.clone();
    let _ = tokio::task::spawn_blocking(move || me.writer.lock().unwrap().set_rules(rules)).await;
    // The rules' fingerprint changed, so this run re-checks every session.
    st.spawn_index(false);
    Json(serde_json::json!({ "ok": true, "rules": count })).into_response()
}

pub async fn commits(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<query::CommitRow>>, ApiError> {
    let f = p.filter(now_ms());
    let limit = p.limit.unwrap_or(300).min(1000);
    st.read(move |idx| query::commits(idx.conn(), &f, limit))
        .await
        .map(Json)
}

#[derive(Serialize)]
pub struct Outbox {
    schema_version: u32,
    machine_id: String,
    sessions: Vec<agentcam_core::share::SessionMeta>,
}

/// What a team server would receive in metadata-only mode. Nothing is sent
/// anywhere; this is a preview.
pub async fn outbox(State(st): Shared, Query(p): Query<Params>) -> Result<Json<Outbox>, ApiError> {
    let f = p.filter(now_ms());
    let limit = p.limit.unwrap_or(50).min(500);
    st.read(move |idx| {
        Ok(Outbox {
            schema_version: agentcam_core::share::SCHEMA_VERSION,
            machine_id: idx.machine_id().to_string(),
            sessions: agentcam_core::share::outbox(idx.conn(), &f, limit)?,
        })
    })
    .await
    .map(Json)
}
