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
    pub fn open(db: &Path, sources: Vec<LogSource>) -> agentcam_core::index::Result<Arc<Self>> {
        let writer = Index::open(db)?;
        let reader = Index::open(db)?;
        Ok(Arc::new(AppState {
            writer: Arc::new(Mutex::new(writer)),
            reader: Arc::new(Mutex::new(reader)),
            sources,
            status: Mutex::default(),
        }))
    }

    /// One shared connection for both, for tests and in-memory indexes.
    pub fn from_index(index: Index, sources: Vec<LogSource>) -> Arc<Self> {
        let index = Arc::new(Mutex::new(index));
        Arc::new(AppState {
            writer: index.clone(),
            reader: index,
            sources,
            status: Mutex::default(),
        })
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
