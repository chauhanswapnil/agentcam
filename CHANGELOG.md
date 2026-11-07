# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

### Added

- Audit: 45 new built-in rules (73 in all) for secret managers and credential files, data sent out (credential files, environment variables, DNS lookups), cleared history, reverse shells and tunnels, sudoers, setuid, containers with host access, shell startup files, scheduled jobs, git hooks and remotes, cloud metadata, disk wipes, agents started with approvals off, and changes to agent settings or agentcam's own files. Each has a test with a match and a near miss, and all were checked against real logs for false alarms.
- Sequence rules: steps in order within a window, matched by tag or rule id. Built in: a secret read then an upload, a command after outside content held instructions, data sent out while approvals were off, another way tried after a call was stopped. Findings link back to the step the chain started from ("follows step 12"), and that step says it starts a chain.
- Outside content: commands that run a package, run a download or send data using a package, URL or host that came from an MCP result, an issue or a web page rather than your prompts; instruction-like text and hidden characters in tool output, with hidden text decoded.
- Rules can say `unless` (a regex that drops near misses), `tags` and `when_tagged`.
- Setup: the tool descriptions and instructions each MCP server gave the agent, read from Claude Code and Codex logs, with checks for hidden characters, orders aimed at the agent and descriptions that changed; skills and instruction files get the same checks, plus skill scripts that read credentials and call out. MCP servers are marked when they bring in outside content, run the latest release on every start, keep a secret in plain text or use plain http, and Setup notes when outside content can reach a shell that runs without asking.
- Evidence bundles: a session or range as JSON with identity, prompts, steps and approvals, changes, findings and commits, secrets masked, with a SHA-256 checksum (`agentcam evidence`, `agentcam evidence verify`, `GET /api/evidence`, buttons on the replay and Audit page).
- Findings for a SIEM as NDJSON or OCSF 1.3 Detection Finding (`agentcam export findings`, `GET /api/findings/export`).
- Team server: a setup baseline (flags required or ruled out, MCP and skill tags, an MCP allowlist, oldest agent versions) with the machines that differ and since when, and a log of setup changes; findings from every machine by pull (`/api/team/findings/export`) or webhook (NDJSON, OCSF, Splunk HEC). Fleet shows tags on MCP servers and skills.
- Docs: [audit rules](docs/audit-rules.md) and [evidence and SIEM](docs/evidence-and-siem.md).

- Who wrote this: each line of a file with the agent session, model and prompt that wrote it, from the Files page, `agentcam blame <file>` and `/api/blame`.
- `agentcam mcp`: a read-only MCP server so agents can search past sessions, read a session or step, see a file's history and blame, a repo's instruction rules and how they were followed, and what lasted.
- Setup page: settings files, instruction and memory files, MCP servers (with calls in 30 days), hooks (with runs and errors) and skills, plus checks worth a look such as approvals skipped by default, broad allow rules, an instruction that an allow rule overrides, hooks pointing at missing files, large memory files and heavy skill listings. `CLAUDE_CONFIG_DIR` and `CODEX_HOME` are honored.
- Replay: the agent's task list as of each step and the plans it proposed (TodoWrite, TaskCreate/TaskUpdate, Codex update_plan, ExitPlanMode), hooks that ran as their own steps, and a subagent tree with each branch's cost.
- Prompts page: search everything you typed, and see the prompts you keep typing across sessions.
- Team server admin commands: `agentcam team machines`, `team remove <machine>` (drops the machine and what it sent; its token stops working), `team viewers` and `team revoke <id>` (never the last admin).
- Docs: [running a team server](docs/team-server.md) (proxy, tokens, backups, upgrades) and [agentcam as an MCP server](docs/mcp.md) (setup and every tool).

