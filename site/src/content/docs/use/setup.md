---
title: Setup, skills and MCP
description: Your agents' settings, skills and MCP servers on this machine, with things worth a look.
---

## Setup

The Setup page reads your agents' config on this machine:

- settings files: user, local, managed, and each recent project's
- instruction and memory files (CLAUDE.md, AGENTS.md)
- MCP servers from `~/.claude.json`, `.mcp.json` and Codex `config.toml`
- hooks and skills

Environment variables show by name only.

![Setup: settings worth a look, settings files, MCP servers and hooks](../../../../../docs/screenshots/setup.png)

It points out things worth a look, such as:

- approvals skipped by default
- an allow rule that covers every command
- an instruction that a setting overrides
- a hook pointing at a file that doesn't exist
- a CLAUDE.md that takes thousands of tokens
- MCP servers no session calls

agentcam honors `CLAUDE_CONFIG_DIR` and `CODEX_HOME`, and never changes these files.

## What MCP servers and skills tell the agent

The logs record the tool descriptions and instructions each MCP server gave your agent. agentcam checks them, and skills' text and scripts, for:

- hidden characters (decoded, so you can read them)
- orders aimed at the agent: "don't tell the user", "ignore previous instructions", "read ~/.ssh first"
- downloads piped into a shell
- descriptions that changed after you started using a server
- skill scripts that read credentials and call out to the network

MCP servers are also marked when they bring in outside content, run the latest release on every start, keep a token in plain text, or use plain http.

Nothing is uploaded or sent to a scanning service.

## Skills

Which skills are installed, which you or the agents ran, which sit unused, and how much context each one and the skill listing take.

![Skills: runs, steps under each skill, size when loaded](../../../../../docs/screenshots/skills.png)
