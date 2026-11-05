//! Local HTTP server for agentcam: a JSON API under `/api` and the
//! embedded web UI for everything else.

use std::net::SocketAddr;

use agentcam_core::LogSource;
use axum::{
    Json, Router,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
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

/// The full app: API routes plus the embedded UI.
pub fn router() -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .fallback(api_not_found);

    Router::new().nest("/api", api).fallback(static_handler)
}

/// Bind to localhost on `port`. Port 0 picks a free one.
pub async fn bind(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await
}

/// Serve the app on `listener` until Ctrl-C.
pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    axum::serve(listener, router())
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

    async fn get(path: &str) -> Response {
        router()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn health_reports_name_and_version() {
        let res = get("/api/health").await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
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
