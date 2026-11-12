# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

### Clickable numbers

- Every number that sums up steps now opens them. A drawer on the right lists the steps behind a part of the time bar, a kind of token, a model, a cause of cache rebuilds or a fix, biggest first, and each row opens the replay at that step. The open number is kept in the address (`?behind=`), so the drawer can be linked and Back closes it. The route is `GET /api/behind`.
- The fix card names the sessions it shows and has "See all N". A row in the ledger opens to its twelve weeks, with a line where the fix was marked done, and the runs since.
- The replay header's numbers act: est. cost opens its split, peak context and errored jump to those steps, files changed goes to the list, the time strip jumps to the next step of its kind, and a dot on the timeline marks each cache rebuild.
- The Overview's stats open Profile, Tools and Files, "steps errored" and "on retries and reverts" filter the session list (`?has=errors|findings|retries`, also `?model=`), session badges open the first finding, retry, error or the subagents, and the turns rows open Compare.
- Setup: MCP servers open their calls and each check can show the row it is about. Tools: MCP server rows set the tool filter. What lasted: week and project rows narrow the files table. Thinking: model tiles narrow the list. Live: usage meters jump to the limits reached.
- Session rows on the Overview are a title button stretched over the row, so badges inside them are real links.

### Profiler

- Loud commands are a fix kind: a command whose output fills the context run after run, with a prompt that asks your agent for a quieter form. The index now keeps each tool output's full size (schema 16; the first run reads the logs again).
- The time bar says how big a session is before any work starts, and the replay header shows where one session's time went.
- A ledger on the Profile page: mark a fix "I did this" and agentcam sets the four weeks before against the time since, per 10 hours of agent time. It states the change only after a full week, and says so when a fix made no difference. "Not for me" sets a fix aside; both can be undone. Marks are kept in `profile.json` next to the index.
- A Profile page with fix cards: the few changes most likely to give time or money back (a slow command run often, a call you allow by hand every time, a prompt cache lost after a break), each with the steps behind it and a setting or a prompt to copy. The Overview shows the top one.
- "Where the time went" on the Overview: agent time split into the model working, tools running, waiting on approval (an estimate), ready for you, and redoing. It also says how much went on reading and searching before the first change of a turn.
- A Prompt cache panel on the Overview: the share of prompt tokens read from the cache, and each time the cache was lost and written again, with what it cost and why (after a break, a model switch, an effort change, or "cause not in the log"). The replay header counts a session's rebuilds; click it to go to the next one.

### Cost

- **Fixed:** Claude Code keeps most cache writes for an hour, which costs 200% of the input price. They were priced as 5-minute writes (125%), so estimates ran low.
- **Fixed:** `prices.yaml` next to the index was never read. It is now, and a file that can't be read is reported instead of ignored.
- **Fixed:** Claude Fable 5.2 and Mythos 5.2 had no price of their own and were charged at the Fable 5 rate; cache reads on Sonnet 5.5 were priced too high.
- Codex sessions now have a cost: the built-in list holds OpenAI's prices too, including the higher price for prompts over 272,000 tokens and for fast (priority) calls.
- Long prompts on Claude Haiku 5.5 and fast-mode calls are priced at their own rates.
- A Cost panel on the Overview shows where the estimate comes from (output, cache writes, cache reads, fresh input), what the cache saved, and cost by model. The replay header's cost shows the same split on hover.
- When prices change, sessions whose logs are gone are priced again from their stored token counts.
- The index gains two columns (schema 15). The first run after updating copies the index to `index.db.bak-v14` and reads the logs again.

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
