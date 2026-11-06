# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

### Added

- Team server (`agentcam team serve`): a self-hosted server that laptops push session metadata to after `agentcam team join`. Pages for Insights (time, cost, what lasted and rules followed by week, team, repo and model), Value (cost per commit and PR, lines kept per dollar, seats against API-priced use, machines active per week, models and agent versions side by side), Governance (findings across the team) and Fleet (agent versions, MCP servers, plugins, skills, hooks and settings worth a look on each machine). Invites, device and viewer tokens, an audit log, and names hidden unless an admin turns them on.
- `agentcam team join|sync|status|pause|resume|leave` on the laptop; the Outbox page shows the team link and the setup inventory that's sent.
- Share contract v2: turn count, lines still in the code, instruction rules followed and pull request numbers (counts only).

- Cargo workspace (`agentcam`, `agentcam-server`, `agentcam-core`) and a React + Vite web UI embedded in the binary.
- `agentcam serve`: local server on `127.0.0.1:4747` with `/api/health`; opens the browser.
- Parsers for Claude Code and Codex logs (including subagents and older Codex formats) into one event model.
- SQLite index with full-text search; only changed logs are re-read, and sessions are kept after their logs are deleted.
- `agentcam index` and `agentcam search` commands.
- Cost: per-step token usage and an estimated cost from a model price table (`prices.yaml`, editable); cost in the overview, projects, sessions, heatmap and replay.
- Context window chart per session, with compactions marked.
- Stuck detection: retry loops, failure streaks and reverts, with time lost shown on sessions, the overview and the replay timeline.
- Commits: which commits each session made, a Commits page, and commits in the replay.
- Trace view in the replay: steps on an agent-time axis with subagents nested inside their calls.
- `agentcam export otlp`: sessions as OpenTelemetry traces, to a file or an OTLP endpoint.
- Compare: two groups of turns side by side (model, agent version, effort, mode, skill, or before and after a date), with comparisons found in your sessions such as before and after an AGENTS.md change.
- Instructions: rules a log can check, pulled from each project's AGENTS.md and CLAUDE.md, with how often sessions followed them and links to the sessions that didn't; also shown in the replay.
- Skills: which skills exist, which the agents or you ran, how much context they and the skill listing take, and which go unused.
- A More menu in the top bar for the less used pages.
- Turns: each prompt and the work until the next, with length, steps, cost, thinking time, model, effort and mode. Turn headers and a Turns view in the replay; typical and longest turns, and how fast you reply, on the overview.
- Approvals: how each Claude Code tool call was allowed (rule, auto mode, mode, you) or stopped, shown on steps; an Approvals panel on the Audit page with stopped calls and allow rules to copy. Rejected or blocked calls no longer count as file changes or errors.
- Thinking page: browse the longest or newest thinking, search it, and see thinking time by model.
- Skills, plugins and MCP servers behind steps (Tools page and replay), pull requests sessions opened (Commits page and replay), files changed outside a session, Codex reasoning tokens and service tier.
- What lasted: of the lines agents added, how many are still in the code, rewritten by later agent edits, or changed since. By age, week, agent and model, project and file; per session and per change in the replay. Works on past sessions with no setup.
- Live page: sessions running now with their state (working, your turn, quiet), latest steps, cost and context; a dot in the nav while an agent works.
- Usage limits: Codex 5-hour and weekly windows with a 7-day chart and reset times; limits reached by Claude Code and Codex, linked to the step.
- Where each session was started from (terminal, desktop app, VS Code, SDK), shown in the replay.
- Team prep: a machine id, the OS user, project ids from git remotes and session versions in the index; a metadata-only share contract (`share.rs`) and an Outbox page that shows what a team server would receive.
- README with screenshots, commands, supported agents and privacy notes; `scripts/demo-data.py` makes fake sessions for demos.
- Rule editor on the Audit page: see all rules, write your own in YAML, check and save; saving re-checks every session.
- Block next time works for Codex too (same hook contract), with setup for both agents on the Audit page.
- Block next time: turn a replay step into a guard rule (tool + pattern + message), see how many past steps it would have blocked, and enforce it with a Claude Code `PreToolUse` hook that runs `agentcam guard`. Rules are listed on the Audit page with the hook setup.
- The local API refuses cross-site writes.
- Report cards: a shareable 1200×630 PNG for a session or the last 7 days, drawn in the browser. Titles, projects and file names are hidden by default; copy or download.
- Risk audit: 28 built-in YAML rules (secrets, sensitive files, dangerous commands, installs, outbound domains, writes outside the project) checked at index time; secrets are masked; your own rules go in `rules.yaml` next to the index.
- `agentcam scan` prints findings in the terminal.
- Audit page, risk flags on the replay timeline, a risks list and per-step findings in the replay, and risk badges on sessions. `R` jumps to the next risk.
- Tool calls page (use, failures and time per tool) and Files page (most changed files and each file's history).
- Search: matches in the ⌘K palette as you type, and a search page with type, agent and time filters. Each match opens the replay at that step.
- Live updates: the server watches agent log folders and re-indexes changed files on its own; open pages refresh.
- Faster replay on long sessions (about 17× faster stepping on a 1,500-step session).
- Session replay: a zoomable timeline with play, pause and 1×/4×/16× speeds; a transcript that follows the playhead; a step pane with command output and side-by-side line-numbered diffs; a files list that jumps to each change; keyboard controls and `?step=` deep links.
- Agent replies render as Markdown (built from React elements; links are shown, never followed).
- File changes made through shell commands (`>`, `sed -i`, `rm`, `mv`, `cp`, `tee`, `touch`) now count, for Claude Code and Codex.
- Overview page: agent time, sessions, prompts, tool calls and files changed with comparison to the previous period; a year-long activity heatmap; projects with weekly sparklines; agents split; sessions grouped by day. Filters live in the URL.
- Session page with facts and a step timeline.
- JSON API over the index; `agentcam serve` indexes in the background on start.
- The server refuses requests that don't use a loopback Host header (blocks DNS rebinding).
- Design system: dark tokens, a color-blind-checked palette for event kinds, bundled Inter and JetBrains Mono, base components, a ⌘K command palette and a `/styleguide` page.
