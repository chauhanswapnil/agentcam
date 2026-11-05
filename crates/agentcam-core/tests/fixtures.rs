//! Parse and index the synthetic logs in `fixtures/`.

use std::path::PathBuf;

use agentcam_core::model::{EventKind, FileAction, ToolCategory};
use agentcam_core::{Agent, Index, LogSource, claude, codex, discover};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn sources() -> Vec<LogSource> {
    vec![
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
    ]
}

const CLAUDE_ID: &str = "11111111-1111-4111-8111-111111111111";

fn claude_main() -> agentcam_core::ParsedSession {
    claude::parse(&fixtures().join(format!(
        "claude-code/projects/-Users-dev-projects-demo/{CLAUDE_ID}.jsonl"
    )))
    .unwrap()
}

#[test]
fn discovers_all_fixture_logs() {
    let files = discover(&sources());
    assert_eq!(files.len(), 4, "{files:#?}");
    assert_eq!(files.iter().filter(|(a, _)| *a == Agent::Codex).count(), 2);
}

#[test]
fn claude_session_metadata() {
    let p = claude_main();
    let s = &p.session;
    assert_eq!(s.id, CLAUDE_ID);
    assert_eq!(s.title.as_deref(), Some("Fix the login bug"));
    assert_eq!(s.project_path.as_deref(), Some("/Users/dev/projects/demo"));
    assert_eq!(s.git_branch.as_deref(), Some("main"));
    assert_eq!(s.model.as_deref(), Some("claude-test-1"));
    assert_eq!(s.cost_usd, Some(0.0123));
    assert_eq!(s.started_at.as_deref(), Some("2026-11-01T10:00:01.000Z"));
    assert_eq!(s.ended_at.as_deref(), Some("2026-11-01T10:01:00.100Z"));
    assert_eq!(
        s.first_prompt.as_deref(),
        Some("The login form rejects valid passwords. Please fix it.")
    );
    assert_eq!(p.bad_lines, 1);
}

#[test]
fn claude_counts_split_message_usage_once() {
    // msg_1 spans three records; usage must count once. Six messages:
    // 10+7+10+10+10+10 input, 5+30+5+5+5+5 output.
    let s = claude_main().session;
    assert_eq!(s.input_tokens, 57);
    assert_eq!(s.output_tokens, 55);
}

#[test]
fn claude_events_in_order() {
    let p = claude_main();
    let kinds: Vec<_> = p.events.iter().map(|e| e.kind).collect();
    use EventKind::*;
    assert_eq!(
        kinds,
        [
            Prompt, Thinking, Response, ToolCall, ToolCall, ToolCall, ToolCall, ToolCall, System,
            Response, Prompt, System
        ]
    );
    assert!(
        p.events
            .iter()
            .enumerate()
            .all(|(i, e)| e.seq as usize == i)
    );
}

#[test]
fn claude_tool_calls_merge_results() {
    let p = claude_main();
    let tools: Vec<_> = p
        .events
        .iter()
        .filter(|e| e.kind == EventKind::ToolCall)
        .collect();

    let bash = tools
        .iter()
        .find(|e| e.tool_name.as_deref() == Some("Bash"))
        .unwrap();
    assert_eq!(bash.category, Some(ToolCategory::Command));
    assert_eq!(bash.title, "npm test");
    assert!(bash.is_error);
    assert_eq!(bash.body, "1 failing: expected 200, got 401");
    assert_eq!(bash.duration_ms, Some(4000));

    let agent = tools
        .iter()
        .find(|e| e.category == Some(ToolCategory::Subagent))
        .unwrap();
    assert_eq!(agent.title, "Find other password checks");
    assert_eq!(agent.body, "Found one more in src/admin.ts");
}

#[test]
fn claude_file_touches_have_diffs() {
    let p = claude_main();
    let auth = "/Users/dev/projects/demo/src/auth.ts";
    let actions: Vec<_> = p
        .touches
        .iter()
        .filter(|t| t.path == auth)
        .map(|t| t.action)
        .collect();
    assert_eq!(actions, [FileAction::Read, FileAction::Edit]);

    let edit = p
        .touches
        .iter()
        .find(|t| t.action == FileAction::Edit)
        .unwrap();
    let diff = edit.diff.as_deref().unwrap();
    assert!(diff.starts_with("@@ -1,1 +1,1 @@\n-export"), "{diff}");

    let created = p
        .touches
        .iter()
        .find(|t| t.action == FileAction::Create)
        .unwrap();
    assert_eq!(created.path, "/Users/dev/projects/demo/.env.example");
    assert_eq!(
        created.diff.as_deref(),
        Some("API_KEY=sk-test-000000000000\n")
    );
}

