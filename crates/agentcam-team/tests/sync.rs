//! A laptop joins a running team server and syncs the fixture sessions.

use std::path::PathBuf;

use agentcam_core::{Agent, Index, LogSource};
use agentcam_team::{client, server, store::Store};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

#[tokio::test(flavor = "multi_thread")]
async fn laptop_joins_and_syncs_only_changes() {
    let dir = std::env::temp_dir().join(format!("agentcam-team-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let store = Store::open_in_memory().unwrap();
    let team = store.team("Platform").unwrap();
    let invite = store.create_invite(team, 5, 7).unwrap();
    let admin = store.create_viewer("Admin", "admin").unwrap();
    let state = server::TeamState::new(store);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, server::router(state)).await.unwrap();
    });

    let db = dir.join("index.db");
    let u = url.clone();
    let d = dir.clone();
    let (first, second, status) = tokio::task::spawn_blocking(move || {
        let mut idx = Index::open(&db).unwrap();
        idx.update(
            &[
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
            ],
            false,
        )
        .unwrap();
        // Not joined: nothing is sent.
        assert!(client::sync(idx.conn(), &db, &d, "test").is_err());
        let link = client::join(
            &db,
            &u,
            &invite,
            idx.machine_id(),
            Some("dev".into()),
            false,
        )
        .unwrap();
        assert_eq!(link.team, "Platform");
        let first = client::sync(idx.conn(), &db, &d, "test").unwrap();
        let second = client::sync(idx.conn(), &db, &d, "test").unwrap();
        (first, second, client::status(&db))
    })
    .await
    .unwrap();

    assert!(first.sent > 0 && first.written == first.sent, "{first:?}");
    assert_eq!(second.sent, 0, "unchanged sessions aren't sent again");
    assert!(status.joined && status.last_error.is_none());
    assert_eq!(status.sessions_on_server, first.sent);

    // The server now shows them.
    let ins: serde_json::Value = tokio::task::spawn_blocking(move || {
        let r = ureq::get(&format!("{url}/api/team/insights?range=all"))
            .set("Authorization", &format!("Bearer {admin}"))
            .call()
            .unwrap();
        serde_json::from_str(&r.into_string().unwrap()).unwrap()
    })
    .await
    .unwrap();
    assert!(ins["totals"]["sessions"].as_i64().unwrap() > 0, "{ins}");
    // The link file keeps the token from other users.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(client::link_path(&dir.join("index.db")))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
