---
title: Audit
description: Risk findings, approvals, and rules you can add and preview.
---

The Audit page shows what the risk rules found, how tool calls were allowed, and your guard rules.

Findings are things worth a second look, not verdicts. Agents do risky-looking things for good reasons all the time.

![Audit: the rules that fired, with a muted rule, and findings to review](../../../../../docs/screenshots/audit-review.png)

## Findings

73 built-in rules check every step when agentcam indexes:

- secrets in transcripts (masked before they're stored)
- sensitive files: `.env`, `~/.ssh`, cloud credentials
- dangerous commands: `rm -rf ~`, `curl | sh`, force pushes
- privilege and persistence changes: sudoers, shell startup files, cron, `authorized_keys`
- data sent out, and writes outside the project

Findings show on the replay timeline, on the step, and here. Press `R` in the replay to jump to the next one.

### Chains and outside content

Some risks only show across steps: a secret read and then an upload, or a command run right after a web page or MCP result held instructions. agentcam also flags a command that runs a package or sends data to an address that first appeared in a tool result, such as a fake Sentry error telling the agent to run a package. Each finding links back to the step the chain started from.

![A command the agent ran because a Sentry result told it to, linked back to that result](../../../../../docs/screenshots/chains.png)

### Reviewing

- **Looks fine** marks one finding as seen.
- **Mute in** a project hides a rule's findings there, or everywhere.

The **To review** list leaves both out. **All** shows them dimmed, with Undo. Marks only change what you see: findings are still stored and exported.

## Approvals

How each Claude Code tool call was allowed: by a rule, by auto mode, by the permission mode, or by you. Or how it was stopped.

The page lists calls that were stopped, and allow rules you could add so agents ask you less. Broad commands are never suggested. agentcam never edits your settings; you copy what you want.

![Audit: a guard rule and approvals with allow rules to copy](../../../../../docs/screenshots/audit.png)

## Your own rules

Add rules in YAML on this page, and **Preview matches** to see what they'd find in recent sessions before you save. See [Audit rules](../../guides/audit-rules/).

From the terminal:

```sh
agentcam scan --min high --range 30d
```
