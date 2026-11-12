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
    /// Where `reviews.json` (findings looked at, rules muted) lives.
    reviews_path: Option<std::path::PathBuf>,
    /// Where `profile.json` (fixes marked done or set aside) lives.
    profile_path: Option<std::path::PathBuf>,
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
    /// Whether the index holds any session yet; false on a first run.
    pub has_sessions: bool,
    /// The log folders agentcam reads, and whether each exists now.
    pub folders: Vec<Folder>,
}

/// A path with the home folder written as `~`, as people type it.
fn tilde(p: &Path) -> String {
    match dirs::home_dir().and_then(|h| p.strip_prefix(h).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => p.display().to_string(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Folder {
    pub agent: &'static str,
    pub path: String,
    pub exists: bool,
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
            reviews_path: Some(agentcam_core::reviews::path(db)),
            profile_path: Some(agentcam_core::ledger::path(db)),
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
            reviews_path: guard_path
                .as_ref()
                .map(|g| g.with_file_name("reviews.json")),
            profile_path: guard_path
                .as_ref()
                .map(|g| g.with_file_name("profile.json")),
            guard_path,
            db_path: None,
            writer: index.clone(),
            reader: index,
            sources,
            status: Mutex::default(),
        })
    }

    /// Fixes marked done or set aside; none when there's no file yet.
    pub fn fix_marks(&self) -> agentcam_core::ledger::Store {
        self.profile_path
            .as_deref()
            .map(agentcam_core::ledger::load)
            .unwrap_or_default()
    }

    /// Findings marked and rules muted; none when there's no file yet.
    pub fn reviews(&self) -> agentcam_core::reviews::Reviews {
        self.reviews_path
            .as_deref()
            .map(agentcam_core::reviews::load)
            .unwrap_or_default()
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
        let mut st = self.status.lock().unwrap().clone();
        st.has_sessions = self
            .reader
            .lock()
            .unwrap()
            .conn()
            .query_row("SELECT EXISTS (SELECT 1 FROM sessions)", [], |r| r.get(0))
            .unwrap_or(true);
        st.folders = self
            .sources
            .iter()
            .map(|s| Folder {
                agent: s.agent.as_str(),
                path: tilde(&s.path),
                exists: s.path.is_dir(),
            })
            .collect();
        st
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
    /// Thinking: `longest` or `recent` (default).
    sort: Option<String>,
    /// Compare: dimension and the two values, plus `at` for dates.
    dim: Option<String>,
    a: Option<String>,
    b: Option<String>,
    at: Option<String>,
    /// Prompts: words to look for.
    q: Option<String>,
    /// Evidence: one session id.
    session: Option<String>,
    /// Findings export: `ndjson` or `ocsf`.
    format: Option<String>,
    /// Audit: `open` (default, what's left to review) or `all`.
    status: Option<String>,
    /// Behind: the number to list steps for, like `time:tools`.
    what: Option<String>,
    /// Ledger weeks: one fix id.
    id: Option<String>,
    /// What lasted: the Monday a week starts on, `YYYY-MM-DD`.
    week: Option<String>,
    /// Sessions: only those with a call on this model.
    model: Option<String>,
    /// Sessions: only those with `errors`, `findings` or `retries`.
    has: Option<String>,
    /// From this time on, instead of what `range` gives.
    since: Option<String>,
}

const DAY_MS: i64 = 86_400_000;

fn range_days(range: Option<&str>) -> Option<i64> {
    range?.strip_suffix('d')?.parse().ok().filter(|d| *d > 0)
}

impl Params {
    fn filter(&self, now: i64) -> Filter {
        let since =
            self.since.clone().filter(|s| !s.is_empty()).or_else(|| {
                range_days(self.range.as_deref()).map(|d| format_ts_ms(now - d * DAY_MS))
            });
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
    let only = query::SessionOnly {
        model: p.model.clone(),
        has: p.has.clone(),
    };
    st.read(move |idx| query::sessions(idx.conn(), &f, &only, limit, offset))
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

#[derive(Deserialize)]
pub struct ExportParams {
    /// `md` (default) or `json`.
    format: Option<String>,
}

pub async fn session(
    State(st): Shared,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<ExportParams>,
) -> Result<Response, ApiError> {
    // Session ids can hold `/` (subagents), so the wildcard route also
    // carries `{id}/export`.
    if let Some(id) = id.strip_suffix("/export") {
        return export(st, id.to_string(), q.format).await;
    }
    let mut found: Option<SessionDetail> =
        st.read(move |idx| query::session(idx.conn(), &id)).await?;
    if let Some(d) = found.as_mut() {
        let r = st.reviews();
        let project = d.session.project_root.clone();
        for f in &mut d.findings {
            f.review = r.state(&d.session.id, f.seq, &f.rule_id, project.as_deref());
        }
    }
    Ok(match found {
        Some(d) => Json(d).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

fn current_rules(path: &Option<std::path::PathBuf>) -> agentcam_core::audit::Rules {
    match path {
        Some(p) => agentcam_core::audit::Rules::with_user_file(p)
            .unwrap_or_else(|_| agentcam_core::audit::Rules::builtin()),
        None => agentcam_core::audit::Rules::builtin(),
    }
}

fn download(body: String, mime: &str, name: &str) -> Response {
    (
        [
            (axum::http::header::CONTENT_TYPE, mime.to_string()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
        ],
        body,
    )
        .into_response()
}

/// `GET /api/evidence?session=<id>` or `?range=7d&project=`: an evidence
/// bundle (JSON with a SHA-256 checksum), secrets masked.
pub async fn evidence(State(st): Shared, Query(q): Query<Params>) -> Result<Response, ApiError> {
    let rules_path = st.rules_path.clone();
    let now = now_ms();
    let f = q.filter(now);
    let scope = agentcam_core::evidence::Scope {
        session_id: q.session.clone().filter(|s| !s.is_empty()),
        since: if q.session.is_some() { None } else { f.since },
        until: None,
        project: q.project.clone(),
        agent: q.agent.clone(),
    };
    let ev = st
        .read(move |idx| {
            agentcam_core::evidence::build(idx.conn(), scope, &current_rules(&rules_path))
        })
        .await?;
    // An unknown session is a 404; an empty range is an empty bundle, so a
    // download link never lands on an error page.
    if ev.sessions.is_empty() && ev.scope.session_id.is_some() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let date = &ev.generated_at[..ev.generated_at.len().min(10)];
    let name = match &ev.scope.session_id {
        Some(id) => format!(
            "agentcam-evidence-{}.json",
            id.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .take(36)
                .collect::<String>()
        ),
        None => format!("agentcam-evidence-{date}.json"),
    };
    let body = serde_json::to_string_pretty(&ev).map_err(|e| ApiError(e.to_string()))?;
    Ok(download(body, "application/json; charset=utf-8", &name))
}

/// `GET /api/findings/export?format=ndjson|ocsf&range=&severity=`: risk
/// findings, one JSON object per line, for a SIEM.
pub async fn findings_export(
    State(st): Shared,
    Query(q): Query<Params>,
) -> Result<Response, ApiError> {
    let format = q.format.clone().unwrap_or_else(|| "ndjson".into());
    if format != "ndjson" && format != "ocsf" {
        return Ok(bad("format must be ndjson or ocsf"));
    }
    let f = q.filter(now_ms());
    let floor = agentcam_core::query::severity_num(q.severity.as_deref().unwrap_or("low"));
    let records = st
        .read(move |idx| agentcam_core::siem::findings(idx.conn(), &f, 1_000_000))
        .await?;
    let records: Vec<_> = records
        .into_iter()
        .filter(|r| agentcam_core::query::severity_num(&r.severity) >= floor)
        .collect();
    let body = agentcam_core::siem::lines(&records, &format);
    Ok(download(
        body,
        "application/x-ndjson; charset=utf-8",
        &format!("agentcam-findings.{format}.ndjson"),
    ))
}

/// `GET /api/sessions/{id}/export?format=md|json`: a download of one
/// session, secrets masked.
async fn export(
    st: Arc<AppState>,
    id: String,
    format: Option<String>,
) -> Result<Response, ApiError> {
    let json = match format.as_deref() {
        None | Some("md" | "markdown") => false,
        Some("json") => true,
        Some(_) => return Ok(bad("format must be md or json")),
    };
    let rules_path = st.rules_path.clone();
    let found = st
        .read(move |idx| {
            let rules = match &rules_path {
                Some(p) => agentcam_core::audit::Rules::with_user_file(p)
                    .unwrap_or_else(|_| agentcam_core::audit::Rules::builtin()),
                None => agentcam_core::audit::Rules::builtin(),
            };
            agentcam_core::export::session(idx.conn(), &id, &rules)
        })
        .await?;
    let Some(x) = found else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let stem: String = x
        .session
        .id
        .replace('/', "-")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let (body, mime, ext) = if json {
        (
            serde_json::to_string_pretty(&x).map_err(|e| ApiError(e.to_string()))?,
            "application/json; charset=utf-8",
            "json",
        )
    } else {
        (
            agentcam_core::export::markdown(&x),
            "text/markdown; charset=utf-8",
            "md",
        )
    };
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, mime.to_string()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"agentcam-session-{stem}.{ext}\""),
            ),
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
        ],
        body,
    )
        .into_response())
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
    /// Findings in range that someone marked or muted.
    reviewed: usize,
}

pub async fn audit(State(st): Shared, Query(p): Query<Params>) -> Result<Json<Audit>, ApiError> {
    let f = p.filter(now_ms());
    let limit = p.limit.unwrap_or(200).min(500);
    // `status=all` keeps findings someone marked or muted; the default list
    // is what's left to review.
    let all = p.status.as_deref() == Some("all");
    let reviews = st.reviews();
    let mut audit = st
        .read(move |idx| {
            let c = idx.conn();
            Ok(Audit {
                rules: query::audit_rules(c, &f)?,
                findings: query::findings(
                    c,
                    &f,
                    p.rule.as_deref(),
                    p.severity.as_deref(),
                    limit * 4,
                )?,
                reviewed: 0,
            })
        })
        .await?;
    for r in &mut audit.rules {
        r.muted = reviews.muted_everywhere(&r.rule_id);
    }
    for h in &mut audit.findings {
        h.review = reviews.state(&h.session_id, h.seq, &h.rule_id, h.project_root.as_deref());
    }
    audit.reviewed = audit.findings.iter().filter(|h| h.review.is_some()).count();
    if !all {
        audit.findings.retain(|h| h.review.is_none());
    }
    audit.findings.truncate(limit as usize);
    Ok(Json(audit))
}

/// `GET /api/reviews`: findings marked and rules muted.
pub async fn reviews(State(st): Shared) -> Json<agentcam_core::reviews::Reviews> {
    Json(st.reviews())
}

/// `POST /api/reviews`: mark a finding as looked at, or mute a rule for a
/// project (or everywhere), or undo either.
pub async fn reviews_save(
    State(st): Shared,
    Json(change): Json<agentcam_core::reviews::Change>,
) -> Response {
    let Some(path) = st.reviews_path.clone() else {
        return bad("reviews are off for this index");
    };
    let mut r = agentcam_core::reviews::load(&path);
    r.apply(change);
    match agentcam_core::reviews::save(&path, &r) {
        Ok(()) => Json(r).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("couldn't save reviews: {e}"),
        )
            .into_response(),
    }
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
#[derive(Deserialize)]
pub struct PreviewRules {
    yaml: String,
}

/// `POST /api/rules/preview`: what the rules in `yaml` would find in the
/// last 90 days (most recent 400 sessions), before saving. Reads only.
pub async fn rules_preview(State(st): Shared, Json(b): Json<PreviewRules>) -> Response {
    let rules = match agentcam_core::audit::Rules::with_user_text(&b.yaml) {
        Ok(r) => r,
        Err(e) => return bad(e),
    };
    let only = agentcam_core::preview::defined_ids(&b.yaml);
    if only.is_empty() {
        return bad("no rules in the YAML to preview");
    }
    match st
        .read(move |idx| agentcam_core::preview::preview(idx.conn(), &rules, &only, 90, 400))
        .await
    {
        Ok(p) => Json(p).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.0).into_response(),
    }
}

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
    /// Agent setup by name, sent with each sync.
    inventory: agentcam_core::fleet::Inventory,
    /// The team link, if this machine joined one (never the token).
    team: Option<TeamLink>,
}

