# AGENTS.md

Guidance for AI coding agents (and humans) working in this repo.

## What this is

agentcam is a local-first, open-source tool that reads the session logs coding agents write (Claude Code, Codex) and shows them as an overview, a session replay and search. Later: a risk audit with YAML rules. It is one Rust binary that serves an embedded React UI on `127.0.0.1`.

Product rules that shape every change:

- **Read only.** Never write to, move or delete an agent's log files.
- **Local only.** Bind to `127.0.0.1`. No network calls, no telemetry, no accounts.
- **Zero setup.** Work from the logs that already exist; no hooks or SDK required.
- **The UI is the product.** Visual polish is a feature, not a nice-to-have.

## Layout

```
crates/
  agentcam-core/     parsers, event model, index (no I/O beyond reading logs + SQLite)
  agentcam-server/   axum server: JSON API under /api, embedded UI for everything else
    ui-dist/         built web UI (generated, gitignored, embedded with rust-embed)
  agentcam/          the CLI binary (clap); published as `agentcam` on crates.io
web/                 React + Vite + TypeScript UI; builds into crates/agentcam-server/ui-dist
  src/styles/        design tokens and base CSS
  src/ui/            shared components (one CSS module each)
  src/pages/         routed pages, including /styleguide
  src/lib/           event kinds, agents, formatting
fixtures/            synthetic agent logs for tests (never real ones, see below)
```

Dependencies flow one way: `agentcam` → `agentcam-server` → `agentcam-core`. Core must not depend on the server or the CLI.

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
```

The index lives in the user data folder (`~/Library/Application Support/agentcam/index.db` on macOS). Use `--db <file>` or `AGENTCAM_DB` to point at a scratch index while developing.

UI dev loop: run `cargo run -- serve --no-open` in one terminal and `npm run dev` in `web/` in another, then open the Vite URL.

`build.rs` in `agentcam-server` writes a placeholder `ui-dist/index.html` if the UI hasn't been built, so `cargo build` always works. Run `npm run build` before a release build or the binary ships the placeholder.

## Before you finish a change

1. `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` are clean.
2. `cargo test` passes.
3. If you touched `web/`: `npm run lint` and `npm run build` pass.
4. New behavior has a test. Parser changes have a fixture that shows the case.
5. UI changes: open the app and look at them. Screenshots in the PR help.

## Privacy rules (important)

Agent logs hold real secrets, file contents and private prompts.

- **Never commit real session logs**, excerpts of them, or indexes built from them. Not in fixtures, tests, docs, issues or screenshots.
- Fixtures in `fixtures/` are hand-written and synthetic. Paths use `/Users/dev/...`; secrets are obviously fake (`sk-test-0000...`).
- Don't log full event payloads at `info` level. Use `debug` or `trace`.

## API

The server (`crates/agentcam-server`) serves JSON under `/api`. Queries live in `agentcam-core/src/query.rs`; handlers in `agentcam-server/src/api.rs` stay thin.

| Route | What it returns |
|---|---|
| `GET /api/status` | Index state; `generation` goes up after each run, so the UI refetches |
| `POST /api/index` | Start indexing new and changed logs (202, or 409 if already running) |
| `GET /api/overview` | Stats, the previous period's stats, and projects with weekly sparklines |
| `GET /api/activity` | Agent time per local day for the last 371 days |
| `GET /api/sessions` | Top-level sessions, most recently active first, paged |
| `GET /api/sessions/{id}` | One session: steps (titles, plus the start of transcript text), files touched, subagents |
| `GET /api/step?session=&seq=` | One step in full: output, input, file changes with diffs |
| `GET /api/search?q=` | Full-text matches, best first; takes `range`, `agent`, `project`, `kind`, `category`, `limit` |

List routes take `range` (`7d`, `30d`, `90d`, `all`), `agent`, `project` (a project root) and `day` (local `YYYY-MM-DD`).

- **Agent time** is the sum of gaps between steps, each capped at 5 minutes, so idle stretches don't count. It's stored per step (`events.gap_ms`) at index time.
- **Project root** folds agent worktrees (`repo/.claude/worktrees/x`) into the repo they belong to.
- Subagent sessions count toward their parent and are hidden from top-level lists.
- `agentcam serve` indexes in the background on start, using a separate SQLite connection so reads stay fast.
- It also watches the log folders (`notify`) and re-indexes 750 ms after agents stop writing. The UI polls `/api/status` and refetches when `generation` changes.
- Search input is turned into a safe FTS5 query (`query::fts_query`): every word quoted, the last one as a prefix. Never pass user text to `MATCH` directly.

### Performance rules

- Long lists (transcript rows, timeline marks) are memoized rows with stable callbacks, and rows use `content-visibility: auto`. Moving the playhead must only re-render rows whose state changed.
- Don't push per-frame state through the router; the replay writes `?step=` with `history.replaceState` after the playhead rests.
- Check big sessions (1,500+ steps): stepping should stay under ~10 ms.
- Every request must use a loopback `Host` (`127.0.0.1`, `localhost`, `[::1]`). This blocks DNS rebinding; keep it.

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
- Change the schema → bump `SCHEMA_VERSION` in `index.rs`; old indexes are dropped and rebuilt.

## Log formats

- Claude Code: `~/.claude/projects/<escaped-project-path>/<session-id>.jsonl`. One JSON record per line. Message records (`user`, `assistant`) carry content blocks (`text`, `thinking`, `tool_use`, `tool_result`). One API message can span several `assistant` records with the same `message.id`, so count usage once per id. `user` records with a `toolUseResult` carry structured results (diffs as `structuredPatch`). Titles come from `custom-title` / `ai-title`; cost from `cost-state`. Subagents live in `<session-id>/subagents/**/agent-*.jsonl` with a `.meta.json` beside them; `journal.jsonl` files there are not transcripts.
- Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`. Records have a top-level `type` (`session_meta`, `turn_context`, `response_item`, `event_msg`, ...) and a `payload` with its own `type`. Newer logs have `event_msg/item_completed` records with typed items (`UserMessage`, `AgentMessage`, `CommandExecution`, `FileChange`, `McpToolCall`, ...); the parser prefers these and falls back to `response_item` records for older logs. Sessions with a `parent_thread_id` are background threads (e.g. guardian reviews).

Both formats change between agent versions without notice. Parse what you know, keep the rest.
