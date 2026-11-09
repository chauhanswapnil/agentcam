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

/// Pages may not be framed by another site (a hidden frame could trick
/// clicks on "Save" or "Block next time"), and load nothing from the
/// network: fonts and scripts ship inside the binary.
const PAGE_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
     img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; \
     frame-ancestors 'none'; base-uri 'none'; form-action 'self'; object-src 'none'";

/// Serve a built asset, or fall back to `index.html` so client-side
/// routes work on reload. Used by the team server too.
pub async fn static_handler(uri: Uri) -> Response {
    let mut res = asset(uri).await;
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        header::HeaderValue::from_static(PAGE_POLICY),
    );
    h.insert(
        header::X_FRAME_OPTIONS,
        header::HeaderValue::from_static("DENY"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("no-referrer"),
    );
    res
}

async fn asset(uri: Uri) -> Response {
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
    // The page's own origin: same host and port as this server. Another
    // app on localhost (a dev server on :3000) is a different origin.
    let host = h
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let origin_ok = h
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|o| o.split_once("://").is_some_and(|(_, rest)| rest == host));
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
        .route("/rules", get(api::rules_info).post(api::rules_save))
        .route("/rules/preview", post(api::rules_preview))
        .route("/commits", get(api::commits))
        .route("/outbox", get(api::outbox))
        .route("/live", get(api::live))
        .route("/limits", get(api::limits))
        .route("/lasted", get(api::lasted))
        .route("/turns", get(api::turns))
        .route("/approvals", get(api::approvals))
        .route("/thinking", get(api::thinking))
        .route("/prs", get(api::prs))
        .route("/sources", get(api::sources))
        .route("/skills", get(api::skills))
        .route("/compare", get(api::compare))
        .route("/compare/options", get(api::compare_options))
        .route("/follow", get(api::follow))
        .route("/blame", get(api::blame))
        .route("/setup", get(api::setup))
        .route("/evidence", get(api::evidence))
        .route("/reviews", get(api::reviews).post(api::reviews_save))
        .route("/findings/export", get(api::findings_export))
        .route("/prompts", get(api::prompts))
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
    async fn rules_preview_before_saving() {
        let app = router(state());
        let post = |body: serde_json::Value| {
            Request::post("/api/rules/preview")
                .header(header::HOST, "127.0.0.1:4747")
                .header(header::CONTENT_TYPE, "application/json")
                .header("origin", "http://127.0.0.1:4747")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        let yaml = "- id: tests-run\n  title: Ran the tests\n  severity: low\n  categories: [command]\n  pattern: 'npm\\s+test'\n";
        let res = send(&app, post(serde_json::json!({"yaml": yaml}))).await;
        assert_eq!(res.status(), StatusCode::OK);
        let p: serde_json::Value =
            serde_json::from_slice(&to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(
            p["rules"].as_array().unwrap().len(),
            1,
            "only the draft's rules: {p}"
        );
        assert_eq!(p["rules"][0]["rule_id"], "tests-run");
        let bad = send(&app, post(serde_json::json!({"yaml": "- id: x\n  title: x\n  severity: low\n  pattern: '('\n"}))).await;
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn reviews_mark_and_mute() {
        let app = router(state());
        let post = |body: &str, origin: &'static str| {
            Request::post("/api/reviews")
                .header(header::HOST, "127.0.0.1:4747")
                .header(header::CONTENT_TYPE, "application/json")
                .header("origin", origin)
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        let mute = r#"{"action":"mute","rule_id":"package-install","project":null,"note":"we install often"}"#;
        assert_eq!(
            send(&app, post(mute, "https://evil.example"))
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        let ok = send(&app, post(mute, "http://127.0.0.1:4747")).await;
        assert_eq!(ok.status(), StatusCode::OK);
        let r = json("/api/reviews").await;
        assert!(r["muted"].is_array());
        let res = send(
            &app,
            post(
                r#"{"action":"review","session_id":"s","seq":1,"rule_id":"rm-rf","note":null}"#,
                "http://127.0.0.1:4747",
            ),
        )
        .await;
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body["findings"][0]["rule_id"], "rm-rf");
        assert_eq!(body["muted"][0]["note"], "we install often");
        let bad = send(
            &app,
            post(r#"{"action":"explode"}"#, "http://127.0.0.1:4747"),
        )
        .await;
        assert!(bad.status().is_client_error());
        // The audit list carries the count of findings looked at.
        let a = json("/api/audit?status=all").await;
        assert!(a["reviewed"].is_number());
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
        // So is another app on localhost: same name, different port.
        let other = send(&app, post(rule, Some(("origin", "http://localhost:3000")))).await;
        assert_eq!(other.status(), StatusCode::FORBIDDEN);
        let other = send(&app, post(rule, Some(("origin", "http://127.0.0.1:3000")))).await;
        assert_eq!(other.status(), StatusCode::FORBIDDEN);
        // Pages can't be framed by another site.
        let page = send(
            &app,
            Request::get("/audit")
                .header(header::HOST, "127.0.0.1:4747")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(page.headers()[header::X_FRAME_OPTIONS], "DENY");
        let csp = page.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
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
        assert_eq!(
            t["matches"], 2,
            "`npm test` in the Claude and Codex fixtures: {t}"
        );

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
        let hooks = info["hooks"].as_array().unwrap();
        assert_eq!(hooks.len(), 2, "Claude Code and Codex");
        assert!(
            hooks
                .iter()
                .all(|h| h["snippet"].as_str().unwrap().contains("PreToolUse"))
        );

        // Rule editor: bad YAML is refused, good YAML saved and listed.
        let save = |yaml: &str| {
            Request::post("/api/rules")
                .header(header::HOST, "127.0.0.1:4747")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::json!({ "yaml": yaml }).to_string()))
                .unwrap()
        };
        let bad = send(&app, save("- id: x\n  title: x\n  severity: nope\n")).await;
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        let ok = save(
            "- id: deploy\n  title: Deployed\n  severity: high\n  categories: [command]\n  pattern: 'fly deploy'\n",
        );
        assert_eq!(send(&app, ok).await.status(), StatusCode::OK);
        let r = send(&app, get("/api/rules".into())).await;
        let r: serde_json::Value =
            serde_json::from_slice(&to_bytes(r.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert!(
            r["rules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x["id"] == "deploy")
        );
        assert!(r["user_yaml"].as_str().unwrap().contains("fly deploy"));
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
        let ob = json("/api/outbox").await;
        assert_eq!(ob["schema_version"], 3);
        assert!(ob["inventory"]["items"].is_array());
        assert!(ob["team"].is_null(), "not joined to a team");
        assert!(
            ob["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s.get("first_prompt").is_none())
        );
        let a = json("/api/audit").await;
        assert!(a["rules"].is_array() && a["findings"].is_array());
        assert!(json("/api/live?minutes=5").await.is_array());
        assert!(json("/api/prompts").await["prompts"].is_array());
        assert!(json("/api/setup").await["checks"].is_array());
        assert_eq!(json("/api/blame?path=/nope/x.rs").await["found"], false);
        let l = json("/api/limits").await;
        assert!(l["windows"].is_array() && l["hits"].is_array() && l["series"].is_array());
        let w = json("/api/lasted?range=all").await;
        assert!(w["settled"]["added"].is_number() && w["weeks"].is_array());
        for path in [
            "/api/turns",
            "/api/approvals",
            "/api/thinking?sort=longest",
            "/api/prs",
            "/api/sources",
            "/api/skills",
            "/api/compare/options",
            "/api/compare?dim=model&a=x&b=y",
            "/api/follow",
        ] {
            let v = json(path).await;
            assert!(v.is_object() || v.is_array(), "{path}");
        }
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
    async fn exports_a_session_as_markdown_and_json() {
        let id = "11111111-1111-4111-8111-111111111111";
        let res = get(&format!("/api/sessions/{id}/export")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let cd = res.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            cd.starts_with("attachment;") && cd.ends_with(".md\""),
            "{cd}"
        );
        let md = String::from_utf8(
            to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(md.contains("## You") && md.contains("The login form rejects valid passwords"));
        assert!(md.contains("```sh\nnpm test"), "{md}");
        assert!(md.contains("```diff") && md.contains("+export function check(p)"));
        assert!(md.contains(&format!("claude --resume {id}")));

        let j = json(&format!("/api/sessions/{id}/export?format=json")).await;
        assert_eq!(j["session"]["id"], id);
        assert!(j["steps"].as_array().unwrap().len() > 3);
        assert_eq!(j["subagents"].as_array().unwrap().len(), 1);

        let sub = get(&format!("/api/sessions/{id}/agent-a1/export?format=json")).await;
        assert_eq!(sub.status(), StatusCode::OK);
        assert_eq!(
            get("/api/sessions/missing/export").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(&format!("/api/sessions/{id}/export?format=pdf"))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn evidence_bundles_and_findings_export() {
        let id = "11111111-1111-4111-8111-111111111111";
        let ev = json(&format!("/api/evidence?session={id}")).await;
        assert_eq!(ev["format"], "agentcam-evidence");
        assert_eq!(ev["sessions"][0]["session"]["id"], id);
        assert_eq!(
            ev["sessions"].as_array().unwrap().len(),
            2,
            "the session and its subagent"
        );
        let text = serde_json::to_string(&ev).unwrap();
        assert_eq!(
            agentcam_core::evidence::verify(&text),
            agentcam_core::evidence::Verdict::Intact
        );
        assert_eq!(
            get("/api/evidence?session=missing").await.status(),
            StatusCode::NOT_FOUND
        );

        // The test logs hold no risky steps; the record shape is tested in core.
        let res = get("/api/findings/export?format=ocsf&range=all").await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            res.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("application/x-ndjson")
        );
        let body = String::from_utf8(
            to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        for line in body.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["class_uid"], 2004);
        }
        assert_eq!(
            get("/api/findings/export?format=csv").await.status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn limits_carry_a_claude_estimate() {
        // Fixture sessions are old, so nothing is open now, but the key is there.
        let l = json("/api/limits").await;
        assert!(l.get("claude").is_some());
        let live = json("/api/live?minutes=5").await;
        assert!(live.is_array());
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
