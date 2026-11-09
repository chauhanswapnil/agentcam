# AGENTS.md

Guidance for AI coding agents (and humans) working in this repo.

## What this is

agentcam is a local-first, open-source tool that reads the session logs coding agents write (Claude Code, Codex) and shows them as an overview, a session replay and search. Later: a risk audit with YAML rules. It is one Rust binary that serves an embedded React UI on `127.0.0.1`.

Product rules that shape every change:

- **Read only.** Never write to, move or delete an agent's log files.
- **Local only.** Bind to `127.0.0.1`. No network calls, no telemetry, no accounts. The one exception is opt-in: after `agentcam team join`, the laptop pushes metadata (never content) to a team server the company runs itself.
- **Zero setup.** Work from the logs that already exist; no hooks or SDK required.
- **The UI is the product.** Visual polish is a feature, not a nice-to-have.
- **Healthy skepticism.** Assume the agent is capable and acting in good faith. Show what happened clearly; flag only what deserves a second look, in neutral words; never hinder the agent by default.

### Voice

agentcam is a flight recorder, not a cop. It helps people understand and trust their agents' work; it doesn't grade or police the models. When writing UI text, docs, rule titles or finding messages:

- **Describe, don't accuse.** "Test expectations changed after a failing run" not "agent tampered with tests". "Retried 3 times" not "failed 3 times".
- **Errors are part of the work.** Say "steps that errored" or "retries", not "failures" or "wasted time".
- **Point to a next step for the person,** not a verdict on the agent: "worth a second look", "you may want to review".
- **Show improvements as plainly as problems.** A comparison that only surfaces regressions is a scorecard, not observability.
- **Keep severity honest.** A leaked key is still critical; calm wording doesn't mean hiding real risk.
- **Guard rules ask, they don't punish.** Default messages read "check with the user before doing this", and the guard never stops an agent unless the user set a rule.

## Layout

```
crates/
  agentcam-core/     parsers, event model, index (no I/O beyond reading logs + SQLite)
  agentcam-server/   axum server: JSON API under /api, embedded UI for everything else
    ui-dist/         built web UI (generated, gitignored, embedded with rust-embed)
  agentcam-team/     self-hosted team server (store, sync API, team views) and the laptop's join/sync client
  agentcam/          the CLI binary (clap); published as `agentcam` on crates.io
web/                 React + Vite + TypeScript UI; builds into crates/agentcam-server/ui-dist
  src/styles/        design tokens and base CSS
  src/ui/            shared components (one CSS module each)
  src/pages/         routed pages, including /styleguide
  src/lib/           event kinds, agents, formatting
  src/team/          the team server's pages, under /team (same build)
fixtures/            synthetic agent logs for tests (never real ones, see below)
docs/                user guides (team server, MCP) and README screenshots
```

Dependencies flow one way: `agentcam` → `agentcam-team` → `agentcam-server` → `agentcam-core`. Core must not depend on the server, the team crate or the CLI. Team code stays in `agentcam-team` so it can be licensed apart later.

## Commands

```sh
# Rust
cargo build
cargo test
cargo fmt --all
cargo clippy --all-targets -- -D warnings

# Web (from web/)
npm ci
npm run build      # writes into crates/agentcam-server/ui-dist
npm run lint
npm run dev        # Vite on :5173, proxies /api to :4747

# Run the app
cargo run                         # serve + open browser on :4747
cargo run -- serve --no-open      # API only, for `npm run dev`
cargo run -- index                # index new/changed logs, print a summary
cargo run -- index --full         # re-parse every log
cargo run -- search "some words"  # full-text search from the terminal
cargo run -- blame path/to/file   # which agent session wrote each line
cargo run -- mcp                  # MCP server on stdio (JSON-RPC, one message per line)
cargo run -- evidence --range 7d  # evidence bundle; `evidence verify <file>` checks one
cargo run -- export findings --format ocsf   # findings for a SIEM, one per line
```

The index lives in the user data folder (`~/Library/Application Support/agentcam/index.db` on macOS). Use `--db <file>` or `AGENTCAM_DB` to point at a scratch index while developing.

Demo data (fake sessions, for screenshots or trying the UI without real logs): `python3 scripts/demo-data.py /tmp/agentcam-demo`, then run with `HOME=/tmp/agentcam-demo` and `--db /tmp/agentcam-demo/index.db` (add `--port 4848` to keep a real instance on 4747). It also writes fake project files, AGENTS.md files and skill folders under that directory, so What lasted, Instructions and Skills have something to read, and one session that is still running for Live. README screenshots in `docs/screenshots/` must come from demo data, never real logs; retake them when a page changes (headless Chrome at 1440 wide, 2× scale). When you add a feature, extend the demo data so it shows.

