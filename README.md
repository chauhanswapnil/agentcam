# agentcam

**A dashcam for your coding agents.**

agentcam reads the session logs that Claude Code and Codex already write on your machine and turns them into something you can look at: months of agent work at a glance, a step-by-step replay of any session, full-text search, and a risk audit of what your agents touched. One local binary. No account, no telemetry, nothing leaves your machine.

![Overview: agent time, a year of activity, sessions and projects](docs/screenshots/overview.png)

> **Status:** works on my machine every day, not yet packaged. Install from source below. Screenshots use made-up demo data.

## What it does

**Overview.** Agent time, sessions, prompts, tool calls and files changed, compared with the previous period. A year-long activity heatmap, projects with weekly sparklines, and every session grouped by day. Agent worktrees count toward their repo, and subagents toward their parent session.

**Replay.** Scrub through a session on a zoomable timeline, or play it at 1×, 4× or 16×. The transcript follows along, and each step shows its command and output, or a line-numbered diff. Keyboard: space, ←/→, `J`/`K`, `Shift+J` for the next file change, `E` for the next failure, `R` for the next risk. Every step has its own link.

![Replay: timeline, transcript, and a diff for the current step](docs/screenshots/replay.png)

**Live.** Which agents are working right now, what they just did, and how full your usage windows are: Codex's 5-hour and weekly limits with reset times, and every time Claude Code or Codex reached a limit.

**What lasted.** Of the lines your agents wrote, how many are still in the code today, how many later agent edits rewrote, and how many changed since. Broken down by age, week, agent and model, project and file, and shown on every change in the replay. It works on your past sessions too, with nothing to install.

**Search.** Full-text search across prompts, replies, commands, output and file paths, from the ⌘K palette or the search page. Every hit opens the replay at that step.

![Search results with highlighted matches](docs/screenshots/search.png)

**Risk audit.** 28 built-in rules flag secrets in transcripts (masked before they're stored), sensitive files (`.env`, `~/.ssh`, cloud credentials, keychains), dangerous commands (`rm -rf`, `sudo`, `curl | sh`, force pushes, `git reset --hard`), package installs, domains contacted, and writes outside the project. Findings show on the replay timeline and the Audit page. Add your own rules in YAML from the Audit page.

**Block next time.** Turn a risky step into a guard rule. A Claude Code or Codex `PreToolUse` hook runs `agentcam guard` before each tool call and stops calls that match, with a message to the agent.

![Audit: rules that fired, latest findings, and a guard rule](docs/screenshots/audit.png)

**Observability.** Estimated cost per step, session, project and day from an editable price table. A context window chart with compactions marked. Stuck detection for retry loops, failure streaks and reverts, with the time they cost. The commits each session made. A trace view of every session, and `agentcam export otlp` to send sessions to Jaeger, Grafana Tempo or Honeycomb.

**Also:** a tool calls page (use, failures and time per tool), a files page (the most changed files and each file's history), shareable report cards (PNG) for a session or your week, and live updates as agents write.

## Install

Not published yet. You need Rust (stable) and Node 20+.

```sh
git clone https://github.com/chauhanswapnil/agentcam
cd agentcam
npm --prefix web ci && npm --prefix web run build
cargo install --path crates/agentcam
agentcam
```

`agentcam` indexes your logs, starts a server on `http://127.0.0.1:4747` and opens your browser. Packages for `cargo install agentcam`, `npx agentcam` and Homebrew are coming.

## Commands

| Command | What it does |
|---|---|
| `agentcam` / `agentcam serve` | Index in the background, watch for new logs, open the UI (`--port`, `--no-open`) |
| `agentcam index [--full]` | Read new and changed logs, print a summary |
| `agentcam search <words>` | Full-text search from the terminal |
| `agentcam scan [--min high] [--range 30d]` | Print what the risk rules found |
| `agentcam export otlp [--range 7d] [--out f] [--endpoint url]` | Sessions as OpenTelemetry traces (OTLP/JSON); sends data only with `--endpoint` |
| `agentcam guard` | The hook command: reads a tool call on stdin, exits 2 to block it |

All of them take `--db <file>` (or `AGENTCAM_DB`) to use another index. By default the index lives in your data folder (`~/Library/Application Support/agentcam/` on macOS), next to `rules.yaml` (your rules) and `guard.yaml` (your guard rules).

## Supported agents

| Agent | Logs read from | Guard hook |
|---|---|---|
| Claude Code | `~/.claude/projects/` (including subagents) | `~/.claude/settings.json` |
| Codex | `~/.codex/sessions/` (newer and older formats) | `~/.codex/hooks.json` (trust it in `/hooks`) |

Cursor and others are planned.

## Privacy

- agentcam only reads agent logs. It never edits, moves or uploads them, and never edits agent settings; the Audit page shows the hook config for you to add.
- The server listens on `127.0.0.1` only, refuses requests addressed to any other host name, and refuses cross-site writes.
- Secrets that the rules find are masked before they reach the index. Report cards hide titles, projects and file names by default.
- Agent logs can hold secrets and private code. Treat agentcam's index, and any screenshot of it, with the same care as the logs.

The index keeps sessions after the agent deletes their logs. Claude Code prunes old logs; agentcam keeps the history.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md). [AGENTS.md](AGENTS.md) covers the layout, commands, design rules and how parsing, the audit and the guard work, for people and coding agents alike.

To try agentcam without your own logs, or to retake these screenshots:

```sh
python3 scripts/demo-data.py /tmp/agentcam-demo
HOME=/tmp/agentcam-demo cargo run -- --db /tmp/agentcam-demo/index.db serve
```

## License

[Apache License 2.0](LICENSE).