#[test]
fn shell_commands_record_file_changes() {
    let dir = std::env::temp_dir().join(format!("agentcam-shell-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    let rec = serde_json::json!({
        "type": "assistant", "timestamp": "2026-11-01T10:00:00.000Z", "cwd": "/Users/dev/projects/demo",
        "sessionId": "s", "message": {"id": "m", "model": "x", "content": [
            {"type": "tool_use", "id": "t", "name": "Bash",
             "input": {"command": "cd web && rm -rf dist && sed -i '' 's/a/b/' src/x.ts"}}
        ]}
    });
    std::fs::write(&path, format!("{rec}\n")).unwrap();
    let p = claude::parse(&path).unwrap();
    let got: Vec<_> = p
        .touches
        .iter()
        .map(|t| (t.path.as_str(), t.action))
        .collect();
    assert_eq!(
        got,
        [
            ("/Users/dev/projects/demo/web/dist", FileAction::Delete),
            ("/Users/dev/projects/demo/web/src/x.ts", FileAction::Edit),
        ]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn claude_slash_commands_and_notices() {
    let p = claude_main();
    let cost = p.events.iter().find(|e| e.title == "/cost").unwrap();
    assert_eq!(cost.kind, EventKind::Prompt);
    let err = p.events.iter().find(|e| e.title == "api_error").unwrap();
    assert!(err.is_error);
    assert_eq!(err.body, "overloaded");
}

#[test]
fn claude_subagent_links_to_parent() {
    let path = fixtures().join(format!(
        "claude-code/projects/-Users-dev-projects-demo/{CLAUDE_ID}/subagents/agent-a1.jsonl"
    ));
    let p = claude::parse(&path).unwrap();
    assert_eq!(p.session.id, format!("{CLAUDE_ID}/agent-a1"));
    assert_eq!(p.session.parent_id.as_deref(), Some(CLAUDE_ID));
    assert_eq!(
        p.session.title.as_deref(),
        Some("Find other password checks")
    );
    let grep = p
        .events
        .iter()
        .find(|e| e.category == Some(ToolCategory::Search))
        .unwrap();
    assert_eq!(grep.title, "password");
}

#[test]
fn codex_item_format() {
    let path = fixtures().join(
        "codex/sessions/2026/11/01/rollout-2026-11-01T12-00-00-22222222-2222-4222-8222-222222222222.jsonl",
    );
    let p = codex::parse(&path).unwrap();
    let s = &p.session;
    assert_eq!(s.id, "22222222-2222-4222-8222-222222222222");
    assert_eq!(s.model.as_deref(), Some("gpt-test-1"));
    assert_eq!(s.git_branch.as_deref(), Some("feature/rate-limit"));
    assert_eq!(
        (s.input_tokens, s.cache_read_tokens, s.output_tokens),
        (200, 800, 50)
    );
    assert_eq!(
        s.first_prompt.as_deref(),
        Some("Add rate limiting to the login route")
    );

    use EventKind::*;
    let kinds: Vec<_> = p.events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [
            Prompt, Thinking, Response, ToolCall, ToolCall, ToolCall, ToolCall, ToolCall, Response
        ]
    );

    let cat = &p.events[3];
    assert_eq!(
        (cat.title.as_str(), cat.duration_ms),
        ("cat src/routes.ts", Some(12))
    );
    let test = &p.events[5];
    assert!(test.is_error);
    assert_eq!(test.duration_ms, Some(3500));
    assert_eq!(p.events[6].category, Some(ToolCategory::Web));
    assert_eq!(p.events[7].body, "Use a token bucket.");

    let change = &p.events[4];
    assert_eq!(change.category, Some(ToolCategory::Edit));
    let mut touches: Vec<_> = p
        .touches
        .iter()
        .filter(|t| t.seq == change.seq)
        .map(|t| t.action)
        .collect();
    touches.sort_by_key(|a| a.as_str());
    assert_eq!(touches, [FileAction::Create, FileAction::Edit]);
    assert!(
        p.touches
            .iter()
            .any(|t| t.action == FileAction::Read && t.path == "src/routes.ts")
    );
}

#[test]
fn codex_legacy_format() {
    let path = fixtures().join(
        "codex/sessions/2026/10/01/rollout-2026-10-01T09-00-00-33333333-3333-4333-8333-333333333333.jsonl",
    );
    let p = codex::parse(&path).unwrap();
    use EventKind::*;
    let kinds: Vec<_> = p.events.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [Prompt, Thinking, ToolCall, ToolCall, Response]);

    let grep = &p.events[2];
    assert_eq!(grep.category, Some(ToolCategory::Command));
    assert_eq!(grep.title, "grep -rn verbose src");
    assert_eq!(grep.body, "src/main.rs:10: verbose: bool");

    let patch = &p.events[3];
    assert_eq!(patch.category, Some(ToolCategory::Edit));
    assert_eq!(patch.title, "src/main.rs");
    assert_eq!(p.touches.len(), 1);
    assert_eq!(p.touches[0].action, FileAction::Edit);
}

