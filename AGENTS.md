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
```

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

## Code style

- Rust 2024 edition, stable toolchain. `unsafe` is forbidden in the workspace.
- Errors: `anyhow` in the binary, typed errors (`thiserror`) in library crates once they have real error cases.
- Parsers must not panic on bad input. Unknown or malformed records are kept raw and left out of views, never fatal.
- Keep dependencies few. Ask before adding a large one.
- TypeScript: strict mode, function components, no `any` without a comment saying why.

## Log formats

- Claude Code: `~/.claude/projects/<escaped-project-path>/<session-id>.jsonl`. One JSON record per line. Message records (`user`, `assistant`) carry content blocks (`text`, `thinking`, `tool_use`, `tool_result`); many other record types are metadata.
- Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`. Records have a top-level `type` (`session_meta`, `turn_context`, `response_item`, `event_msg`, ...) and a `payload` with its own `type`.

Both formats change between agent versions without notice. Parse what you know, keep the rest.
