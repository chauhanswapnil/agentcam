---
title: FAQ
description: Common questions about agentcam.
---

### Do I need to set anything up in my agent?

No. agentcam reads the logs your agents already write. The guard hook is the only optional piece, and only if you want it.

### Does it work on sessions from before I installed it?

Yes. The first run reads every log still on disk, and What lasted and blame work on those sessions too.

### Why is the cost different from my bill?

agentcam prices tokens at API list prices. On a subscription you pay the plan price instead. Read cost as a measure of how much work was done.

### Why does a number say "est."?

Some things the logs don't record: how long a call waited for your approval, or Claude Code's usage windows. agentcam works them out and marks them as estimates.

### A session is missing.

- With no sessions at all, the first screen lists the log folders agentcam looked in and whether each exists.
- Run `agentcam index` to read new logs now and print a summary.
- If your agent uses `CLAUDE_CONFIG_DIR` or `CODEX_HOME`, set the same variable for agentcam.
- Subagent sessions don't show on their own; they count toward the session that started them.

### Can I delete the index?

Yes. agentcam rebuilds it from the logs. Sessions whose logs the agent has since pruned are lost with it, so keep it if you want that history. Your rules and marks live in other files and stay.

### Does agentcam stop my agent?

Only if you add a guard rule and set up the hook. Findings never block anything.

### Can my company see my prompts?

Not through agentcam. A team server gets metadata only: counts, times, cost, findings, file paths. The Outbox page shows exactly what it would receive.

### Is it free?

Yes. agentcam is open source under the Apache License 2.0.
