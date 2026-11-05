# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

### Added

- Cargo workspace (`agentcam`, `agentcam-server`, `agentcam-core`) and a React + Vite web UI embedded in the binary.
- `agentcam serve`: local server on `127.0.0.1:4747` with `/api/health`; opens the browser.
- Parsers for Claude Code and Codex logs (including subagents and older Codex formats) into one event model.
- SQLite index with full-text search; only changed logs are re-read, and sessions are kept after their logs are deleted.
- `agentcam index` and `agentcam search` commands.
