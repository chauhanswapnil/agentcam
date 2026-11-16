---
title: First run
description: What happens the first time you run agentcam, and where to look first.
---

```sh
agentcam
```

1. agentcam reads every past session from `~/.claude/projects/` and `~/.codex/sessions/` into its index. On months of logs this takes a little while; the UI opens as soon as it starts.
2. It starts a server on `http://127.0.0.1:4747` and opens your browser. If the port is taken it picks a free one.
3. It keeps watching the log folders. New steps show up within a second or two.

Use `--port` to pick a port and `--no-open` to skip the browser:

```sh
agentcam serve --port 4848 --no-open
```

## No sessions?

The first screen says which log folders agentcam looked in and whether each exists. If you use an agent for the first time after agentcam started, it notices the new folder within a few seconds, no restart needed.

If your agent keeps its config somewhere else, set `CLAUDE_CONFIG_DIR` or `CODEX_HOME` before you run agentcam, as you would for the agent.

## Where to look first

1. **Overview.** Your agent time this week against last week, and every project.
2. **Replay** a session you remember. Press space to play, `E` to jump to the next error.
3. **Profile.** The top fix is the change most likely to give time back.
4. **Audit.** Findings to review, if any.

## Keyboard

Press `⌘K` (or `Ctrl+K`) anywhere to search or jump to a page.
