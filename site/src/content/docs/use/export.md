---
title: Export and share
description: Session exports, CSV, report cards, OpenTelemetry and the Outbox.
---

## Session export

From the replay, download a session as:

- **Markdown**: prompts, replies, commands with the first 20 lines of output, and diffs. Thinking is left out.
- **JSON**: everything in the index for that session.

Secrets the rules find are masked in both.

## CSV

Every table (sessions, tools, files, commits, findings) downloads as CSV, with the rows it shows.

## Report cards

PNG cards for a session or your week. Titles, projects and file names are hidden by default; show them if you want.

## Evidence bundles

One JSON file for an auditor or an incident review, with a checksum. See [Evidence and SIEM](../../guides/evidence-and-siem/).

## OpenTelemetry

Send sessions as traces to Jaeger, Grafana Tempo, Honeycomb or any OTLP collector. Subagents nest under the call that started them.

```sh
agentcam export otlp --range 7d --out traces.json
agentcam export otlp --endpoint http://localhost:4318/v1/traces
```

It writes a file unless you pass `--endpoint`. Add headers with `--header name=value`.

## Outbox

The Outbox page shows exactly what a team server would receive from this machine. Nothing leaves until you join a team. See [Running a team server](../../guides/team-server/).
