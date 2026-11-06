//! The team server: a sync API for laptops (device tokens), a JSON API for
//! the team UI (viewer tokens), and the same embedded web UI as the local
//! app, which switches to its team pages under `/team`.
//!
//! Unlike the local app this one is meant to be reached over the network,
//! so it checks a token on every API call instead of the Host header. Run
//! it behind TLS (a reverse proxy) when it's not on localhost.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};

use agentcam_core::fleet::Inventory;
use agentcam_core::share::SessionMeta;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::store::{Device, Store, Viewer};
use crate::views::{self, Scope};
use crate::{Error, Result};

pub struct TeamState {
    store: Mutex<Store>,
}

type Shared = State<Arc<TeamState>>;

impl TeamState {
    pub fn new(store: Store) -> Arc<Self> {
        Arc::new(TeamState {
            store: Mutex::new(store),
        })
    }

    pub fn open(data_dir: &Path) -> Result<Arc<Self>> {
        Ok(Self::new(Store::open(&data_dir.join("team.db"))?))
    }

    /// Run `f` with the store on a blocking thread.
    async fn with<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&mut Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let st = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut s = st.store.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut s)
        })
        .await
        .map_err(|e| Error::Http(e.to_string()))?
    }
}

struct ApiError(Error);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, msg) = match &self.0 {
            Error::Bad(m) => (StatusCode::BAD_REQUEST, m.clone()),
            Error::Denied(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            e => {
                tracing::error!("team api: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server error".to_string(),
                )
            }
        };
        (code, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

type ApiResult<T> = std::result::Result<Json<T>, ApiError>;

fn bearer(h: &HeaderMap) -> Option<String> {
    h.get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|t| t.trim().to_string())
}

async fn viewer(st: &Arc<TeamState>, h: &HeaderMap) -> Result<Viewer> {
    let token = bearer(h).ok_or_else(|| Error::Denied("sign in with a viewer token".into()))?;
    st.with(move |s| s.viewer(&token))
        .await?
        .ok_or_else(|| Error::Denied("unknown token".into()))
}

async fn admin(st: &Arc<TeamState>, h: &HeaderMap) -> Result<Viewer> {
    let v = viewer(st, h).await?;
    if v.role == "admin" {
        Ok(v)
    } else {
        Err(Error::Denied("admins only".into()))
    }
}

async fn device(st: &Arc<TeamState>, h: &HeaderMap) -> Result<Device> {
    let token = bearer(h).ok_or_else(|| Error::Denied("missing device token".into()))?;
    st.with(move |s| s.device(&token))
        .await?
        .ok_or_else(|| Error::Denied("unknown device; join again".into()))
}

#[derive(Serialize)]
struct Health {
    name: &'static str,
    version: &'static str,
    mode: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        name: "agentcam",
        version: env!("CARGO_PKG_VERSION"),
        mode: "team",
    })
}

#[derive(Serialize, Deserialize)]
pub struct JoinReq {
    pub invite: String,
    pub machine_id: String,
    pub os_user: Option<String>,
}

async fn join(State(st): Shared, Json(req): Json<JoinReq>) -> ApiResult<crate::store::Joined> {
    let j = st
        .with(move |s| s.redeem_invite(&req.invite, &req.machine_id, req.os_user.as_deref()))
        .await?;
    Ok(Json(j))
}

#[derive(Serialize, Deserialize)]
pub struct SyncReq {
    pub schema_version: u32,
    pub agentcam_version: Option<String>,
    pub sessions: Vec<SessionMeta>,
    pub inventory: Option<Inventory>,
}

#[derive(Serialize, Deserialize)]
pub struct SyncResp {
    pub received: usize,
    pub written: usize,
}

async fn sync(State(st): Shared, h: HeaderMap, Json(req): Json<SyncReq>) -> ApiResult<SyncResp> {
    let dev = device(&st, &h).await?;
    if req.schema_version == 0 || req.schema_version > agentcam_core::share::SCHEMA_VERSION {
        return Err(Error::Bad(format!(
            "this server reads share schema 1 to {}; update the server",
            agentcam_core::share::SCHEMA_VERSION
        ))
        .into());
    }
    if req.sessions.len() > 1000 {
        return Err(Error::Bad("send at most 1000 sessions at a time".into()).into());
    }
    let received = req.sessions.len();
    let written = st
        .with(move |s| {
            s.ingest(
                &dev,
                req.agentcam_version.as_deref(),
                &req.sessions,
                req.inventory.as_ref(),
            )
        })
        .await?;
    Ok(Json(SyncResp { received, written }))
}

