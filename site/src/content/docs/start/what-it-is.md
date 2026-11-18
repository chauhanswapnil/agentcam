---
title: What agentcam is
description: What agentcam does, who it's for, and what it never does.
---

Coding agents write a log of every session to your disk: each prompt, reply, command, file edit and tool call. agentcam reads those logs and turns them into something you can look at.

It works like a flight recorder. It shows what happened clearly, and flags only what deserves a second look, in plain words. It doesn't grade or police the agent.

## What you get

- **Overview** of months of agent work: time, sessions, projects, a year of activity.
- **Replay** of any session on a timeline, with every command, output and diff.
- **Search** across every prompt, reply, command, output and file path.
- **What lasted**: how much of what agents wrote is still in your code.
- **Profile**: where agent time and money went, and what to change.
- **Audit**: risk rules over every step, plus a guard that can ask before a call runs.
- **Exports** for reviews, auditors, SIEMs and OpenTelemetry.
- A **team server** you run yourself, for metadata across many laptops.

## Agents it reads

Claude Code and Codex. See [Supported agents](../../reference/agents/) for what each log holds.

## What it never does

- Write to, move or delete an agent's logs.
- Edit agent settings. Where a change would help, agentcam shows text for you to copy.
- Send anything off your machine, unless you join a team server you run.
- Stop an agent, unless you added a guard rule yourself.

## How it runs

agentcam is one Rust binary. It reads logs into a SQLite index in your data folder, then serves a web UI on `127.0.0.1:4747`. It watches the log folders and picks up new steps within a second or two, so the UI stays current while agents work.
