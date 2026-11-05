//! Local HTTP server for agentcam: a JSON API under `/api` and the
//! embedded web UI for everything else.

pub mod api;
mod watch;

use std::net::SocketAddr;
use std::sync::Arc;

use agentcam_core::LogSource;
use axum::{
    Json, Router,
    extract::Request,
    http::{StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};

pub use api::AppState;
use rust_embed::Embed;
use serde::Serialize;
use tokio::net::TcpListener;

#[derive(Embed)]
#[folder = "ui-dist/"]
struct Assets;

#[derive(Serialize)]
struct Health {
    name: &'static str,
    version: &'static str,
    sources: Vec<LogSource>,
}

async fn health() -> Json<Health> {
    Json(Health {
        name: "agentcam",
        version: env!("CARGO_PKG_VERSION"),
        sources: agentcam_core::default_log_sources(),
    })
}

async fn api_not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// Serve a built asset, or fall back to `index.html` so client-side
/// routes work on reload.
async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match Assets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            // Vite puts a content hash in every file under assets/, so those
            // never change. Everything else (index.html) must be re-checked.
            let cache = if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            (
                [
                    (header::CONTENT_TYPE, mime.as_ref()),
                    (header::CACHE_CONTROL, cache),
                ],
                file.data,
            )
                .into_response()
        }
        None => match Assets::get("index.html") {
            Some(index) => (
                [
                    (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    (header::CACHE_CONTROL, "no-cache"),
                ],
                index.data,
            )
                .into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

/// Only answer requests addressed to this machine by a loopback name.
/// A web page on another site can point its own hostname at 127.0.0.1
/// (DNS rebinding); its requests then carry that hostname and are refused.
async fn local_host_only(req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let name = match host.rsplit_once(':') {
        Some((n, port)) if port.chars().all(|c| c.is_ascii_digit()) => n,
        _ => host,
    };
    if matches!(name, "127.0.0.1" | "localhost" | "[::1]") {
        next.run(req).await
    } else {
        (StatusCode::FORBIDDEN, "agentcam only answers on localhost").into_response()
    }
}

/// Requests that change something must come from agentcam's own page.
/// Another site can't read our responses, but it could still send a form
/// POST to localhost; browsers mark those with `Origin` and
/// `Sec-Fetch-Site`, so refuse anything cross-site.
async fn same_site_writes(req: Request, next: Next) -> Response {
    let m = req.method();
    if m == axum::http::Method::GET || m == axum::http::Method::HEAD {
        return next.run(req).await;
    }
    let h = req.headers();
    let site_ok = h
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| v == "same-origin" || v == "none");
    let origin_ok = h
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|o| {
            let host = o.split("://").nth(1).unwrap_or("");
            let name = host.rsplit_once(':').map_or(host, |(n, _)| n);
            matches!(name, "127.0.0.1" | "localhost" | "[::1]")
        });
    if site_ok && origin_ok {
        next.run(req).await
    } else {
        (StatusCode::FORBIDDEN, "cross-site request refused").into_response()
    }
}

/// The full app: API routes plus the embedded UI.
pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/status", get(api::status))
        .route("/index", post(api::reindex))
        .route("/overview", get(api::overview))
        .route("/activity", get(api::activity))
        .route("/sessions", get(api::sessions))
        .route("/sessions/{*id}", get(api::session))
        .route("/step", get(api::step))
        .route("/search", get(api::search))
        .route("/audit", get(api::audit))
        .route("/tools", get(api::tools))
        .route("/tool-calls", get(api::tool_calls))
        .route("/files", get(api::files))
        .route("/file", get(api::file_history))
        .route("/guard", get(api::guard_info).post(api::guard_add))
        .route("/guard/test", get(api::guard_test))
        .route("/guard/suggest", get(api::guard_suggest))
        .route("/guard/{id}", axum::routing::delete(api::guard_delete))
        .fallback(api_not_found)
        .with_state(state);

    Router::new()
        .nest("/api", api)
        .fallback(static_handler)
        .layer(middleware::from_fn(same_site_writes))
        .layer(middleware::from_fn(local_host_only))
}

/// Bind to localhost on `port`. Port 0 picks a free one.
pub async fn bind(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await
}

