---
title: Privacy
description: What agentcam reads, what it stores, and what can ever leave your machine.
---

## What it reads

- Your agents' session logs. Read only: never edited, moved or uploaded.
- Your agents' settings, MCP config, hooks and skill folders, for the Setup page. Environment variables by name only.
- Project files, AGENTS.md and CLAUDE.md, to show what lasted and check instructions. It stores counts, never file contents.

## What it stores

The index in your data folder holds what the logs hold: prompts, replies, commands, output (cut to 32 KB per step) and diffs. Secrets the rules find are masked before they're stored.

The index keeps sessions after the agent deletes their logs. Claude Code prunes old logs; agentcam keeps the history.

**Treat the index, and any screenshot of agentcam, with the same care as the logs.** They can hold secrets the rules didn't find, and private code.

## What leaves the machine

Nothing, unless you do one of these:

| You run | What goes out | Where |
|---|---|---|
| `agentcam team join` | Metadata only (below) | A team server your company runs |
| `agentcam export otlp --endpoint …` | Session traces | The endpoint you name |

No telemetry, no update checks, no accounts. Fonts and scripts ship inside the binary.

### What a team server receives

Counts, times, tokens, cost, findings (secrets masked on the laptop), commits, pull request links, file paths relative to the project, and agent setup by name.

Never prompts, replies, thinking, command text, output, tool inputs or diffs.

The **Outbox** page shows exactly what would be sent. `agentcam team pause` stops it.

## The local server

- Listens on `127.0.0.1` only.
- Refuses requests addressed to any other host name, which blocks DNS rebinding.
- Refuses writes that don't come from agentcam's own page.
- Can't be framed by another site.

## Exports

Session exports, evidence bundles and MCP answers mask secrets again. Report cards hide titles, projects and file names by default.