#[derive(Serialize)]
pub struct TeamLink {
    server: Option<String>,
    team: Option<String>,
    joined_at: Option<String>,
    paused: bool,
    last_sync: Option<String>,
    last_error: Option<String>,
    sessions_on_server: usize,
}

/// `team.json` and `team-sync.json` next to the index, written by
/// `agentcam team join` and the sync loop.
fn team_link(db: Option<&std::path::Path>) -> Option<TeamLink> {
    let db = db?;
    let read = |name: &str| -> Option<serde_json::Value> {
        serde_json::from_str(&std::fs::read_to_string(db.with_file_name(name)).ok()?).ok()
    };
    let link = read("team.json")?;
    let state = read("team-sync.json").unwrap_or_default();
    let s = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    Some(TeamLink {
        server: s(&link, "server"),
        team: s(&link, "team"),
        joined_at: s(&link, "joined_at"),
        paused: link
            .get("paused")
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        last_sync: s(&state, "last_sync"),
        last_error: s(&state, "last_error"),
        sessions_on_server: state
            .get("sent")
            .and_then(|x| x.as_object())
            .map_or(0, |m| m.len()),
    })
}

/// What a team server receives in metadata-only mode: a preview, or
/// exactly what the sync sends once this machine joined a team.
pub async fn outbox(State(st): Shared, Query(p): Query<Params>) -> Result<Json<Outbox>, ApiError> {
    let f = p.filter(now_ms());
    let limit = p.limit.unwrap_or(50).min(500);
    let team = team_link(st.db_path.as_deref());
    st.read(move |idx| {
        let home = dirs::home_dir().unwrap_or_default();
        Ok(Outbox {
            schema_version: agentcam_core::share::SCHEMA_VERSION,
            machine_id: idx.machine_id().to_string(),
            sessions: agentcam_core::share::outbox(idx.conn(), &f, limit)?,
            inventory: agentcam_core::fleet::inventory(
                idx.conn(),
                &home,
                env!("CARGO_PKG_VERSION"),
            )?,
            team,
        })
    })
    .await
    .map(Json)
}