UI dev loop: run `cargo run -- serve --no-open` in one terminal and `npm run dev` in `web/` in another, then open the Vite URL.

`build.rs` in `agentcam-server` writes a placeholder `ui-dist/index.html` if the UI hasn't been built, so `cargo build` always works. Run `npm run build` before a release build or the binary ships the placeholder.

## Before you finish a change

1. `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` are clean.
2. `cargo test` passes.
3. If you touched `web/`: `npm run lint` and `npm run build` pass.
4. New behavior has a test. Parser changes have a fixture that shows the case.
5. UI changes: open the app and look at them. Screenshots in the PR help.
6. Install-path changes: `scripts/smoke.sh target/debug/agentcam` runs a built binary against the fixtures (index, search, guard, serve). CI runs it on macOS, Linux and Windows after `cargo install`, and checks `cargo package --workspace`.

## Privacy rules (important)

Agent logs hold real secrets, file contents and private prompts.

- **Never commit real session logs**, excerpts of them, or indexes built from them. Not in fixtures, tests, docs, issues or screenshots.
- Fixtures in `fixtures/` are hand-written and synthetic. Paths use `/Users/dev/...`; secrets are obviously fake (`sk-test-0000...`).
- Don't log full event payloads at `info` level. Use `debug` or `trace`.

## API

The server (`crates/agentcam-server`) serves JSON under `/api`. Queries live in `agentcam-core/src/query.rs`; handlers in `agentcam-server/src/api.rs` stay thin.

| Route | What it returns |
|---|---|
| `GET /api/status` | Index state; `generation` goes up after each run, so the UI refetches; `has_sessions` and `folders` (log folders and whether each exists) drive the first-run screen |
| `POST /api/index` | Start indexing new and changed logs (202, or 409 if already running) |
| `GET /api/overview` | Stats, the previous period's stats, and projects with weekly sparklines |
| `GET /api/activity` | Agent time per local day for the last 371 days |
| `GET /api/sessions` | Top-level sessions, most recently active first, paged |
| `GET /api/sessions/{id}` | One session: steps (titles, plus the start of transcript text), files touched, subagents |
| `GET /api/sessions/{id}/export?format=md\|json` | One session as a download (Markdown or JSON), secrets masked |
| `GET /api/step?session=&seq=` | One step in full: output, input, file changes with diffs |
| `GET /api/audit` | Rules that fired and the latest findings; takes `rule`, `severity` (lowest to show), `status` (`open` hides findings marked looked at or muted, `all`) |
| `GET/POST /api/reviews` | Findings marked "looks fine" and rules muted per project or everywhere (`reviews.rs`, kept in `reviews.json`) |
| `POST /api/rules/preview` | What draft rules would find in recent sessions, before saving (`preview.rs`) |
| `GET /api/tools`, `GET /api/tool-calls?tool=` | Tool use across sessions, and the latest calls of one tool |
| `GET /api/files`, `GET /api/file?path=` | Files touched, most changed first, and one file's history |
| `GET/POST /api/guard`, `DELETE /api/guard/{id}` | Guard rules and hook setup; `GET /api/guard/test` counts past matches, `GET /api/guard/suggest` proposes a pattern |
| `GET /api/commits` | Commits made by agent sessions, newest first |
| `GET /api/live?minutes=` | Top-level sessions with a step in the last 15 minutes: state, latest steps, context, error streak, cost |
| `GET /api/limits` | Usage windows (latest reading, last 7 days), limits reached, and `claude`: Claude Code's 5-hour blocks estimated from tokens |
| `GET /api/lasted` | Which agent-written lines are still in the code: by age, week, agent and model, project, file |
| `GET /api/turns` | Turn lengths, waits between turns, turns by model, effort and mode, longest turns |
| `GET /api/approvals` | How tool calls were allowed or stopped, stopped calls, allow rules to suggest |
| `GET /api/thinking?sort=` | Thinking steps (`longest` or newest) and thinking time by model |
| `GET /api/prs` | Pull requests sessions linked |
| `GET /api/sources` | Skills, plugins, MCP servers and apps behind steps |
| `GET /api/skills` | Skills installed, listed and used, with sizes and latest runs |
| `GET /api/compare/options`, `GET /api/compare?dim=&a=&b=&at=` | Values to compare by, comparisons found in the data, and two groups of turns side by side |
| `GET /api/follow` | Each project's AGENTS.md / CLAUDE.md rules and how sessions followed them |
| `GET /api/blame?path=` | Each line of a file today and the agent edit that wrote it (`survival::blame`) |
| `GET /api/setup` | Settings, memory files, MCP servers, hooks and skills on this machine, with checks (`setup.rs`) |
| `GET /api/prompts?q=` | Prompts you typed, newest first, and the ones repeated across sessions (`prompts.rs`) |
| `GET /api/outbox` | The metadata a team server receives (`share.rs`), the setup inventory (`fleet.rs`) and the team link, if any |
| `GET /api/search?q=` | Full-text matches, best first; takes `range`, `agent`, `project`, `kind` (`risky`: only steps with findings), `category`, `limit` |
| `GET /api/evidence?session=` or `?range=` | An evidence bundle: JSON with a SHA-256 checksum, secrets masked (`evidence.rs`) |
| `GET /api/findings/export?format=ndjson\|ocsf` | Findings one per line for a SIEM; takes `range`, `severity` (`siem.rs`) |