/// Serve the app on `listener` until Ctrl-C.
pub async fn serve(listener: TcpListener, state: Arc<AppState>) -> std::io::Result<()> {
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

    fn fixtures() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
    }

    fn state() -> Arc<AppState> {
        use agentcam_core::{Agent, Index};
        let sources = vec![
            LogSource {
                agent: Agent::ClaudeCode,
                path: fixtures().join("claude-code/projects"),
                exists: true,
            },
            LogSource {
                agent: Agent::Codex,
                path: fixtures().join("codex/sessions"),
                exists: true,
            },
        ];
        let mut idx = Index::open_in_memory().unwrap();
        idx.update(&sources, false).unwrap();
        let guard = std::env::temp_dir()
            .join(format!(
                "agentcam-srv-{}-{}",
                std::process::id(),
                rand_suffix()
            ))
            .join("guard.yaml");
        AppState::from_index(idx, sources, Some(guard))
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    async fn send(app: &Router, req: Request<Body>) -> Response {
        app.clone().oneshot(req).await.unwrap()
    }

    #[tokio::test]
    async fn guard_rules_add_test_delete_and_block_cross_site() {
        let app = router(state());
        let post = |body: &str, extra: Option<(&'static str, &'static str)>| {
            let mut b = Request::post("/api/guard")
                .header(header::HOST, "127.0.0.1:4747")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some((k, v)) = extra {
                b = b.header(k, v);
            }
            b.body(Body::from(body.to_string())).unwrap()
        };
        let rule = r#"{"tool":"Bash","pattern":"npm\\s+test","reason":"no tests"}"#;
        // Cross-site writes are refused.
        let evil = send(&app, post(rule, Some(("origin", "https://evil.example")))).await;
        assert_eq!(evil.status(), StatusCode::FORBIDDEN);
        let evil = send(&app, post(rule, Some(("sec-fetch-site", "cross-site")))).await;
        assert_eq!(evil.status(), StatusCode::FORBIDDEN);
        // Bad patterns are refused.
        let bad = send(
            &app,
            post(r#"{"tool":"Bash","pattern":"(","reason":""}"#, None),
        )
        .await;
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        // Same-site add works.
        let ok = send(&app, post(rule, Some(("origin", "http://127.0.0.1:4747")))).await;
        assert_eq!(ok.status(), StatusCode::CREATED);
        let body = to_bytes(ok.into_body(), usize::MAX).await.unwrap();
        let rules: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = rules[0]["id"].as_str().unwrap().to_string();

        let get = |p: String| {
            Request::get(p)
                .header(header::HOST, "127.0.0.1:4747")
                .body(Body::empty())
                .unwrap()
        };
        let t = send(
            &app,
            get("/api/guard/test?tool=Bash&pattern=npm%5Cs%2Btest".into()),
        )
        .await;
        let t: serde_json::Value =
            serde_json::from_slice(&to_bytes(t.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(t["matches"], 1, "the fixture's `npm test`: {t}");

        let del = Request::delete(format!("/api/guard/{id}"))
            .header(header::HOST, "127.0.0.1:4747")
            .body(Body::empty())
            .unwrap();
        let del = send(&app, del).await;
        assert_eq!(del.status(), StatusCode::OK);
        let info = send(&app, get("/api/guard".into())).await;
        let info: serde_json::Value =
            serde_json::from_slice(&to_bytes(info.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(info["rules"].as_array().unwrap().len(), 0);
        assert!(
            info["hook"]["snippet"]
                .as_str()
                .unwrap()
                .contains("PreToolUse")
        );
    }

    async fn get_host(path: &str, host: &str) -> Response {
        router(state())
            .oneshot(
                Request::get(path)
                    .header(header::HOST, host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn get(path: &str) -> Response {
        get_host(path, "127.0.0.1:4747").await
    }

    async fn json(path: &str) -> serde_json::Value {
        let res = get(path).await;
        assert_eq!(res.status(), StatusCode::OK, "{path}");
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn refuses_other_hosts() {
        assert_eq!(
            get_host("/api/health", "evil.example:4747").await.status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            get_host("/", "evil.example").await.status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            get_host("/api/health", "localhost:4747").await.status(),
            StatusCode::OK
        );
        assert_eq!(
            get_host("/api/health", "[::1]:4747").await.status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn overview_and_sessions() {
        let a = json("/api/audit").await;
        assert!(a["rules"].is_array() && a["findings"].is_array());
        let t = json("/api/tools").await;
        assert!(
            t.as_array()
                .unwrap()
                .iter()
                .any(|x| x["tool_name"] == "Bash")
        );
        let calls = json("/api/tool-calls?tool=Bash").await;
        assert!(!calls.as_array().unwrap().is_empty());
        let files = json("/api/files").await;
        assert!(!files.as_array().unwrap().is_empty());
        let h = json("/api/file?path=/Users/dev/projects/demo/src/auth.ts").await;
        assert_eq!(h.as_array().unwrap().len(), 2);
        let o = json("/api/overview").await;
        assert_eq!(o["stats"]["sessions"], 3);
        assert!(o["previous"].is_null());
        let s = json("/api/sessions?agent=codex").await;
        assert_eq!(s["total"], 2);
        let d = json("/api/sessions/11111111-1111-4111-8111-111111111111").await;
        assert_eq!(d["subagents"].as_array().unwrap().len(), 1);
        let sub = json("/api/sessions/11111111-1111-4111-8111-111111111111/agent-a1").await;
        assert_eq!(
            sub["session"]["parent_id"],
            "11111111-1111-4111-8111-111111111111"
        );
        assert_eq!(
            get("/api/sessions/missing").await.status(),
            StatusCode::NOT_FOUND
        );
        assert!(d["files"].as_array().unwrap().len() >= 3);
        assert_eq!(
            d["events"][0]["text"],
            "The login form rejects valid passwords. Please fix it."
        );

        // seq 5 is `npm test`, which failed; seq 4 is the edit.
        let step = json("/api/step?session=11111111-1111-4111-8111-111111111111&seq=5").await;
        assert_eq!(step["title"], "npm test");
        assert_eq!(step["is_error"], true);
        let edit = json("/api/step?session=11111111-1111-4111-8111-111111111111&seq=4").await;
        let diffs: String = edit["touches"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["diff"].as_str())
            .collect();
        assert!(diffs.contains("+export function check(p)"), "{edit}");
        assert_eq!(
            get("/api/step?session=x&seq=0").await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn health_reports_name_and_version() {
        let json = json("/api/health").await;
        assert_eq!(json["name"], "agentcam");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn unknown_api_route_is_404_not_index() {
        assert_eq!(get("/api/nope").await.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn index_is_never_cached() {
        let res = get("/").await;
        assert_eq!(res.headers()[header::CACHE_CONTROL], "no-cache");
        let res = get("/some/client/route").await;
        assert_eq!(res.headers()[header::CACHE_CONTROL], "no-cache");
    }

    #[tokio::test]
    async fn client_routes_fall_back_to_index() {
        let res = get("/sessions/abc").await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            res.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
    }
}