/// Sessions with a step in the last 15 minutes (or `?minutes=`).
pub async fn live(
    State(st): Shared,
    Query(p): Query<LiveParams>,
) -> Result<Json<Vec<agentcam_core::live::LiveSession>>, ApiError> {
    let within = p.minutes.unwrap_or(15).clamp(1, 24 * 60) * 60_000;
    st.read(move |idx| agentcam_core::live::live(idx.conn(), now_ms(), within))
        .await
        .map(Json)
}

#[derive(Deserialize)]
pub struct LiveParams {
    minutes: Option<i64>,
}

/// Usage windows (latest readings, the last week) and limits reached.
pub async fn limits(State(st): Shared) -> Result<Json<agentcam_core::live::Limits>, ApiError> {
    st.read(move |idx| agentcam_core::live::limits(idx.conn(), now_ms()))
        .await
        .map(Json)
}

/// Which agent-written lines are still in the code (`survival.rs`).
pub async fn lasted(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::survival::Report>, ApiError> {
    let now = now_ms();
    let f = p.filter(now);
    st.read(move |idx| agentcam_core::survival::report(idx.conn(), &f, now))
        .await
        .map(Json)
}

/// `GET /api/lasted/files`: the files table of What lasted, for one project
/// (`project`) and one week (`week`, the Monday it starts on).
pub async fn lasted_files(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<agentcam_core::survival::FileRow>>, ApiError> {
    let now = now_ms();
    let f = p.filter(now);
    let week = p.week.clone().filter(|w| !w.is_empty());
    st.read(move |idx| agentcam_core::survival::files(idx.conn(), &f, now, week.as_deref()))
        .await
        .map(Json)
}

/// `GET /api/fixes`: the few changes most likely to give time or money
/// back, each with something to copy. Fixes marked done or set aside are
/// left out; the ledger has those.
pub async fn fixes(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::fixes::Fixes>, ApiError> {
    let f = p.filter(now_ms());
    let marks = st.fix_marks();
    st.read(move |idx| {
        let mut found = agentcam_core::fixes::fixes(idx.conn(), idx.prices(), &f)?;
        found.fixes.retain(|x| marks.state(&x.id).is_none());
        Ok(found)
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct FixMark {
    id: String,
    /// `applied`, `dismissed`, or nothing to clear the mark.
    state: Option<agentcam_core::ledger::State>,
}

/// `POST /api/fixes`: mark a fix done or not for me, or clear the mark.
/// Returns the ledger.
pub async fn fixes_mark(State(st): Shared, Json(m): Json<FixMark>) -> Response {
    let Some(path) = st.profile_path.clone() else {
        return bad("fix marks are off for this index");
    };
    if m.id.is_empty() || m.id.len() > 300 {
        return bad("a fix id is needed");
    }
    let now = now_ms();
    let done = st
        .read(move |idx| {
            let mut store = agentcam_core::ledger::load(&path);
            agentcam_core::ledger::mark(idx.conn(), idx.prices(), &mut store, &m.id, m.state, now)?;
            agentcam_core::ledger::save(&path, &store)?;
            agentcam_core::ledger::ledger(idx.conn(), idx.prices(), &store, now)
        })
        .await;
    match done {
        Ok(l) => Json(l).into_response(),
        Err(e) => e.into_response(),
    }
}

/// `GET /api/ledger`: fixes marked done, with the weeks before and the time
/// since side by side, and fixes set aside.
pub async fn ledger(State(st): Shared) -> Result<Json<agentcam_core::ledger::Ledger>, ApiError> {
    let marks = st.fix_marks();
    let now = now_ms();
    st.read(move |idx| agentcam_core::ledger::ledger(idx.conn(), idx.prices(), &marks, now))
        .await
        .map(Json)
}

/// `GET /api/ledger/weeks?id=<fix id>`: the fix's measure for each of the
/// last 12 weeks, with agent time per week.
pub async fn ledger_weeks(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<agentcam_core::ledger::Week>>, ApiError> {
    let id = p.id.clone().unwrap_or_default();
    let now = now_ms();
    st.read(move |idx| agentcam_core::ledger::weeks(idx.conn(), idx.prices(), &id, now))
        .await
        .map(Json)
}

/// `GET /api/cache` or `?session=<id>`: how much of each prompt came from
/// the prompt cache, and the calls where it was written again.
pub async fn cache(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::cache::Cache>, ApiError> {
    let f = p.filter(now_ms());
    let session = p.session.clone().filter(|s| !s.is_empty());
    st.read(move |idx| match &session {
        Some(id) => agentcam_core::cache::session_cache(idx.conn(), idx.prices(), id),
        None => agentcam_core::cache::cache(idx.conn(), idx.prices(), &f),
    })
    .await
    .map(Json)
}

/// `GET /api/profile` or `?session=<id>`: where agent time went.
pub async fn profile(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::profile::TimeBar>, ApiError> {
    let f = p.filter(now_ms());
    let session = p.session.clone().filter(|s| !s.is_empty());
    st.read(move |idx| match &session {
        Some(id) => agentcam_core::profile::session_time_bar(idx.conn(), id),
        None => agentcam_core::profile::time_bar(idx.conn(), &f),
    })
    .await
    .map(Json)
}

/// `GET /api/cost` or `?session=<id>`: the estimated cost by kind of token
/// and by model, at list prices, and what the prompt cache saved.
pub async fn cost(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::cost::Costs>, ApiError> {
    let f = p.filter(now_ms());
    let session = p.session.clone().filter(|s| !s.is_empty());
    st.read(move |idx| {
        let mut c = match &session {
            Some(id) => agentcam_core::cost::session_costs(idx.conn(), idx.prices(), id),
            None => agentcam_core::cost::costs(idx.conn(), idx.prices(), &f),
        }?;
        c.prices_error = idx.prices_error().map(String::from);
        Ok(c)
    })
    .await
    .map(Json)
}

/// `GET /api/behind?what=<key>`: the steps behind a number. Takes the list
/// filters and `session`.
pub async fn behind(State(st): Shared, Query(p): Query<Params>) -> Response {
    let Some(what) = p.what.clone().filter(|w| !w.is_empty()) else {
        return bad("what= is needed");
    };
    let f = p.filter(now_ms());
    let session = p.session.clone().filter(|s| !s.is_empty());
    let found = st
        .read(move |idx| {
            agentcam_core::behind::behind(idx.conn(), idx.prices(), &f, session.as_deref(), &what)
        })
        .await;
    match found {
        Ok(Some(b)) => Json(b).into_response(),
        Ok(None) => bad("unknown key"),
        Err(e) => e.into_response(),
    }
}

/// Turn lengths, waits between turns, and turns by model, effort and mode.
pub async fn turns(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::turns::TurnStats>, ApiError> {
    let f = p.filter(now_ms());
    st.read(move |idx| agentcam_core::turns::turn_stats(idx.conn(), &f))
        .await
        .map(Json)
}

/// How tool calls were allowed or stopped, and allow rules to suggest.
pub async fn approvals(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::approvals::Approvals>, ApiError> {
    let f = p.filter(now_ms());
    st.read(move |idx| agentcam_core::approvals::approvals(idx.conn(), &f))
        .await
        .map(Json)
}

/// Thinking steps and thinking time by model. Search thinking text with
/// `/api/search?kind=thinking`.
pub async fn thinking(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::approvals::Thinking>, ApiError> {
    let f = p.filter(now_ms());
    let sort = p.sort.clone().unwrap_or_default();
    let limit = p.limit.unwrap_or(50).min(200);
    let model = p.model.clone().filter(|m| !m.is_empty());
    st.read(move |idx| {
        agentcam_core::approvals::thinking(idx.conn(), &f, &sort, model.as_deref(), limit)
    })
    .await
    .map(Json)
}

/// Pull requests sessions linked, newest first.
pub async fn prs(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<agentcam_core::turns::PrRow>>, ApiError> {
    let f = p.filter(now_ms());
    let limit = p.limit.unwrap_or(100).min(500);
    st.read(move |idx| agentcam_core::turns::prs(idx.conn(), &f, None, limit))
        .await
        .map(Json)
}

/// Skills, plugins, MCP servers and apps behind steps.
pub async fn sources(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<agentcam_core::turns::Source>>, ApiError> {
    let f = p.filter(now_ms());
    st.read(move |idx| agentcam_core::turns::sources(idx.conn(), &f))
        .await
        .map(Json)
}

fn home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

/// Skills: installed, listed and used, with recent uses.
pub async fn skills(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::skills::Report>, ApiError> {
    let f = p.filter(now_ms());
    let home = home_dir();
    st.read(move |idx| agentcam_core::skills::report(idx.conn(), &f, &home))
        .await
        .map(Json)
}

/// Values to compare by and comparisons found in the data.
pub async fn compare_options(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::compare::Options>, ApiError> {
    let now = now_ms();
    let f = p.filter(now);
    st.read(move |idx| agentcam_core::compare::options(idx.conn(), &f, now))
        .await
        .map(Json)
}

/// Two groups of turns side by side: `dim`, `a`, `b` (and `at` for dates).
pub async fn compare(State(st): Shared, Query(p): Query<Params>) -> Response {
    let bad = |msg: &str| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response()
    };
    let now = now_ms();
    let f = p.filter(now);
    let Ok(dim) = serde_json::from_value::<agentcam_core::compare::Dim>(serde_json::Value::String(
        p.dim.clone().unwrap_or_default(),
    )) else {
        return bad("dim must be model, agent, agent_version, effort, mode, skill or date");
    };
    let (Some(a), Some(b)) = (p.a.clone(), p.b.clone()) else {
        return bad("a and b are required");
    };
    let req = agentcam_core::compare::Request {
        dim,
        a,
        b,
        at: p.at.clone(),
    };
    match st
        .read(move |idx| agentcam_core::compare::compare(idx.conn(), &f, &req, now))
        .await
    {
        Ok(c) => Json(c).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Each project's AGENTS.md / CLAUDE.md rules and how sessions followed them.
pub async fn follow(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<Vec<agentcam_core::follow::ProjectFollow>>, ApiError> {
    let f = p.filter(now_ms());
    st.read(move |idx| agentcam_core::follow::report(idx.conn(), &f))
        .await
        .map(Json)
}

/// Which agent edit wrote each line of a file today.
pub async fn blame(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::survival::Blame>, ApiError> {
    let path = p.path.clone().unwrap_or_default();
    st.read(move |idx| agentcam_core::survival::blame(idx.conn(), &path))
        .await
        .map(Json)
}

/// Settings, memory files, MCP servers, hooks and skills, with checks.
pub async fn setup(State(st): Shared) -> Result<Json<agentcam_core::setup::Setup>, ApiError> {
    let home = home_dir();
    st.read(move |idx| agentcam_core::setup::report(idx.conn(), &home))
        .await
        .map(Json)
}

/// Prompts you typed, newest first, and the ones you repeat.
pub async fn prompts(
    State(st): Shared,
    Query(p): Query<Params>,
) -> Result<Json<agentcam_core::prompts::Report>, ApiError> {
    let f = p.filter(now_ms());
    let limit = p.limit.unwrap_or(200).min(1000);
    let q = p.q.clone();
    st.read(move |idx| agentcam_core::prompts::report(idx.conn(), &f, q.as_deref(), limit))
        .await
        .map(Json)
}