List routes take `range` (`7d`, `30d`, `90d`, `all`), `agent`, `project` (a project root) and `day` (local `YYYY-MM-DD`).

- **Agent time** is the sum of gaps between steps, each capped at 5 minutes, so idle stretches don't count. It's stored per step (`events.gap_ms`) at index time.
- **Project root** folds agent worktrees (`repo/.claude/worktrees/x`) into the repo they belong to.
- Subagent sessions count toward their parent and are hidden from top-level lists.
- `agentcam serve` indexes in the background on start, using a separate SQLite connection so reads stay fast.
- It also watches the log folders (`notify`) and re-indexes 750 ms after agents stop writing. The UI polls `/api/status` and refetches when `generation` changes.
- Search input is turned into a safe FTS5 query (`query::fts_query`): every word quoted, the last one as a prefix. Never pass user text to `MATCH` directly.

### Risk audit

- Rules live in `crates/agentcam-core/rules/default.yaml` (built in) and `rules.yaml` next to the index, which users edit on the Audit page (`GET/POST /api/rules`; saving re-checks every session) (the user's own; same id replaces a built-in rule). The file header documents every field.
- Rules run at index time in `audit.rs`. A `RegexSet` per field prefilters, so adding rules stays cheap. The prefilter uses ASCII word boundaries (`ascii_boundaries`): Unicode `\b` drops the regex engine onto its slow path for any text with a non-ASCII character. The rule's own regex confirms each candidate. Time it with `cargo run --release -p agentcam-core --example bench_audit` (reads your real logs, prints only timings and counts).
- Command rules see the command with heredoc bodies that are data or another language's code removed (`shell::shell_text`); bodies a shell runs stay. Secrets in those bodies are still found through `input`.
- Three kinds of rule. **Pattern rules** (`pattern`, optional `unless` to drop near misses, `when_tagged` to check only steps with a context tag). **Sequence rules** (`sequence` of links matched by `tags` or `rules`, within `within` steps, default 30); the finding sits on the last step with `related_seq` pointing at the first. **Outside-content rules** (`outside_content: [mcp, tracker, web]`) flag a command that runs a package, runs a download or sends data, when that package, URL or host showed up in that kind of tool output in the last `within` steps (default 15) and never in a prompt. Plain downloads and the source's own host don't count.
- Pattern rules leave `tags` on steps (`secret-read`, `egress`, `bypass`, `injection`). Every step also gets context tags: `outside-content` (`outside-mcp`, `outside-web`, `outside-tracker`), `guardrails-off`, `denied`, and `command`, `edit`, `web`, `mcp`.
- Check new rules against real logs before shipping them (`cargo run -- --db /tmp/x.db index`, then count `findings` by rule): a rule that fires on ordinary work is worse than no rule.
- Matches marked `secret: true` are masked before they are stored. Never store or log a raw secret.
- Changed rules re-check every session (fingerprint in the `meta` table). If you change how rules are checked in code, bump `ENGINE_VERSION` in `audit.rs`.
- Every rule needs a test in `audit.rs` that shows a match and a near miss.
- `agentcam scan` prints findings in the terminal.
- **Reviews** (`reviews.rs`): a finding marked "looks fine" or a rule muted for a project (or everywhere) drops out of the Audit page's To review list. Marks live in `reviews.json` next to the index, not in it, so they survive a rebuild. They never change what rules find or what exports contain.
- **Rule preview** (`preview.rs`): rebuilds the most recent sessions from the index (titles, text, inputs, touches, turn settings) and checks them with the draft rules, reporting only the rules the draft defines, with example steps. Text is the indexed 32 KB cut, so a preview can miss a match deep in a long output.

### Evidence and SIEM export

- `evidence.rs`: one session (with subagents) or a range as a single JSON bundle: identity, prompts, steps with approvals, file changes, findings, commits, PRs, secrets masked. `integrity.content_sha256` is SHA-256 of the canonical JSON (keys sorted) without that field; `evidence::verify` recomputes it. `serde_json` is built with `float_roundtrip` so numbers read back exactly; keep it. `GET /api/evidence`, `agentcam evidence [--session|--range] [verify file]`.
- `siem.rs`: findings as NDJSON or OCSF 1.3 Detection Finding (class 2004), oldest first. `GET /api/findings/export`, `agentcam export findings --format ocsf --since <ts>`. Never prompts, output or diffs.
- Team server: `siem.rs` in `agentcam-team` builds the same records from synced metadata (OS user only when names are on). Pull: `GET /api/team/findings/export` (viewer token). Push: `POST /api/team/siem` sets a webhook (`ndjson`, `ocsf`, `splunk_hec`; https or loopback only); each sync sends the findings it brought, off the request path. The token is stored on the server and never returned.

### What MCP servers and skills tell the agent

- `toolcheck.rs`. Claude Code logs MCP tool descriptions (`deferred_tools_record`) and server instructions (`mcp_instructions_delta`); Codex logs `additional_tools` namespaces named `mcp__<server>`. Each session's are stored in `session_tools`, texts once in `tool_texts` by hash, so a changed description shows as a second hash.
- `scan_text` checks any text the agent is given: hidden Unicode (tag characters decoded), "don't tell the user", "ignore previous instructions", requests to read secret files, downloads piped into a shell. Skill scripts also get `credentials_and_network` (credential files or the whole environment, plus a network call; one env var for an API token is normal).
- Setup adds config checks per MCP server (`outside_content`, `unpinned`, `secret_keys` names only, `plain_http`) and the risky mix `outside_content_shell`. The fleet inventory carries these as `Item.tags` and machine flags (share schema 3). Calibrate against real config before adding a check; skills and docs mention keys and tokens all the time.

### Block next time (guard)

- `guard.rs` holds guard rules (`guard.yaml` next to the index): tool name, regex, message. The UI adds them from a replay step.
- A Claude Code or Codex `PreToolUse` hook runs `agentcam guard`, which reads the call from stdin and exits 2 (block, reason on stderr) or 0. It must stay fast (no index, no logging) and **fail open**: any error lets the call run.
- agentcam never edits the agent's settings. The Audit page shows the hook JSON for the user to add.
- Codex uses the same hook contract (exit 2 + stderr blocks; `Bash` and `apply_patch` send `tool_input.command`). Codex makes users trust new hooks in `/hooks` before they run. Indexed Codex tool names map to hook names with `guard::hook_tool`.
- Write routes (`POST`, `DELETE`) are refused unless `Origin` and `Sec-Fetch-Site` say the request came from agentcam's own page (`same_site_writes` in the server). Keep it.

### Live view and usage limits

- `live.rs` answers both. A session's state comes from its latest step: `working` (a step in the last 2 minutes, a tool call still running, or a subagent busy), `waiting` (the agent replied), else `quiet`.
- Usage windows come from the logs, never from an API. Codex writes `rate_limits` on token counts (5-hour and weekly `used_percent`, `resets_at`, plan); the parser keeps only changed readings. Claude Code only logs when a limit is reached ("You've hit your session limit · resets 3pm (...)", `error: rate_limit`); it becomes a `System` step, not a reply, plus a hit. Repeats of one hit are merged in the query.
- `sessions.entrypoint` records where a session started (`cli`, `claude-desktop`, Codex `originator`); the UI maps it to plain words in `web/src/lib/live.ts`.
- The watcher re-indexes at least every 3 s while an agent keeps writing, so the Live page keeps up.
- `quota.rs` estimates Claude Code's windows from logged tokens (sessions and subagents, last 90 days): a block starts at the hour of the first call after the previous block ended and lasts 5 hours, or ends after 5 hours without a call (as ccusage does). Tokens are all four counts added up; value is the per-step API price. The reference is the highest ended block, never a limit. Always label it an estimate in the UI.
- `LiveSession.error_streak` counts tool calls in a row that errored, newest first, across the session and its subagents; rejected or blocked calls are skipped.

### Alerts, resume and export

- Alerts run in the browser (`web/src/lib/alerts.ts`, mounted in `AppShell`): they compare each `/api/live` reading with the last and use the Notification API, asked for only from a click. Settings (kinds, cost limits, muted projects, quiet hours) live in localStorage, every access in try/catch. The first reading only sets a baseline. Cost alerts remember what fired so a reload doesn't repeat them. The server sends nothing.
- Alert text follows the voice guide: "Waiting for you", "Done for now", "4 steps in a row errored · worth a look". Never "failed".
- Resume commands (`web/src/lib/session.ts`, `export::resume_command`) `cd` into `sessions.project_path` (the real folder, worktree included); subagents resume through their parent.
- "Open in editor" links use `vscode://file`, `cursor://file` and `zed://file` URLs; the editor choice is in localStorage.
- `export.rs` builds exports from the index (text already cut to 32 KB) and masks every match of a `secret: true` rule with `Rules::mask_secrets`. Markdown leaves out thinking and keeps the first 20 lines of command output.
- CSV downloads are built in the browser from the rows a table shows (`web/src/lib/csv.ts`); cells that start with `=`, `+`, `-` or `@` get a leading `'` so spreadsheets don't run them.

### Turns, approvals and what the logs add

- A **turn** is a prompt you typed and every step until the next one (`ParsedSession.turns`, table `turns` with `seq`..`end_seq`). Claude Code turns take the permission mode from the prompt record and the model and effort from the first reply; Codex turns take the last `turn_context` before the next prompt and end at `task_complete`. Counts and lengths are computed from the steps in `turns.rs`.
- **Approvals** come from Claude Code's `permissionDecision` and `toolDenialKind` on tool results (`Event::approval`: `rule`, `auto`, `mode`, `config`, `you`, `rejected`, `blocked-*`). A rejected or blocked call never ran: it records no file changes and isn't counted as an error.
- **Allow rule suggestions** (`approvals.rs`) group calls you approved by hand or that auto mode had to check. Broad commands (interpreters, shells, `rm`, `sed`, `git push`, ...) are never suggested; MCP tools are grouped per server. The UI starts with nothing picked and only shows JSON to copy: agentcam never edits settings.
- `Event::via` records the skill or plugin (Claude Code `attributionSkill`, `attributionPlugin`) or MCP app (Codex `appName`) behind a step. Thinking steps carry `thinkingDurationMs` as their duration.
- Claude Code `edited_text_file` attachments become `System` steps (`tool_name = external_edit`): a file changed outside the session. They are not file touches.
- `pr-link` records go to the `prs` table. Codex `reasoning_output_tokens` and `service_tier` are kept per step and session.

### Skills, comparison and follow-through

- **Skills** (`skills.rs`): uses come from Claude Code's `Skill` tool, slash commands that name a listed skill, `invoked_skills` (loaded size) and either agent reading a `SKILL.md` (`detect_reads`, real folder names only). `skill_listing` gives the skills available per session and how much context the listing takes. The inventory reads `~/.claude/skills`, `~/.claude/plugins`, `~/.codex/skills` and each project's `.claude/skills`: names, descriptions and sizes only.
- **Comparison** (`compare.rs`): every top-level turn is a row with its model, agent version, effort, mode, skills and start time. A group is the turns with one value. Numbers are medians or rates per 100 steps or calls, so groups of different sizes compare fairly. `options` finds comparisons worth a look: the two most used models, efforts and modes, each agent's newest version against the one before, the latest AGENTS.md or CLAUDE.md change per project, and the most used skill. The UI describes changes ("24% shorter"), never scores them.
- **Follow-through** (`follow.rs`): rules are pulled from AGENTS.md and CLAUDE.md (following `@file` imports) only when a log can check them: commands under a "before you finish" style heading or a line saying they must pass (optionally only when files under a folder changed), a sole non-npm package manager, paths right after "never edit", commands right after "never run", and folders a layout block marks generated. Checks use command keys (`approvals::command_prefix`), so flags don't matter. Docs-only edits don't call for checks. Files are read as they are today.

### What lasted (code survival)

- `survival.rs` takes the lines each edit added (from the diff in the log; all lines for a new file) and looks for them in the file on disk today, by trimmed text. Lines with fewer than three letters or digits are skipped. A missing line is `reworked` if a later agent edit removed it (or the agent deleted the file), else `changed`.
- It runs at the end of every index run and every 2 minutes from the watcher. A file is re-read only when its size, mtime or edits change (`survival_files.sig`). Bump `ENGINE` in `survival.rs` when you change how lines are read or sorted.
- Files in agent worktrees are looked up in the main repo once the worktree is gone. If a file's project folder no longer exists, the edit is left out rather than counted as gone.
- It reads project files but stores only counts. Never store or log file contents here.
- The session detail carries `lasted` (session plus subagents) and each step touch carries its own counts.
- `blame(path)` reads the file on demand and gives each line to the latest agent edit that added the same trimmed text; blank lines and braces between two lines of one edit go with it. Worktree edits count for the main repo's path. Nothing is stored. A file no agent edited is never read (returns no lines), so the API can't be used to read arbitrary files. Used by `/api/blame`, `agentcam blame` and the MCP `blame` tool.

### Plans, hooks, prompts, setup and MCP

- `plans.rs` rebuilds the agent's task list per step from tool inputs: Claude Code `TodoWrite` (whole list), `TaskCreate` / `TaskUpdate` (ids given in creation order, `deleted` removes), Codex `update_plan`, and `ExitPlanMode` plans. The session detail carries `plan`; the replay shows the list as of the playhead.
- `util::command_program` names the script or program a hook or MCP command runs (skipping `X=Y`, flags and launchers such as `bunx`, `uv run`, `python3`). Step titles, Setup and the fleet inventory all use it, so they agree.
- Claude Code `hook_*` attachments become `System` steps with `tool_name = hook` (title like "PostToolUse hook (prettier) ran"; `_error` kinds are errored). The UI gives them their own icon.
- `prompts.rs`: prompts on top-level sessions; repeats are grouped by a normalized text (case, spacing, punctuation) and need two or more sessions.
- `setup.rs` reads the agents' config locally: settings files (user, local, managed, each recent project's), `~/.claude.json` and `.mcp.json` MCP servers, Codex `config.toml`, hooks, memory and instruction files, skills. Environment variables are listed by name only. Checks say "worth a look", never "violation". `ConfigDirs` in `lib.rs` honors `CLAUDE_CONFIG_DIR` and `CODEX_HOME` everywhere (log folders, skills, fleet).
- `mcp.rs` answers JSON-RPC for `agentcam mcp`: `initialize`, `tools/list`, `tools/call` (search_sessions, recent_sessions, get_session, get_step, file_history, blame, project_rules, what_lasted). Read only; stdout carries only protocol messages, so log to stderr. Every tool answer goes through `Rules::mask_secrets`. The binary indexes at start and again before a `tools/call` when 30 s have passed. User docs: `docs/mcp.md`; keep its tool table in step with `tools()`.