#[test]
fn index_is_incremental_and_searchable() {
    let mut idx = Index::open_in_memory().unwrap();
    let first = idx.update(&sources(), false).unwrap();
    assert_eq!(
        (first.files_seen, first.files_parsed, first.files_failed),
        (4, 4, 0)
    );

    let again = idx.update(&sources(), false).unwrap();
    assert_eq!(
        again.files_parsed, 0,
        "unchanged files must not be re-parsed"
    );

    let full = idx.update(&sources(), true).unwrap();
    assert_eq!(full.files_parsed, 4);

    let sessions: i64 = idx
        .conn()
        .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sessions, 4);
    let fts: i64 = idx
        .conn()
        .query_row("SELECT count(*) FROM events_fts", [], |r| r.get(0))
        .unwrap();
    let events: i64 = idx
        .conn()
        .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fts, events, "re-indexing must not leave stale search rows");

    let hits = idx.search("rate limiting", 10).unwrap();
    assert!(
        hits.iter()
            .any(|h| h.session_id == "22222222-2222-4222-8222-222222222222"),
        "{hits:#?}"
    );
    let hits = idx.search("passwords", 10).unwrap();
    assert!(hits.iter().all(|h| h.session_id.starts_with(CLAUDE_ID)));
    assert!(!hits.is_empty());

    let (files, parent): (i64, Option<String>) = idx
        .conn()
        .query_row(
            "SELECT file_count, parent_id FROM sessions WHERE id = ?",
            [format!("{CLAUDE_ID}/agent-a1")],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((files, parent.as_deref()), (0, Some(CLAUDE_ID)));
}

mod queries {
    use super::*;
    use agentcam_core::query::{self, Filter};

    fn index() -> Index {
        let mut idx = Index::open_in_memory().unwrap();
        idx.update(&sources(), false).unwrap();
        idx
    }

    #[test]
    fn stats_count_subagents_toward_their_parent() {
        let idx = index();
        let all = query::stats(idx.conn(), &Filter::default()).unwrap();
        // Claude main + subagent count once; two Codex sessions.
        assert_eq!(all.sessions, 3);
        assert_eq!(all.projects, 3);
        assert!(all.active_ms > 0);
        assert_eq!(all.errors, 3, "{all:?}");

        let codex = query::stats(
            idx.conn(),
            &Filter {
                agent: Some("codex".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            (codex.sessions, codex.files_changed, codex.files_created),
            (2, 3, 1)
        );
    }

    #[test]
    fn active_time_caps_idle_gaps() {
        let idx = index();
        let ms: i64 = idx
            .conn()
            .query_row(
                "SELECT active_ms FROM sessions WHERE id = ?",
                [CLAUDE_ID],
                |r| r.get(0),
            )
            .unwrap();
        // Steps run 10:00:01 -> 10:01:00.100, no gap over the 5 minute cap.
        assert_eq!(ms, 59_100);
    }

    #[test]
    fn range_filters_by_step_time() {
        let idx = index();
        let f = Filter {
            since: Some("2026-11-01T00:00:00.000Z".into()),
            until: Some("2026-11-02T00:00:00.000Z".into()),
            ..Default::default()
        };
        let page = query::sessions(idx.conn(), &f, 50, 0).unwrap();
        assert_eq!(page.total, 2);
        let ids: Vec<_> = page.items.iter().map(|s| s.id.as_str()).collect();
        // Codex session (12:00) is more recent than Claude (10:00).
        assert_eq!(ids, ["22222222-2222-4222-8222-222222222222", CLAUDE_ID]);
        assert_eq!(page.items[1].subagents, 1);
        assert!(page.items.iter().all(|s| s.parent_id.is_none()));
    }

    #[test]
    fn activity_and_projects() {
        let idx = index();
        let days = query::activity(idx.conn(), &Filter::default()).unwrap();
        assert_eq!(days.iter().map(|d| d.events).sum::<i64>(), {
            idx.conn()
                .query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
                .unwrap()
        });

        let projects = query::projects(
            idx.conn(),
            &Filter::default(),
            4,
            "2026-11-08T00:00:00.000Z",
        )
        .unwrap();
        let demo = projects
            .iter()
            .find(|p| p.path == "/Users/dev/projects/demo")
            .unwrap();
        assert_eq!(demo.sessions, 1);
        assert_eq!(demo.agents, ["claude-code"]);
        assert_eq!(demo.weekly_ms.len(), 4);
        assert_eq!(
            demo.weekly_ms[3], demo.active_ms,
            "all activity is in the last week"
        );
    }

    #[test]
    fn session_detail_lists_steps_and_subagents() {
        let idx = index();
        let d = query::session(idx.conn(), CLAUDE_ID).unwrap().unwrap();
        assert_eq!(d.events.len(), 12);
        assert_eq!(d.subagents.len(), 1);
        assert_eq!(d.session.title.as_deref(), Some("Fix the login bug"));
        assert!(query::session(idx.conn(), "nope").unwrap().is_none());
    }
}
