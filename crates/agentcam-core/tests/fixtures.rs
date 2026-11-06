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

#[test]
fn search_finds_steps_with_filters() {
    use agentcam_core::query::{self, Filter};
    let mut idx = Index::open_in_memory().unwrap();
    idx.update(&sources(), false).unwrap();
    let c = idx.conn();
    let hits = query::search(c, "rate limit", &Filter::default(), None, None, 20).unwrap();
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| h.agent == "codex"));
    assert!(hits.iter().any(|h| h.snippet.contains('\u{1}')));
    // Prefix match on the last word, and odd characters don't break it.
    assert!(
        !query::search(c, "passw", &Filter::default(), None, None, 20)
            .unwrap()
            .is_empty()
    );
    assert!(query::search(c, "\"-:*", &Filter::default(), None, None, 20).is_ok());
    let cmds = query::search(c, "npm", &Filter::default(), None, Some("command"), 20).unwrap();
    assert!(
        !cmds.is_empty()
            && cmds
                .iter()
                .all(|h| h.category.as_deref() == Some("command"))
    );
}

#[test]
fn audit_tools_and_files_views() {
    use agentcam_core::query::{self, Filter};
    // A throwaway Claude log with risky steps, next to the fixtures.
    let dir = std::env::temp_dir().join(format!("agentcam-audit-{}", std::process::id()));
    let proj = dir.join("projects/-Users-dev-app");
    std::fs::create_dir_all(&proj).unwrap();
    let rec = |ts: &str, id: &str, cmd: &str| {
        serde_json::json!({
            "type": "assistant", "timestamp": ts, "cwd": "/Users/dev/app", "sessionId": "risky",
            "message": {"id": id, "model": "x", "content": [
                {"type": "tool_use", "id": id, "name": "Bash", "input": {"command": cmd}}
            ]}
        })
    };
    let lines = [
        rec(
            "2026-11-01T09:00:00.000Z",
            "a",
            "rm -rf dist && git push --force",
        ),
        rec(
            "2026-11-01T09:00:01.000Z",
            "b",
            "export GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789",
        ),
        rec(
            "2026-11-01T09:00:02.000Z",
            "c",
            "echo hi > /Users/dev/other/x.txt",
        ),
    ];
    let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(proj.join("risky.jsonl"), body).unwrap();

    let mut idx = Index::open_in_memory().unwrap();
    let mut srcs = sources();
    srcs.push(LogSource {
        agent: Agent::ClaudeCode,
        path: dir.join("projects"),
        exists: true,
    });
    idx.update(&srcs, false).unwrap();
    let c = idx.conn();

    let rules = query::audit_rules(c, &Filter::default()).unwrap();
    let ids: Vec<_> = rules.iter().map(|r| r.rule_id.as_str()).collect();
    for want in [
        "secret-github-token",
        "git-force-push",
        "rm-rf",
        "write-outside-project",
    ] {
        assert!(ids.contains(&want), "{want} missing from {ids:?}");
    }
    assert_eq!(rules[0].severity, "critical", "worst first");

    let hits =
        query::findings(c, &Filter::default(), Some("secret-github-token"), None, 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(
        !hits[0].excerpt.contains("ijklmnop"),
        "secret must be masked: {}",
        hits[0].excerpt
    );
    let serious = query::findings(c, &Filter::default(), None, Some("high"), 50).unwrap();
    assert!(
        serious
            .iter()
            .all(|h| h.severity == "high" || h.severity == "critical")
    );

    let risky = query::session(c, "risky").unwrap().unwrap();
    assert_eq!(risky.session.max_severity.as_deref(), Some("critical"));
    assert!(risky.findings.len() >= 4);
    assert!(
        query::stats(c, &Filter::default())
            .unwrap()
            .serious_findings
            >= 2
    );

    let tools = query::tools(c, &Filter::default()).unwrap();
    let bash = tools.iter().find(|t| t.tool_name == "Bash").unwrap();
    assert!(bash.calls >= 4 && bash.errors >= 1);
    assert!(
        !query::tool_calls(c, &Filter::default(), "Bash", 10)
            .unwrap()
            .is_empty()
    );

    let files = query::files(c, &Filter::default(), 100).unwrap();
    let auth = files
        .iter()
        .find(|f| f.path.ends_with("src/auth.ts"))
        .unwrap();
    assert_eq!((auth.reads, auth.edits), (1, 1));
    let hist = query::file_history(c, "/Users/dev/projects/demo/src/auth.ts", 10).unwrap();
    assert_eq!(hist.len(), 2);
    assert!(hist.iter().all(|h| h.action.is_some()));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn otel_export_nests_subagents() {
    use agentcam_core::query::Filter;
    let mut idx = Index::open_in_memory().unwrap();
    idx.update(&sources(), false).unwrap();
    let (body, n) = agentcam_core::otel::export(idx.conn(), &Filter::default()).unwrap();
    assert_eq!(n, 3, "three top-level sessions");
    let spans = body["resourceSpans"][0]["scopeSpans"][0]["spans"]
        .as_array()
        .unwrap();
    let sub = spans
        .iter()
        .find(|s| s["name"] == "Find other password checks" && s.get("parentSpanId").is_some())
        .unwrap();
    let agent_call = spans.iter().find(|s| s["name"] == "Agent").unwrap();
    assert_eq!(
        sub["parentSpanId"], agent_call["spanId"],
        "subagent nests under its Agent call"
    );
    assert_eq!(sub["traceId"], agent_call["traceId"]);
    assert!(
        spans.iter().any(|s| s["status"]["code"] == 2),
        "failed steps are errors"
    );
}

#[test]
fn outbox_sends_metadata_only() {
    use agentcam_core::query::Filter;
    let mut idx = Index::open_in_memory().unwrap();
    idx.update(&sources(), false).unwrap();
    let out = agentcam_core::share::outbox(idx.conn(), &Filter::default(), 50).unwrap();
    assert_eq!(out.len(), 4, "every session, subagents included");
    let json = serde_json::to_value(&out).unwrap();
    let text = json.to_string();

    // Allowed keys only, at every level.
    fn keys(v: &serde_json::Value, path: &str, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    out.push(p.clone());
                    keys(v, &p, out);
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|v| keys(v, path, out)),
            _ => {}
        }
    }
    let mut all = Vec::new();
    for s in json.as_array().unwrap() {
        keys(s, "", &mut all);
    }
    for k in &all {
        let leaf = k.rsplit('.').next().unwrap();
        assert!(
            ![
                "body",
                "title",
                "first_prompt",
                "text",
                "input",
                "diff",
                "project_path",
                "source_path"
            ]
            .contains(&leaf),
            "content field leaked: {k}"
        );
    }
    // No prompt text, command output or absolute paths.
    assert!(
        !text.contains("rejects valid passwords"),
        "prompt text leaked"
    );
    assert!(
        !text.contains("expected 200, got 401"),
        "command output leaked"
    );
    assert!(
        !text.contains("/Users/dev/projects/demo/src"),
        "absolute path leaked"
    );
    assert!(
        text.contains("\"src/auth.ts\""),
        "paths are relative to the project"
    );
    assert!(!text.contains("/Users/"), "no home folders anywhere");
}