### Identity and the share contract (team prep)

- `identity.rs`: a random `machine-id` file next to the index, the OS user, and a project id from the `origin` remote in `.git/config` (`host/owner/repo`, lowercased, credentials dropped; no git command runs). Each session row stores these plus a `version` that changes when the session is re-indexed with new data.
- `share.rs` is the only thing a sync may send. `SessionMeta` holds counts, times, tokens, cost, findings, commits and file paths; never prompts, replies, thinking, command text, output, inputs or diffs. Paths are relative to the project, others cut to a file name; excerpts lose home folders (`share_text`). Bump `share::SCHEMA_VERSION` when fields change.
- `outbox_sends_metadata_only` in `tests/fixtures.rs` fails if a content field or a home path slips in. Keep it passing when you add fields.
- The Outbox page (`/outbox`) shows exactly what would be sent.
- `fleet.rs` is the other half of the contract: agent versions in use, MCP servers, plugins, skills and hooks **by name**, and a few setting flags (`approvals_skipped_by_default`, `sandbox_off`, `broad_allow_rule`, `danger_prompt_skipped`, `guard_hook_installed`). Never commands, arguments, URLs, headers or environment values; hook commands become the program's file name. Its test fails if a URL, path or secret leaks.

### Team server

- `crates/agentcam-team`: `store.rs` (SQLite, numbered `MIGRATIONS`, plain SQL that Postgres also runs: `ON CONFLICT ... DO UPDATE`, no SQLite-only functions in queries), `server.rs` (axum), `views.rs` (Insights, Value, Governance, Fleet), `client.rs` (laptop side).
- `agentcam team serve --data <dir> --bind 127.0.0.1:4850` prints the first admin token once. It serves the same embedded UI; `web/src/main.tsx` renders the team app for paths under `/team`.
- Auth is by token, not Host: laptops use device tokens (`ac_dev_`), the UI uses viewer tokens (`ac_view_`, roles `admin` and `viewer`, kept in the browser's localStorage and sent as `Authorization: Bearer`). Invites (`ac_inv_`) are traded for a device token. Only SHA-256 hashes of tokens are stored. Admin changes go to `audit_log`. On the server machine, `agentcam team machines|remove|viewers|revoke` manage machines and viewer tokens; the last admin can't be revoked. Operator docs: `docs/team-server.md`.
- `POST /api/team/join` and `POST /api/team/sync` (device token; at most 1000 sessions per request, unchanged versions skipped, a laptop may only send its own machine id). UI routes: `GET /api/team/me|insights|value|governance|fleet|settings|baseline|findings/export` (`range`, `team`), `GET/POST /api/team/siem` and `POST /api/team/settings|invites|viewers|baseline` (admin).
- The laptop keeps `team.json` (link and token, mode 600) and `team-sync.json` (versions sent, last sync, last error) next to the index. `agentcam serve` syncs every 5 minutes when linked; `agentcam team sync|status|pause|resume|leave` do the rest. `https://` is required unless the server is on loopback or `--allow-http` is passed.
- Baseline (`baseline.rs`): an admin sets flags every machine should have or none should, tags no MCP server or skill should carry, an MCP allowlist and oldest agent versions (`GET/POST /api/team/baseline`). Each sync stores the machine's setup as fact strings in `machine_facts` (with `first_seen`) and logs added and removed facts in `fact_changes`, so each difference says since when. It never changes a machine.
- Views describe, never rank people. Machines show as `Machine abc123` unless an admin turns on `show_people` (logged). Cost is at API list prices; seats compare a monthly seat price with that.
- Demo: `agentcam team serve --data /tmp/agentcam-team-demo`, then `python3 scripts/team-demo.py http://127.0.0.1:4850 <admin token>` makes 24 fake machines in 4 teams over 90 days. Team screenshots come from it.

### Cost, context, waste and commits

- **Tokens per step:** each model call's usage sits on the first step it produced (`TokenUsage` in `model.rs`); context size = fresh input + cache reads + cache writes.
- **Prices:** `rules/prices.yaml` (built in, Anthropic list prices) plus the user's `prices.yaml` next to the index. Longest `match` prefix wins. Missing cache prices are estimated (10% / 125% of input), and a model without a price leaves the cost unknown, never guessed. Changing prices re-checks every session (fingerprint in `meta`).
- **Waste (`patterns.rs`):** retry loops (same call failing 3+ times), error streaks (4+ failed tool calls in a row), reverts (an Edit that undoes an earlier one) and edit churn (6+ edits to one file, a signal only). `events.wasted` marks steps counted as wasted, each once.
- **Commits (`commits.rs`):** only real `git commit` command segments count (via `shell::command_words`, heredoc bodies skipped); hash and branch from git's `[branch hash] message` output when present.
- **OTel (`otel.rs`):** `agentcam export otlp` writes OTLP/JSON; it only sends data when `--endpoint` is given.

### Performance rules

- Pages other than Overview and Replay are `React.lazy` chunks (`web/src/App.tsx`), and the team app loads only under `/team`. Add new pages the same way.
- Long lists (transcript rows, timeline marks) are memoized rows with stable callbacks, and rows use `content-visibility: auto`. Moving the playhead must only re-render rows whose state changed.
- Don't push per-frame state through the router; the replay writes `?step=` with `history.replaceState` after the playhead rests.
- Check big sessions (1,500+ steps): stepping should stay under ~10 ms.
- Every request must use a loopback `Host` (`127.0.0.1`, `localhost`, `[::1]`). This blocks DNS rebinding; keep it.
- Pages (both apps, via `static_handler`) send a Content Security Policy (`PAGE_POLICY`: only `'self'`, no framing) and `X-Frame-Options: DENY`, so another site can't frame agentcam and trick clicks. Write routes need an `Origin` equal to the request's `Host`, so another app on localhost can't post either.

## UI and design system

Open `/styleguide` in the running app to see every token and component. Before adding UI:

- **Tokens only.** Colors, sizes, spacing, radius and motion come from `web/src/styles/tokens.css`. No raw hex values or pixel sizes outside it (layout widths aside).
- **Text uses `--text-*`**, never a kind, agent or status color. Data colors go on marks, dots and icons beside the text.
- **Event colors are fixed at five groups** (`web/src/lib/kinds.ts`): You (blue), Changes (orange), Looks (aqua), Agent (gray), Error (red). The three hues were checked for color-blind separation as a set. Don't add a fourth hue; put new kinds in an existing group and give them their own icon.
- **Status colors** (good, warning, serious, critical) always come with an icon or a word.
- **Fonts ship inside the binary** (`@fontsource-variable/*`). Never load fonts, scripts or anything else from the network.
- **Motion is short and responds to the user.** Use `--dur-*` and `--ease-out`; reduced-motion users get none.
- Components live in `web/src/ui/`, one CSS module each. Pages live in `web/src/pages/`. Keep each component small and add it to the styleguide.
- Check every UI change at 1280px and at phone width (375px) with no sideways scroll.

## Code style

- Rust 2024 edition, stable toolchain. `unsafe` is forbidden in the workspace.
- Errors: `anyhow` in the binary, typed errors (`thiserror`) in library crates once they have real error cases.
- Parsers must not panic on bad input. Unknown or malformed records are kept raw and left out of views, never fatal.
- Keep dependencies few. Ask before adding a large one.
- TypeScript: strict mode, function components, no `any` without a comment saying why.

## How parsing works

- `agentcam-core/src/model.rs`: the shared model. A `Session` has `Event`s (prompt, response, thinking, tool_call, system) and `FileTouch`es. A tool call and its result are one event.
- `claude.rs` and `codex.rs` turn one log file into one `ParsedSession`. They read line by line, skip bad lines, and ignore record types they don't know.
- `index.rs` stores sessions in SQLite with an FTS5 table over event titles and bodies. It re-parses a file only when its size or modified time changes. Sessions stay after their log file is deleted (agents prune old logs; agentcam keeps the history).
- `shell.rs` reads file changes out of shell commands (`>`, `>>`, `tee`, `sed -i`, `rm`, `mv`, `cp`, `touch`, `cd` chains, skipping heredoc bodies). It is a careful reader, not a shell: when unsure (globs, variables, `~`), it records nothing. Files written by scripts it runs (say `python script.py`) are not seen.
- Long text is cut to 32 KB in the index. `source_offset` points back at the original record.
- **Schema changes are migrations.** `SCHEMA` in `index.rs` is the 0.1.0 schema (`BASE_VERSION`, 14) and never changes. To change it, append a `Migration` to `MIGRATIONS`: SQL run in one transaction, and `reparse: true` when new fields can only come from the logs (the next run re-reads every log; sessions whose logs are gone keep what they had). Never edit a released step. Before migrating, the index is copied to `index.db.bak-v<N>`. An index from a newer build is refused, never wiped. Pre-release indexes (below 14) are still dropped and rebuilt.
- Log folders are looked up on every index run, and the watcher checks every 5 s for one that didn't exist at start, so an agent used for the first time after agentcam started shows up without a restart.

## Log formats

- Claude Code: `~/.claude/projects/<escaped-project-path>/<session-id>.jsonl`. One JSON record per line. Message records (`user`, `assistant`) carry content blocks (`text`, `thinking`, `tool_use`, `tool_result`). One API message can span several `assistant` records with the same `message.id`, so count usage once per id. `user` records with a `toolUseResult` carry structured results (diffs as `structuredPatch`). Titles come from `custom-title` / `ai-title`; cost from `cost-state`. Subagents live in `<session-id>/subagents/**/agent-*.jsonl` with a `.meta.json` beside them; `journal.jsonl` files there are not transcripts.
- Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`. Records have a top-level `type` (`session_meta`, `turn_context`, `response_item`, `event_msg`, ...) and a `payload` with its own `type`. Newer logs have `event_msg/item_completed` records with typed items (`UserMessage`, `AgentMessage`, `CommandExecution`, `FileChange`, `McpToolCall`, ...); the parser prefers these and falls back to `response_item` records for older logs. Sessions with a `parent_thread_id` are background threads (e.g. guardian reviews).

Both formats change between agent versions without notice. Parse what you know, keep the rest.