#[derive(Deserialize, Default)]
struct ScopeParams {
    range: Option<String>,
    team: Option<i64>,
}

impl ScopeParams {
    fn scope(&self) -> Scope {
        let days = match self.range.as_deref() {
            Some("all") => None,
            Some(r) => r
                .strip_suffix('d')
                .and_then(|d| d.parse().ok())
                .or(Some(30)),
            None => Some(30),
        };
        Scope {
            days,
            team: self.team,
        }
    }
}

fn now_ms() -> i64 {
    agentcam_core::util::now_ms()
}

#[derive(Serialize)]
struct Me {
    name: String,
    role: String,
    org: String,
}

async fn me(State(st): Shared, h: HeaderMap) -> ApiResult<Me> {
    let v = viewer(&st, &h).await?;
    let org = st
        .with(|s| Ok(s.setting("org_name").unwrap_or_else(|| "Your team".into())))
        .await?;
    Ok(Json(Me {
        name: v.name,
        role: v.role,
        org,
    }))
}

async fn insights(
    State(st): Shared,
    h: HeaderMap,
    Query(p): Query<ScopeParams>,
) -> ApiResult<views::Insights> {
    viewer(&st, &h).await?;
    let sc = p.scope();
    Ok(Json(
        st.with(move |s| views::insights(s, &sc, now_ms())).await?,
    ))
}

async fn value(
    State(st): Shared,
    h: HeaderMap,
    Query(p): Query<ScopeParams>,
) -> ApiResult<views::Value> {
    viewer(&st, &h).await?;
    let sc = p.scope();
    Ok(Json(
        st.with(move |s| views::value(s, &sc, now_ms())).await?,
    ))
}

async fn governance(
    State(st): Shared,
    h: HeaderMap,
    Query(p): Query<ScopeParams>,
) -> ApiResult<views::Governance> {
    viewer(&st, &h).await?;
    let sc = p.scope();
    Ok(Json(
        st.with(move |s| views::governance(s, &sc, now_ms()))
            .await?,
    ))
}

async fn fleet(
    State(st): Shared,
    h: HeaderMap,
    Query(p): Query<ScopeParams>,
) -> ApiResult<views::Fleet> {
    viewer(&st, &h).await?;
    let sc = p.scope();
    Ok(Json(st.with(move |s| views::fleet(s, &sc)).await?))
}

#[derive(Serialize)]
struct Settings {
    org_name: String,
    seat_price_usd: Option<f64>,
    show_people: bool,
    teams: Vec<crate::store::Team>,
    /// Admins only.
    log: Vec<crate::store::LogEntry>,
}

