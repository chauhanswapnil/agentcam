---
title: Commands
description: Every agentcam command and its options.
---

Every command takes `--db <file>` (or the `AGENTCAM_DB` environment variable) to use another index. Run `agentcam <command> --help` for the full list of options.

## Local

| Command | What it does |
|---|---|
| `agentcam` | Same as `agentcam serve` |
| `agentcam serve [--port 4747] [--no-open]` | Index in the background, watch for new logs, serve the UI on `127.0.0.1`. Falls back to a free port if the one asked for is taken. |
| `agentcam index [--full]` | Read new and changed logs, then print a summary. `--full` re-reads every log. |
| `agentcam search <words> [--limit 20]` | Full-text search from the terminal |
| `agentcam scan [--min medium] [--range all] [--all]` | Index, then print what the risk rules found. `--all` prints every finding, not a summary per rule. |
| `agentcam blame <file>` | Which agent session wrote each line of a file today |
| `agentcam guard` | The hook command: reads a tool call on stdin, exits 2 to block it. See [Guard](../../guides/guard/). |
| `agentcam mcp` | Read-only MCP server over stdio. See [MCP](../../guides/mcp/). |

## Export

| Command | What it does |
|---|---|
| `agentcam evidence [--session id] [--range 7d] [--project path] [--out file]` | An evidence bundle as JSON with a checksum |
| `agentcam evidence verify <file>` | Check that a bundle wasn't changed after it was made |
| `agentcam export findings [--format ndjson\|ocsf] [--range 30d] [--since ts] [--severity low] [--out file]` | Findings one per line, for a SIEM |
| `agentcam export otlp [--range 7d] [--project path] [--out file] [--endpoint url] [--header name=value]` | Sessions as OpenTelemetry traces. Sends data only with `--endpoint`. |

Ranges are `7d`, `30d`, `90d` or `all`.

## Team server (on the server machine)

| Command | What it does |
|---|---|
| `agentcam team serve [--data dir] [--bind 127.0.0.1:4850]` | Run a team server. Prints the first admin token once. |
| `agentcam team invite <team> [--uses 50] [--days 7]` | An invite code for laptops |
| `agentcam team token [--name Admin] [--role admin\|viewer]` | A token for the team UI |
| `agentcam team machines` | List joined machines |
| `agentcam team remove <machine>` | Remove a machine and everything it sent |
| `agentcam team viewers` | List viewer tokens |
| `agentcam team revoke <id>` | Revoke a viewer token |

## Team server (on a laptop)

| Command | What it does |
|---|---|
| `agentcam team join <server> <invite> [--allow-http]` | Join a team |
| `agentcam team sync` | Send changed metadata now |
| `agentcam team status` | The link, the last sync and the last error |
| `agentcam team pause` / `resume` | Stop sending, or start again |
| `agentcam team leave` | Forget the link. The server keeps what it has. |

See [Running a team server](../../guides/team-server/).
