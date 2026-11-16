---
title: Supported agents
description: Which agents agentcam reads, from where, and what each log records.
---

| Agent | Logs read from | Guard hook |
|---|---|---|
| Claude Code | `~/.claude/projects/`, including subagents | `~/.claude/settings.json` |
| Codex | `~/.codex/sessions/`, newer and older formats | `~/.codex/hooks.json` (trust it in `/hooks`) |

## What the logs record

| | Claude Code | Codex |
|---|---|---|
| Prompts, replies, tool calls, diffs | ✓ | ✓ |
| Turns with model, effort and mode | ✓ | ✓ |
| Thinking | ✓ | ✓ |
| Subagents | ✓ | background threads |
| How each call was allowed | ✓ | |
| Skills and plugins behind a step | ✓ | MCP apps |
| Usage windows | when a limit is reached | 5-hour and weekly readings |
| Pull requests linked | ✓ | |
| Reasoning tokens | | ✓ |
| Files changed outside the session | ✓ | |

Where a log doesn't record something, agentcam leaves it blank rather than guess.

## When the format changes

Both agents change their log format between versions without notice. agentcam reads what it knows and keeps the rest. A line it can't read is skipped, never fatal. If a new agent version shows something wrong, [open an issue](https://github.com/chauhanswapnil/agentcam/issues), but never paste a real log: describe its shape instead.

## Other agents

agentcam reads Claude Code and Codex only, and aims to read them well.