async fn settings(State(st): Shared, h: HeaderMap) -> ApiResult<Settings> {
    let v = viewer(&st, &h).await?;
    let is_admin = v.role == "admin";
    Ok(Json(
        st.with(move |s| {
            Ok(Settings {
                org_name: s.setting("org_name").unwrap_or_else(|| "Your team".into()),
                seat_price_usd: s.setting("seat_price_usd").and_then(|p| p.parse().ok()),
                show_people: views::show_people(s),
                teams: s.teams()?,
                log: if is_admin {
                    s.audit_log(100)?
                } else {
                    Vec::new()
                },
            })
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct SettingsReq {
    org_name: Option<String>,
    seat_price_usd: Option<f64>,
    show_people: Option<bool>,
}

async fn save_settings(
    State(st): Shared,
    h: HeaderMap,
    Json(req): Json<SettingsReq>,
) -> ApiResult<serde_json::Value> {
    let v = admin(&st, &h).await?;
    st.with(move |s| {
        if let Some(n) = req
            .org_name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
        {
            s.set_setting("org_name", &n)?;
            s.log(&v.name, "renamed org", Some(&n))?;
        }
        if let Some(p) = req.seat_price_usd {
            if !(0.0..100_000.0).contains(&p) {
                return Err(Error::Bad("seat price out of range".into()));
            }
            s.set_setting("seat_price_usd", &p.to_string())?;
            s.log(&v.name, "set seat price", Some(&format!("${p}/month")))?;
        }
        if let Some(b) = req.show_people {
            s.set_setting("show_people", if b { "true" } else { "false" })?;
            s.log(
                &v.name,
                if b {
                    "turned on names"
                } else {
                    "turned off names"
                },
                None,
            )?;
        }
        Ok(())
    })
    .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct InviteReq {
    team: String,
    uses: Option<i64>,
    days: Option<i64>,
}

#[derive(Serialize)]
struct InviteResp {
    code: String,
    team: String,
    expires_in_days: i64,
}

async fn invite(
    State(st): Shared,
    h: HeaderMap,
    Json(req): Json<InviteReq>,
) -> ApiResult<InviteResp> {
    let v = admin(&st, &h).await?;
    let days = req.days.unwrap_or(7).clamp(1, 365);
    let r = st
        .with(move |s| {
            let id = s.team(&req.team)?;
            let code = s.create_invite(id, req.uses.unwrap_or(50), days)?;
            s.log(&v.name, "created invite", Some(&req.team))?;
            Ok(InviteResp {
                code,
                team: req.team.trim().to_string(),
                expires_in_days: days,
            })
        })
        .await?;
    Ok(Json(r))
}

#[derive(Deserialize)]
struct ViewerReq {
    name: String,
    role: String,
}

async fn add_viewer(
    State(st): Shared,
    h: HeaderMap,
    Json(req): Json<ViewerReq>,
) -> ApiResult<serde_json::Value> {
    let v = admin(&st, &h).await?;
    let token = st
        .with(move |s| {
            let t = s.create_viewer(req.name.trim(), &req.role)?;
            s.log(
                &v.name,
                "added viewer",
                Some(&format!("{} ({})", req.name.trim(), req.role)),
            )?;
            Ok(t)
        })
        .await?;
    Ok(Json(serde_json::json!({ "token": token })))
}

pub fn router(state: Arc<TeamState>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/team/join", post(join))
        .route(
            "/team/sync",
            post(sync).layer(DefaultBodyLimit::max(64 << 20)),
        )
        .route("/team/me", get(me))
        .route("/team/insights", get(insights))
        .route("/team/value", get(value))
        .route("/team/governance", get(governance))
        .route("/team/fleet", get(fleet))
        .route("/team/settings", get(settings).post(save_settings))
        .route("/team/invites", post(invite))
        .route("/team/viewers", post(add_viewer))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(state);
    Router::new()
        .route("/", get(|| async { Redirect::temporary("/team") }))
        .nest("/api", api)
        .fallback(agentcam_server::static_handler)
}

/// Serve until Ctrl-C.
pub async fn serve(addr: SocketAddr, state: Arc<TeamState>) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

    async fn call(
        app: &Router,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut b = Request::builder().method(method).uri(path);
        if let Some(t) = token {
            b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(j) => b
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(j.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let res = app.clone().oneshot(req).await.unwrap();
        let code = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (
            code,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    fn meta(machine: &str, id: &str, version: &str) -> serde_json::Value {
        serde_json::json!({
            "session_id": id, "parent_id": null, "version": version, "machine_id": machine,
            "os_user": "dev", "agent": "claude-code", "agent_version": "2.1.0", "model": "claude-opus-5-5",
            "project_remote": "github.com/acme/api", "project_name": "api", "branch": "main",
            "started_at": "2026-11-05T10:00:00.000Z", "ended_at": agentcam_core::util::format_ts_ms(agentcam_core::util::now_ms() - 3_600_000),
            "active_ms": 600000, "retry_ms": 60000, "steps": {"tool_call": 40, "prompt": 3},
            "tool_calls": {"command": 30, "edit": 10}, "errored_steps": 4,
            "tokens": {"fresh_input": 1000, "output": 2000, "cache_read": 50000, "cache_write": 3000},
            "est_cost_usd": 2.5, "peak_context": 90000,
            "findings": [{"rule_id": "force-push", "severity": "high", "step": 12, "excerpt": "git push -f"}],
            "commits": [{"step": 30, "hash": "abc1234", "message": "Fix"}],
            "files": [{"path": "src/a.rs", "action": "edit"}],
            "turns": 3, "lasted": {"added": 100, "kept": 80, "reworked": 10, "changed": 10},
            "follow": {"followed": 2, "not_followed": 1}, "prs": [{"number": 7, "repo": "acme/api"}]
        })
    }

    #[tokio::test]
    async fn join_sync_and_read_views() {
        let store = Store::open_in_memory().unwrap();
        let admin_token = store.create_viewer("Admin", "admin").unwrap();
        let viewer_token = store.create_viewer("Lead", "viewer").unwrap();
        let app = router(TeamState::new(store));

        // Views need a token.
        let (code, _) = call(&app, "GET", "/api/team/insights", None, None).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        // Only admins make invites.
        let body = serde_json::json!({"team": "Platform"});
        let (code, _) = call(
            &app,
            "POST",
            "/api/team/invites",
            Some(&viewer_token),
            Some(body.clone()),
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        let (code, inv) = call(
            &app,
            "POST",
            "/api/team/invites",
            Some(&admin_token),
            Some(body),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        let invite = inv["code"].as_str().unwrap().to_string();

        let machine = "m0000000000000000000000000000001";
        let (code, _) = call(
            &app,
            "POST",
            "/api/team/join",
            None,
            Some(serde_json::json!({"invite": "ac_inv_nope", "machine_id": machine})),
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        let (code, j) = call(
            &app,
            "POST",
            "/api/team/join",
            None,
            Some(serde_json::json!({"invite": invite, "machine_id": machine, "os_user": "dev"})),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{j}");
        let dev = j["device_token"].as_str().unwrap().to_string();
        assert_eq!(j["team"], "Platform");

        let sync = |sessions: Vec<serde_json::Value>| {
            serde_json::json!({
                "schema_version": 2, "agentcam_version": "0.1.0", "sessions": sessions,
                "inventory": {"os": "macos", "arch": "aarch64", "agentcam_version": "0.1.0",
                    "agents": [{"agent": "claude-code", "version": "2.1.0", "sessions": 1, "last_used": null}],
                    "items": [{"kind": "mcp_server", "agent": "claude-code", "name": "linear", "detail": "http", "scope": "user"}],
                    "flags": [{"id": "approvals_skipped_by_default", "agent": "codex", "scope": "user"}]}
            })
        };
        let (code, r) = call(
            &app,
            "POST",
            "/api/team/sync",
            Some(&dev),
            Some(sync(vec![meta(machine, "s1", "v1")])),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{r}");
        assert_eq!(r["written"], 1);
        // Same version again: nothing new to write.
        let (_, r) = call(
            &app,
            "POST",
            "/api/team/sync",
            Some(&dev),
            Some(sync(vec![meta(machine, "s1", "v1")])),
        )
        .await;
        assert_eq!(r["written"], 0);
        // A laptop can't send another machine's sessions.
        let (code, _) = call(
            &app,
            "POST",
            "/api/team/sync",
            Some(&dev),
            Some(sync(vec![meta(
                "m0000000000000000000000000000002",
                "s9",
                "v1",
            )])),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        // Viewer tokens can't sync.
        let (code, _) = call(
            &app,
            "POST",
            "/api/team/sync",
            Some(&viewer_token),
            Some(sync(vec![])),
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);

        let (_, ins) = call(
            &app,
            "GET",
            "/api/team/insights?range=7d",
            Some(&viewer_token),
            None,
        )
        .await;
        assert_eq!(ins["totals"]["sessions"], 1);
        assert_eq!(ins["totals"]["lasted"]["kept"], 80);
        assert_eq!(ins["by_project"][0]["name"], "github.com/acme/api");
        let (_, val) = call(
            &app,
            "GET",
            "/api/team/value?range=30d",
            Some(&viewer_token),
            None,
        )
        .await;
        assert_eq!(val["cost_per_commit"], 2.5);
        assert_eq!(val["kept_lines_per_dollar"], 32.0);
        let (_, gov) = call(
            &app,
            "GET",
            "/api/team/governance",
            Some(&viewer_token),
            None,
        )
        .await;
        assert_eq!(gov["rules"][0]["rule_id"], "force-push");
        assert_eq!(
            gov["latest"][0]["machine"], "Machine m00000",
            "names hidden by default"
        );
        let (_, fl) = call(&app, "GET", "/api/team/fleet", Some(&viewer_token), None).await;
        assert_eq!(fl["items"][0]["name"], "linear");
        assert_eq!(fl["flags"][0]["id"], "approvals_skipped_by_default");
        assert_eq!(fl["machines"][0]["os"], "macos aarch64");

        // Admin turns names on; the change is logged.
        let (code, _) = call(
            &app,
            "POST",
            "/api/team/settings",
            Some(&admin_token),
            Some(serde_json::json!({"show_people": true, "seat_price_usd": 30.0})),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        let (_, gov) = call(
            &app,
            "GET",
            "/api/team/governance",
            Some(&viewer_token),
            None,
        )
        .await;
        assert_eq!(gov["latest"][0]["machine"], "dev");
        let (_, st) = call(&app, "GET", "/api/team/settings", Some(&admin_token), None).await;
        assert!(
            st["log"]
                .as_array()
                .unwrap()
                .iter()
                .any(|l| l["action"] == "turned on names")
        );
        let (_, st) = call(&app, "GET", "/api/team/settings", Some(&viewer_token), None).await;
        assert!(
            st["log"].as_array().unwrap().is_empty(),
            "the log is for admins"
        );
    }
}
