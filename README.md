# agentcam

**A dashcam for your coding agents.**

agentcam reads the session logs that Claude Code and Codex already write on your machine and turns them into something you can look at: months of agent work at a glance, a step-by-step replay of any session, full-text search, and a risk audit of what your agents touched. One local binary. No account, no telemetry, nothing leaves your machine.

![Overview: agent time, a year of activity, sessions and projects](docs/screenshots/overview.png)

> **Status:** 0.1.0, the first release. Used every day, still young: expect rough edges and say so in [issues](https://github.com/chauhanswapnil/agentcam/issues). The team server is a preview. Screenshots use made-up demo data.

## What it does

### See the work

**Overview.** Agent time, sessions, prompts, tool calls and files changed, compared with the previous period. A year-long activity heatmap, projects with weekly sparklines, every session grouped by day, and how long turns take. Agent worktrees count toward their repo, and subagents toward their parent session.

**Replay.** Scrub through a session on a zoomable timeline, or play it at 1×, 4× or 16×. The transcript follows along, split into turns: each prompt with how long it ran, its steps, cost, model, effort and mode. Each step shows its command and output, or a line-numbered diff, how it was approved, and the skill behind it. Keyboard: space, ←/→, `J`/`K`, `Shift+J` for the next file change, `E` for the next error, `R` for the next risk. Every step has its own link. Side panels show the agent's own task list as it stood at that step, plans it proposed, hooks that ran, and each subagent's share of the cost. Copy the command that resumes the session in its folder (`claude --resume` or `codex resume`), and open any changed file at its line in VS Code, Cursor or Zed.

![Replay: timeline, transcript, and a diff for the current step](docs/screenshots/replay.png)

**Live.** Which agents are working right now and what they just did, with a dot in the top bar while one works and a count of sessions waiting for you.

**Alerts.** Browser notifications when a session is waiting for you, is done for now, or has several steps in a row error, and when a session or a day passes a cost you set. Mute projects and set quiet hours. Settings stay in your browser.

![Live: usage windows, a working session and a quiet one](docs/screenshots/live.png)

**Search and thinking.** Full-text search across prompts, replies, commands, output and file paths, from the ⌘K palette or the search page. A Thinking page lists the longest or newest reasoning, with thinking time by model. Every hit opens the replay at that step.

![Search results with highlighted matches](docs/screenshots/search.png)

**Prompts.** Everything you typed to an agent, searchable, with the prompts you keep typing grouped together. A prompt you type in many sessions might belong in a slash command, a skill or AGENTS.md.

![Prompts: your prompts and the ones you keep asking](docs/screenshots/prompts.png)

### Did it work?

**What lasted.** Of the lines your agents wrote, how many are still in the code today, how many later agent edits rewrote, and how many changed since. By age, week, agent and model, project and file, and on every change in the replay. It works on your past sessions too, with nothing to install.

![What lasted: share of agent lines still in the code, by age, model and week](docs/screenshots/lasted.png)

**Who wrote this.** Open any file and see which session, model and prompt wrote each line that's still there, with a link to that step. Also `agentcam blame <file>` in the terminal. It works on past sessions, not just ones after you install.

![Who wrote this: each line of a file with the agent edit that wrote it](docs/screenshots/blame.png)

**Compare.** Put two groups of turns side by side: two models, agent versions, efforts, modes or skills, or before and after a date. agentcam finds comparisons worth a look, such as before and after you changed a project's AGENTS.md. It describes the changes ("24% shorter", "4 points more lines kept") and doesn't score them.

![Compare: two models side by side, with comparisons found in the sessions](docs/screenshots/compare.png)

**Instructions.** agentcam reads each project's AGENTS.md and CLAUDE.md and keeps the rules a log can check: commands to run after the last edit, the package manager, files not to edit, commands not to run. See how often sessions followed each one, and jump to the ones that didn't.

![Instructions: AGENTS.md rules and how often sessions followed them](docs/screenshots/instructions.png)

**Stuck steps and commits.** Retry loops, error streaks and reverts, with the time they took. The commits each session made, and the pull requests it opened.

### Cost and limits

**Profile.** Where agent time went (the model working, tools running, waiting on approval, ready for you, redoing) and the few changes most likely to give some back: a slow command run again and again, a call you allow by hand every time, a prompt cache lost after a break. Each fix comes with the steps behind it and something to copy, a setting or a prompt for your agent. agentcam never edits your files. Mark a fix done and a ledger sets the weeks before against the time since, and says so when it made no difference.

![Profile: where the time went, fix cards and the ledger](docs/screenshots/profile.png)

**Cost.** Estimated cost per step, turn, session, project and day at API list prices, from an editable price table that covers Anthropic and OpenAI models. See where it comes from (output, cache writes, cache reads, fresh input), what the prompt cache saved, and each time the cache was lost and written again. A context window chart with compactions marked.

**Usage limits.** Codex's 5-hour and weekly windows, with a 7-day chart and reset times, and every time Claude Code or Codex reached a limit. Claude Code doesn't log its windows, so agentcam estimates them from logged tokens: the current 5-hour block, its pace and where it is heading, compared with your highest past block, plus a rolling 7-day total. Both are marked as estimates.

### Safety

**Risk audit.** 73 built-in rules flag secrets in transcripts (masked before they're stored), sensitive files, dangerous commands, privilege and persistence changes (sudoers, shell startup files, cron, `authorized_keys`), data sent out, and writes outside the project. Findings show on the replay timeline and the Audit page. Mark a finding "looks fine" or mute a rule for one project, and the To review list keeps only what you haven't seen. Add your own rules in YAML from the Audit page and preview what they'd find before saving; [the rules guide](docs/audit-rules.md) covers every field.

**Chains and outside content.** Some risks only show across steps: a secret read and then an upload, a stopped call followed by another way around it, a command run right after a web page or MCP result held instructions. agentcam also notices when a command runs a package or sends data to an address that came from a tool result, such as a fake Sentry error telling the agent to run a package, and decodes hidden characters in tool output. Each finding links back to the step the chain started from.

![A command the agent ran because a Sentry result told it to, linked back to that result](docs/screenshots/chains.png)

**Approvals.** How each Claude Code tool call was allowed (a rule, auto mode, the permission mode, you) or stopped. The Audit page lists calls that were stopped and allow rules you could add, so agents ask you less. Broad commands are never suggested, and agentcam never edits your settings: you copy what you want.

**Block next time.** Turn a risky step into a guard rule. A Claude Code or Codex `PreToolUse` hook runs `agentcam guard` before each tool call and stops calls that match, with a message to the agent.

![Audit: a guard rule and approvals with allow rules to copy](docs/screenshots/audit.png)

![Audit: the rules that fired, with a muted rule, and findings to review with Looks fine and Mute buttons](docs/screenshots/audit-review.png)

### Skills and tools

**Skills.** Which skills exist, which the agents or you ran, which sit unused, and how much context each one and the skill listing take.

![Skills: runs, steps under each skill, size when loaded](docs/screenshots/skills.png)

**Setup.** Your agents' settings files, instruction and memory files, MCP servers, hooks and skills on this machine, with things worth a look: approvals skipped by default, allow rules that cover every command, an instruction that a setting overrides, a hook pointing at a missing file, a CLAUDE.md that takes thousands of tokens, MCP servers nobody calls. Reads `CLAUDE_CONFIG_DIR` and `CODEX_HOME` when they're set. agentcam never changes these files.

**What MCP servers and skills tell the agent.** The tool descriptions and instructions each MCP server gave your agents, read from the logs, checked for hidden characters, orders for the agent ("don't tell the user", "read ~/.ssh first") and descriptions that changed after you started using them. Skills' text and scripts get the same check, plus scripts that read credentials and call out. MCP servers that bring in outside content, run the latest release on every start, keep a token in plain text or use plain http are marked. Nothing is uploaded or sent to a scanning service.

![Setup: settings worth a look, settings files, MCP servers and hooks](docs/screenshots/setup.png)

**Ask your history (MCP).** `agentcam mcp` is a read-only MCP server, so your agents can look up past work: how a bug was fixed before, what a repo's AGENTS.md asks and what tends to get missed, which edits to a file lasted, who wrote a line. Add it with `claude mcp add agentcam -- agentcam mcp`, or in Codex's `config.toml` as `[mcp_servers.agentcam]` with `command = "agentcam"` and `args = ["mcp"]`. Answers have secrets masked. [Every tool](docs/mcp.md).

**Tools and files.** Use, errors and time per tool; skills, plugins and MCP servers behind steps; the most changed files and each file's history.

### Share and export

**Session export.** Download a session as Markdown (prompts, replies, commands with the start of their output, diffs) or JSON, with secrets masked. Every table (sessions, tools, files, commits, findings) downloads as CSV.

**Evidence bundles.** A session or a date range as one JSON file for an auditor or an incident review: who ran it and when, prompts, every step and how it was allowed, file changes, findings, commits. Secrets are masked, and a SHA-256 checksum shows whether the file changed later (`agentcam evidence verify`). Findings also export for a SIEM as NDJSON or OCSF. [Evidence and SIEM](docs/evidence-and-siem.md).

**Report cards.** PNG cards for a session or your week, with titles, projects and file names hidden by default.

**OpenTelemetry.** `agentcam export otlp` sends sessions as traces to Jaeger, Grafana Tempo or Honeycomb, with subagents nested under the call that started them.

**Outbox.** Exactly what a team server receives from this machine: counts, times, tokens, findings, file paths relative to the project, and agent setup by name. Never prompts, output or diffs. Nothing leaves until you join a team.

## For teams

Run your own team server, and each laptop sends it metadata only. Vendor dashboards each count usage of their own agent. agentcam covers Claude Code and Codex together, works on any plan or API key, and shows what the work came to: lines still in the code, rules followed, commits and pull requests.

- **Insights:** agent time, cost, lines still there and rules followed, by week, team, repo and model.
- **Value:** cost per commit and per pull request, lines kept per dollar, seats against use at API prices, machines active each week, and models and agent versions side by side.
- **Governance:** what the risk rules found across the team, with secrets masked on each laptop before anything is sent.
- **Fleet:** agent versions, MCP servers, plugins, skills and hooks on each machine, and settings worth a look, such as approvals skipped by default or the sandbox off.
- **Baseline:** set the setup every machine should have (the guard hook, no secrets in MCP configs, approved MCP servers, a minimum agent version) and see which machines differ, since when, and what changed.
- **SIEM:** findings from every machine to Splunk, Sentinel or anything that reads OCSF, by pull or webhook.

People's names stay hidden unless an admin turns them on (logged), and no view ranks people.

![Team insights: time, cost and what lasted by week, team and repo](docs/screenshots/team-insights.png)

![Value: cost per commit and PR, lines kept per dollar, seats, and models side by side](docs/screenshots/team-value.png)

![Governance: findings across the team by severity and rule](docs/screenshots/team-governance.png)

![Fleet: settings worth a look, agent versions and what's installed across machines](docs/screenshots/team-fleet.png)

```sh
# On the server (put TLS in front when laptops reach it over a network)
agentcam team serve --bind 0.0.0.0:4850     # prints the first admin token
agentcam team invite Platform               # or make invites on the Settings page

# On each laptop
agentcam team join https://agentcam.example.com ac_inv_...
```

[Running a team server](docs/team-server.md) covers the proxy, tokens, removing a machine, backups and upgrades.

## Install

macOS and Linux, with [Homebrew](https://brew.sh):

```sh
brew install chauhanswapnil/tap/agentcam
```

Anywhere with Node 18+:

```sh
npx agentcam
```

Or the install script, which puts the binary in `~/.cargo/bin`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/chauhanswapnil/agentcam/releases/latest/download/agentcam-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/chauhanswapnil/agentcam/releases/latest/download/agentcam-installer.ps1 | iex"
```

With Rust: `cargo install agentcam`. Binaries for every platform are on the [releases page](https://github.com/chauhanswapnil/agentcam/releases).

Then run:

```sh
agentcam
```

It indexes your logs, starts a server on `http://127.0.0.1:4747` and opens your browser. The first run reads every past session, so there's something to see straight away.

## Commands

| Command | What it does |
|---|---|
| `agentcam` / `agentcam serve` | Index in the background, watch for new logs, open the UI (`--port`, `--no-open`) |
| `agentcam index [--full]` | Read new and changed logs, print a summary |
| `agentcam search <words>` | Full-text search from the terminal |
| `agentcam scan [--min high] [--range 30d]` | Print what the risk rules found |
| `agentcam export otlp [--range 7d] [--out f] [--endpoint url]` | Sessions as OpenTelemetry traces (OTLP/JSON); sends data only with `--endpoint` |
| `agentcam guard` | The hook command: reads a tool call on stdin, exits 2 to block it |
| `agentcam evidence [--session id] [--range 7d]` | An evidence bundle; `agentcam evidence verify <file>` checks one |
| `agentcam export findings [--format ocsf] [--since ts]` | Findings one per line, for a SIEM |
| `agentcam blame <file>` | Which agent session wrote each line of a file today |
| `agentcam mcp` | Read-only MCP server over stdio for your agents |
| `agentcam team serve [--data dir] [--bind addr]` | Run a team server |
| `agentcam team invite <team>` / `team token` | Make an invite code or a viewer token on the server machine |
| `agentcam team machines` / `team remove <machine>` | List machines, or remove one and what it sent |
| `agentcam team viewers` / `team revoke <id>` | List viewer tokens, or revoke one |
| `agentcam team join <server> <invite>` | Join a team; then `team sync`, `status`, `pause`, `resume`, `leave` |

All of them take `--db <file>` (or `AGENTCAM_DB`) to use another index. By default the index lives in your data folder (`~/Library/Application Support/agentcam/` on macOS), next to `rules.yaml` (your rules), `guard.yaml` (your guard rules) and `reviews.json` (findings you marked and rules you muted).

## Supported agents

| Agent | Logs read from | Guard hook |
|---|---|---|
| Claude Code | `~/.claude/projects/` (including subagents) | `~/.claude/settings.json` |
| Codex | `~/.codex/sessions/` (newer and older formats) | `~/.codex/hooks.json` (trust it in `/hooks`) |

For both agents agentcam reads what the logs record: turns and their settings, approvals (Claude Code), thinking, skills, usage limits, pull requests (Claude Code), reasoning tokens (Codex), and files changed outside a session.

## Privacy

- agentcam only reads agent logs. It never edits, moves or uploads them, and never edits agent settings; the Audit page shows the hook config for you to add.
- The server listens on `127.0.0.1` only, refuses requests addressed to any other host name, and refuses cross-site writes.
- Nothing leaves the machine unless you run `agentcam team join`. Then it sends metadata to a server your company runs, over https; `agentcam team pause` stops it.
- Secrets that the rules find are masked before they reach the index, and again in session exports and MCP answers. Report cards hide titles, projects and file names by default.
- To show what lasted and check instructions, agentcam reads project files, AGENTS.md and CLAUDE.md on this machine. It stores counts, never file contents. It reads skill folders for names, descriptions and sizes only.
- Agent logs can hold secrets and private code. Treat agentcam's index, and any screenshot of it, with the same care as the logs.

The index keeps sessions after the agent deletes their logs. Claude Code prunes old logs; agentcam keeps the history.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md). [AGENTS.md](AGENTS.md) covers the layout, commands, design rules and how parsing, the audit and the guard work, for people and coding agents alike.

### Try it with demo data

To look around without your own logs, or to retake these screenshots, make a folder of made-up sessions with the script in this repo (Python 3, nothing to install) and point agentcam at it. Your real logs and index aren't touched:

```sh
git clone https://github.com/chauhanswapnil/agentcam && cd agentcam
python3 scripts/demo-data.py /tmp/agentcam-demo
HOME=/tmp/agentcam-demo agentcam --db /tmp/agentcam-demo/index.db
```

From a checkout without agentcam installed, use `cargo run --` in place of `agentcam`.

## License

[Apache License 2.0](LICENSE).