#[test]
fn limits_from_both_agents() {
    let c = claude::parse(&fixtures().join("claude-code/session-limit-hit.jsonl")).unwrap();
    assert_eq!(c.session.entrypoint.as_deref(), Some("cli"));
    assert_eq!(c.limits.len(), 1);
    let hit = &c.limits[0];
    assert!(hit.hit);
    assert_eq!(hit.window, "session");
    assert_eq!(hit.resets_text.as_deref(), Some("3pm (Europe/London)"));
    let e = &c.events[hit.seq.unwrap() as usize];
    assert_eq!(
        e.kind,
        EventKind::System,
        "a limit notice is not the agent's reply"
    );
    assert!(!e.is_error);

    let x = codex::parse(&fixtures().join(
        "codex/sessions/2026/11/01/rollout-2026-11-01T12-00-00-22222222-2222-4222-8222-222222222222.jsonl",
    ))
    .unwrap();
    assert_eq!(x.session.entrypoint.as_deref(), Some("codex_cli"));
    // 5h at 42% then 100%; the weekly reading didn't change, so it's kept once.
    let five: Vec<_> = x.limits.iter().filter(|l| l.window == "5h").collect();
    let week: Vec<_> = x.limits.iter().filter(|l| l.window == "weekly").collect();
    assert_eq!(five.len(), 2);
    assert_eq!(week.len(), 1);
    assert_eq!(five[0].used_percent, Some(42.0));
    assert!(!five[0].hit && five[1].hit);
    assert_eq!(five[0].plan.as_deref(), Some("plus"));
    assert_eq!(
        five[0].resets_at.as_deref(),
        Some("2026-11-01T18:00:00.000Z")
    );

    let mut idx = Index::open_in_memory().unwrap();
    idx.update(&sources(), false).unwrap();
    let now = agentcam_core::util::parse_ts_ms("2026-11-01T13:00:00.000Z").unwrap();
    let l = agentcam_core::live::limits(idx.conn(), now).unwrap();
    let w5 = l.windows.iter().find(|w| w.window == "5h").unwrap();
    assert_eq!(w5.used_percent, 100.0);
    assert!(!w5.expired);
    assert_eq!(l.hits.iter().filter(|h| h.agent == "codex").count(), 1);

    let later = agentcam_core::util::parse_ts_ms("2026-11-01T12:05:00.000Z").unwrap();
    let live = agentcam_core::live::live(idx.conn(), later, 15 * 60_000).unwrap();
    assert_eq!(
        live.len(),
        1,
        "only the Codex session ran in the last 15 minutes"
    );
    assert_eq!(live[0].session.agent, "codex");
    assert!(!live[0].recent.is_empty());
}

