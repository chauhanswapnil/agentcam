# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

### Added

- Cargo workspace (`agentcam`, `agentcam-server`, `agentcam-core`) and a React + Vite web UI embedded in the binary.
- `agentcam serve`: local server on `127.0.0.1:4747` with `/api/health`; opens the browser.
- Parsers for Claude Code and Codex logs (including subagents and older Codex formats) into one event model.
- SQLite index with full-text search; only changed logs are re-read, and sessions are kept after their logs are deleted.
- `agentcam index` and `agentcam search` commands.
- Overview page: agent time, sessions, prompts, tool calls and files changed with comparison to the previous period; a year-long activity heatmap; projects with weekly sparklines; agents split; sessions grouped by day. Filters live in the URL.
- Session page with facts and a step timeline.
- JSON API over the index; `agentcam serve` indexes in the background on start.
- The server refuses requests that don't use a loopback Host header (blocks DNS rebinding).
- Design system: dark tokens, a color-blind-checked palette for event kinds, bundled Inter and JetBrains Mono, base components, a ⌘K command palette and a `/styleguide` page.
