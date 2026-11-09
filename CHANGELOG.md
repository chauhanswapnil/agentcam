# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

## 0.1.0 - 2026-11-09

The first release. agentcam reads the logs Claude Code and Codex already write on your machine, including sessions from before you installed it, and shows them back to you. One local binary: no account, no telemetry, nothing leaves the machine.

### See the work

- **Overview:** agent time, sessions, cost and files changed, a year of activity, projects and sessions by day.
- **Replay** any session step by step: timeline, transcript, diffs, the agent's task list as it changed, subagents with their cost, hooks that ran.
- **Search** every prompt, reply, command and file across both agents, from the page or `agentcam search`.
- **Live:** sessions working or waiting for you, with optional browser alerts.
- **Resume** a session with one copied command, or open a changed file in VS Code, Cursor or Zed.

### Did it work?

- **What lasted:** of the lines agents wrote, how many are still in the code today.
- **Who wrote this:** each line of a file with the session and prompt behind it (`agentcam blame`).
- **Compare** two groups of turns: models, agent versions, effort, before and after an AGENTS.md change.
- **Instructions:** which AGENTS.md and CLAUDE.md rules sessions followed.

### Cost and limits

- Estimated cost per step, session and project at API list prices; your own prices in `prices.yaml`.
- Codex usage windows from its logs, and a Claude Code 5-hour usage estimate.
- Retries, error streaks and reverts called out.

### Safety

- **Risk audit:** 73 built-in rules (secrets, sensitive files, dangerous commands, data sent out, persistence) plus your own in YAML. Chains across steps, such as a secret read before an upload, and commands built from MCP results, issues or web pages. Mark findings "looks fine" or mute a rule per project.
- **Block next time:** a `PreToolUse` hook for Claude Code and Codex that asks the agent to check with you before a call you chose. agentcam never edits your agent settings.
- **Setup:** settings, MCP servers, hooks and skills on this machine, with checks worth a look, including hidden or changed text in MCP tool descriptions.
- **Evidence bundles** with a checksum (`agentcam evidence`), and findings for a SIEM as NDJSON or OCSF.

### Share and export

- Sessions as Markdown or JSON, every table as CSV, report cards as PNG, secrets masked.
- `agentcam mcp`: a read-only MCP server, so agents can look up past sessions.
- **Team server (preview):** run your own; laptops send metadata only, never prompts, code or output. Insights, Value, Governance and Fleet views, a setup baseline, and SIEM export.

### Install

Homebrew (`brew install chauhanswapnil/tap/agentcam`), npm (`npx agentcam`), shell and PowerShell installers, or `cargo install agentcam`. macOS, Linux and Windows.
