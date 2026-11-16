---
title: Files and folders
description: Where agentcam keeps its index and your settings, and what each file holds.
---

## The data folder

| OS | Folder |
|---|---|
| macOS | `~/Library/Application Support/agentcam/` |
| Linux | `~/.local/share/agentcam/` (or `$XDG_DATA_HOME/agentcam/`) |
| Windows | `%LOCALAPPDATA%\agentcam\` |

With `--db <file>` or `AGENTCAM_DB`, the files below sit next to that index instead.

| File | Holds |
|---|---|
| `index.db` | The index: sessions, steps, findings, counts. Safe to delete; agentcam rebuilds it from the logs. Sessions whose logs are gone are lost with it. |
| `rules.yaml` | Your audit rules. See [Audit rules](../../guides/audit-rules/). |
| `guard.yaml` | Your guard rules. See [Guard](../../guides/guard/). |
| `reviews.json` | Findings marked "looks fine" and rules muted |
| `profile.json` | Fixes marked done or set aside |
| `prices.yaml` | Your own model prices, if you add one |
| `machine-id` | A random id for this machine, used by the team server |
| `team.json` | The team link and token, if you joined one (mode 600) |
| `team-sync.json` | What was sent to the team server, and when |

Your marks and rules live outside `index.db`, so they survive a rebuild.

## Prices

`prices.yaml` adds to the built-in table; a row with the same `match` replaces the built-in one. The longest matching `match` prefix wins. Prices are US dollars per million tokens:

```yaml
- { match: my-company-model, input: 3.00, output: 15.00 }
- { match: gpt-6-mini, input: 0.30, output: 1.20, cache_read: 0.03 }
```

| Field | Meaning |
|---|---|
| `match` | A model id or the start of one |
| `input`, `output` | Fresh input and output tokens |
| `cache_read` | Tokens read from the prompt cache (default 10% of input) |
| `cache_write` | Cache writes kept 5 minutes (default 125% of input) |
| `cache_write_1h` | Cache writes kept 1 hour (default 200% of input) |
| `long` | Prices for a call whose prompt is over a size: `{ over, input, output, cache_read }` |
| `fast`, `flex` | Multiples for fast (default 2) and flex (default 0.5) calls |

If the file doesn't parse, agentcam leaves it out and the Cost page says so.

## Logs it reads

| Agent | Folder |
|---|---|
| Claude Code | `~/.claude/projects/` (or `$CLAUDE_CONFIG_DIR/projects/`) |
| Codex | `~/.codex/sessions/` (or `$CODEX_HOME/sessions/`) |

agentcam only reads these. It never writes to, moves or deletes them.

## Backups before upgrades

When a new version changes the index, agentcam copies it first to `index.db.bak-v<N>`. An index made by a newer agentcam is refused, never wiped.