- Team server (`agentcam team serve`): a self-hosted server that laptops push session metadata to after `agentcam team join`. Pages for Insights (time, cost, what lasted and rules followed by week, team, repo and model), Value (cost per commit and PR, lines kept per dollar, seats against API-priced use, machines active per week, models and agent versions side by side), Governance (findings across the team) and Fleet (agent versions, MCP servers, plugins, skills, hooks and settings worth a look on each machine). Invites, device and viewer tokens, an audit log, and names hidden unless an admin turns them on.
- `agentcam team join|sync|status|pause|resume|leave` on the laptop; the Outbox page shows the team link and the setup inventory that's sent.
- Share contract v2: turn count, lines still in the code, instruction rules followed and pull request numbers (counts only).
- Alerts: browser notifications (asked for only when you click) when a session is waiting for you, is done for now, has 4+ steps in a row error, or passes a cost per session or per day. Per-kind switches, project mutes and quiet hours, kept in the browser. A "waiting for you" count in the top bar.
- Claude Code usage estimate: 5-hour blocks counted from logged tokens the way ccusage does, with tokens and API-priced value, pace, a projection to the block end, your highest past block as the reference, and a rolling 7-day total. On the Live page and in `/api/limits` (`claude`), always marked as an estimate.
- Resume and open: copy `claude --resume <id>` or `codex resume <id>` with a `cd` into the session's folder (worktrees included) from the replay and session rows; open changed files at their line in VS Code, Cursor or Zed (choice remembered in the browser).
- Export: `GET /api/sessions/{id}/export?format=md|json` downloads a session as Markdown or JSON, secrets masked; CSV downloads on the sessions, tools, files, commits and findings tables.

- Cargo workspace (`agentcam`, `agentcam-server`, `agentcam-core`) and a React + Vite web UI embedded in the binary.
- `agentcam serve`: local server on `127.0.0.1:4747` with `/api/health`; opens the browser.
- Parsers for Claude Code and Codex logs (including subagents and older Codex formats) into one event model.
- SQLite index with full-text search; only changed logs are re-read, and sessions are kept after their logs are deleted.
- `agentcam index` and `agentcam search` commands.
- Cost: per-step token usage and an estimated cost from a model price table (`prices.yaml`, editable); cost in the overview, projects, sessions, heatmap and replay.
- Context window chart per session, with compactions marked.
- Retry detection: retry loops, error streaks and reverts, with time on retries shown on sessions, the overview and the replay timeline.
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

### Changed

- The audit prefilter uses ASCII word boundaries, so 73 rules check as fast as 28 did (214 ms against 220 ms on 26,000 real steps); each rule's own regex still makes the final call.
- Command rules ignore heredoc bodies that are data or another language's code (`python3 - <<'EOF'`).
- The share contract is version 3: inventory items carry tags. Index schema 14.

- MCP answers mask secrets the same way exports do, and the MCP server picks up new logs while it runs (at most every 30 seconds, before a tool call).
- Blame reads a file only when an agent session edited it, so the local API never serves other files.
- Pages other than the overview and replay load on first visit, and the team pages load only on the team server: the first download is a third smaller.
- Wording follows the voice guide everywhere: "errored" instead of "failed", "Retried 3 times" for retry loops, "on retries and reverts" instead of "stuck".
- Unknown addresses show "No page here" instead of the overview.

### Fixed

- Hook runs on the Setup page now count hooks started through `bunx`, `uvx`, `deno` or `uv run`; one helper now names a command's program for step titles, Setup and the fleet inventory.
- Codex `sandbox_mode` shows beside the approval mode on the Setup page instead of as an "ask" rule.
- `get_session` in MCP listed a file once per edit; now once per path and action.
- The MCP server echoed any protocol version a client sent; it now answers with one it speaks.
- Segmented controls (such as the Search filters) scroll inside themselves on phones instead of widening the page.
- The terminal search and the Compare page no longer panic on a timestamp shorter than a date.
- `agentcam index` folds worktrees into their repo in its project summary and says "no date" instead of "?".
