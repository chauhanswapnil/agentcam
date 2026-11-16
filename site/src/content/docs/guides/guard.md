---
title: Block next time (guard)
description: Turn a step into a rule that a PreToolUse hook checks before the agent runs a tool.
---

The guard lets you stop a kind of tool call before it runs, with a message to the agent. It does nothing until you add a rule and set up the hook.

## How it works

1. You add a guard rule: a tool name, a pattern, and a message.
2. A `PreToolUse` hook in Claude Code or Codex runs `agentcam guard` before each tool call.
3. `agentcam guard` reads the call on stdin. If a rule matches, it exits with code 2 and prints your message, which the agent sees. Otherwise it exits 0 and the call runs.

The guard is fast (it doesn't open the index) and fails open: if anything goes wrong, the call runs. A broken guard never stops your agent.

## Add a rule

From the replay, open a step and choose **Block next time**. agentcam suggests a pattern from the step, and shows how many past calls it would have matched. Edit it, write the message, and save.

Rules live in `guard.yaml` next to the index:

```yaml
- id: no-prod-deploy
  tool: Bash                 # Claude Code tool name, or * for any
  pattern: '\bfly deploy\b'  # regex, checked against the command, path or URL
  reason: Check with the user before deploying to production.
```

Write messages that ask, not punish: say what to check and with whom.

## Set up the hook

agentcam never edits agent settings. The Audit page shows the hook JSON for each agent, with the full path to your `agentcam` binary filled in, and whether a hook is already set up.

**Claude Code**, in `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "*", "hooks": [{ "type": "command", "command": "agentcam guard" }] }
    ]
  }
}
```

Merge it with any hooks you already have, then restart Claude Code.

**Codex**, in `~/.codex/hooks.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": ".*", "hooks": [{ "type": "command", "command": "agentcam guard" }] }
    ]
  }
}
```

Then open `/hooks` in Codex and trust the new hook. Codex skips hooks until you trust them.

If you use another index (`--db`), pass the same `--db` before `guard` in the command.

## Tool names

Codex commands reach the hook as `Bash`, like Claude Code's, so one `Bash` rule covers both.
