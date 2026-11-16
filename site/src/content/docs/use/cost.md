---
title: Cost and usage limits
description: Estimated cost at API list prices, the prompt cache, context size and usage windows.
---

## Cost

agentcam estimates cost per step, turn, session, project and day, at API list prices. On a subscription you pay the plan price, so read these as a measure of how much work was done, not a bill.

- **Where it comes from**: fresh input, output, cache writes and cache reads.
- **What the prompt cache saved**: the same calls priced with no cache.
- **Cache rebuilds**: each time the cache was lost and written again, with its cause (a model switch, a break longer than the cache is kept, an effort change, or unknown) and what it cost.
- **Context**: a chart of the context window over a session, with compactions marked.

### Prices

A built-in table covers Anthropic and OpenAI models. To add or change a price, put a `prices.yaml` next to the index (see [Files and folders](../../reference/files/)). The longest matching model prefix wins. A model with no price shows its cost as unknown, never guessed.

## Usage limits

| Agent | What agentcam shows |
|---|---|
| Codex | The 5-hour and weekly windows Codex logs, with a 7-day chart and reset times |
| Claude Code | An estimate from logged tokens: the current 5-hour block, its pace and where it's heading, against your highest past block, plus a rolling 7-day total |

Claude Code doesn't log its windows, so its numbers are always marked as estimates. Both agents' "limit reached" messages are listed with their times.

Nothing here comes from an API. It's all read from the logs.