/// A throwaway project and Claude Code log, so survival can read real files.
fn survival_world(name: &str) -> (PathBuf, Vec<LogSource>) {
    let dir = std::env::temp_dir().join(format!("agentcam-survival-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = dir.join("project");
    std::fs::create_dir_all(project.join("src")).unwrap();
    let file = project.join("src/app.rs");
    let path = file.to_string_lossy().to_string();
    let cwd = project.to_string_lossy().to_string();
    let rec = |ty: &str, ts: &str, extra: serde_json::Value| {
        let mut v = serde_json::json!({
            "sessionId": "55555555-5555-4555-8555-555555555555", "cwd": cwd, "type": ty,
            "timestamp": ts, "uuid": ts, "userType": "external"
        });
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        v.to_string()
    };
    let created = "fn alpha() {}\nfn beta() {}\nfn gamma() {}\nfn delta() {}\n";
    let lines = [
        rec(
            "user",
            "2026-10-01T09:00:00.000Z",
            serde_json::json!({"message": {"role": "user", "content": "Add the app"}}),
        ),
        rec(
            "assistant",
            "2026-10-01T09:00:01.000Z",
            serde_json::json!({"message": {"id": "m1", "role": "assistant", "model": "claude-test-1",
            "content": [{"type": "tool_use", "id": "t1", "name": "Write", "input": {"file_path": path, "content": created}}]}}),
        ),
        rec(
            "user",
            "2026-10-01T09:00:02.000Z",
            serde_json::json!({"message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]},
            "toolUseResult": {"type": "create", "filePath": path, "content": created, "structuredPatch": []}}),
        ),
        rec(
            "assistant",
            "2026-10-01T09:00:03.000Z",
            serde_json::json!({"message": {"id": "m2", "role": "assistant", "model": "claude-test-1",
            "content": [{"type": "tool_use", "id": "t2", "name": "Edit", "input": {"file_path": path, "old_string": "beta", "new_string": "beta2"}}]}}),
        ),
        rec(
            "user",
            "2026-10-01T09:00:04.000Z",
            serde_json::json!({"message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t2", "content": "ok"}]},
            "toolUseResult": {"filePath": path, "structuredPatch": [{"oldStart": 2, "oldLines": 1, "newStart": 2, "newLines": 1,
                "lines": ["-fn beta() {}", "+fn beta2() {}"]}]}}),
        ),
    ];
    let logs = dir.join("logs/-project");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(
        logs.join("55555555-5555-4555-8555-555555555555.jsonl"),
        lines.join("\n"),
    )
    .unwrap();
    // Today: someone else removed `delta`.
    std::fs::write(&file, "fn alpha() {}\nfn beta2() {}\nfn gamma() {}\n").unwrap();
    let sources = vec![LogSource {
        agent: Agent::ClaudeCode,
        path: dir.join("logs"),
        exists: true,
    }];
    (dir, sources)
}

#[test]
fn survival_checks_agent_lines_against_files() {
    use agentcam_core::survival::{Counts, session_counts};
    let (dir, sources) = survival_world("lines");
    let mut idx = Index::open_in_memory().unwrap();
    idx.update(&sources, false).unwrap();
    let id = "55555555-5555-4555-8555-555555555555";
    assert_eq!(
        session_counts(idx.conn(), id).unwrap(),
        Some(Counts {
            added: 5,
            kept: 3,
            reworked: 1,
            changed: 1
        }),
        "alpha, gamma, beta2 kept; beta rewritten by the agent; delta changed since"
    );
    let step = agentcam_core::query::step(idx.conn(), id, 2)
        .unwrap()
        .unwrap();
    assert_eq!(step.touches[0].lasted.unwrap().kept, 1);

    // Someone edits the file: the next run sees it without the logs changing.
    std::fs::write(
        dir.join("project/src/app.rs"),
        "fn beta2() {}\n// rewritten by hand\n",
    )
    .unwrap();
    idx.update(&sources, false).unwrap();
    let c = session_counts(idx.conn(), id).unwrap().unwrap();
    assert_eq!((c.kept, c.changed), (1, 3));

    let now = agentcam_core::util::parse_ts_ms("2026-11-01T00:00:00.000Z").unwrap();
    let r =
        agentcam_core::survival::report(idx.conn(), &agentcam_core::query::Filter::default(), now)
            .unwrap();
    assert_eq!(r.settled.added, 5);
    assert_eq!(r.ages[2].counts.added, 5, "a month old");
    assert_eq!(r.by_agent.len(), 1);

    // The project is gone: nothing to say, rather than "nothing lasted".
    std::fs::remove_dir_all(dir.join("project")).unwrap();
    idx.update(&sources, false).unwrap();
    assert_eq!(session_counts(idx.conn(), id).unwrap(), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn old_schema_is_replaced_whatever_tables_it_had() {
    let dir = std::env::temp_dir().join(format!("agentcam-schema-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("index.db");
    let _ = std::fs::remove_file(&db);
    {
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch(
            "CREATE TABLE limits (x); CREATE TABLE someday_table (y);
             CREATE VIRTUAL TABLE events_fts USING fts5(title);
             PRAGMA user_version = 1;",
        )
        .unwrap();
    }
    let mut idx = Index::open(&db).unwrap();
    idx.update(&sources(), false).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}
