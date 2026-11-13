# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## 0.2.0 - 2026-11-12

Where your agents' time and money went, what to change, and every number opens the steps behind it. Cost estimates are also more accurate, so totals you saw in 0.1.0 will move.

### Upgrading

- The index gains three columns (schema 16). The first run after updating copies it to `index.db.bak-v14` and reads your logs again; on a few hundred sessions that takes seconds. Sessions whose logs are gone keep what they had and are priced again from their stored tokens.

### Cost

- **Fixed:** Claude Code keeps most cache writes for an hour, which costs 200% of the input price. They were priced as 5-minute writes (125%), so estimates ran low.
- **Fixed:** `prices.yaml` next to the index was never read. It is now, and a file that can't be read is reported instead of ignored.
- **Fixed:** Claude Fable 5.2 and Mythos 5.2 had no price of their own and were charged at the Fable 5 rate; cache reads on Sonnet 5.5 were priced too high.
- Codex sessions now have a cost: the built-in list holds OpenAI's prices too, including the higher price for prompts over 272,000 tokens and for fast (priority) calls. Long prompts on Claude Haiku 5.5 and fast-mode calls are priced at their own rates.
- A Cost panel on the Overview shows where the estimate comes from (output, cache writes, cache reads, fresh input), what the cache saved, and cost by model.

### Profile

- **Where the time went:** agent time split into the model working, tools running, waiting on approval (an estimate), ready for you, and redoing, on the Overview, the new Profile page and each replay. It also says how much went on reading and searching before the first change of a turn, and how big a session is before any work starts.
- **Fixes:** the few changes most likely to give time or money back: a slow command run again and again, a command whose output fills the context, a call you allow by hand every time, a prompt cache lost after a break. Each comes with the steps behind it and a setting or a prompt to copy. agentcam never edits your files.
- **Ledger:** mark a fix "I did this" and agentcam sets the four weeks before against the time since, per 10 hours of agent time. It states the change only after a full week, and says so when a fix made no difference. "Not for me" sets a fix aside; both can be undone. Marks are kept in `profile.json` next to the index; a file that can't be read is reported and kept as `profile.json.bad`.
- **Prompt cache:** the share of prompt tokens read from the cache, and each time the cache was lost and written again, with what it cost and why (after a break, a model switch, an effort change, or "cause not in the log").

### Every number opens what is behind it

- A drawer lists the steps behind a part of the time bar, a kind of token, a model, a cause of cache rebuilds or a fix, biggest first; each row opens the replay at that step. The open number is kept in the address (`?behind=`), so it can be linked and Back closes it.
- Replay: est. cost opens its split, peak context and errored jump to those steps, the time strip jumps to the next step of its kind, and a dot on the timeline marks each cache rebuild.
- Overview: the stats open Profile, Tools and Files; "steps errored" and "on retries and reverts" filter the session list; session badges open the first finding, retry or error; turn bars open their turns.
- Setup, Tools, What lasted, Thinking, Live and Compare: rows, tiles and meters narrow the page or open the steps they count. Table rows open with Enter or Space.
- New routes: `GET /api/cost`, `/api/profile`, `/api/cache`, `/api/fixes`, `/api/ledger`, `/api/behind`; `/api/sessions` takes `model` and `has`.

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
