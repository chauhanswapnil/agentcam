# Evidence bundles and SIEM export

For a security review, an audit, or a look at what happened in an incident.

## Evidence bundles

A bundle is one JSON file covering one session or every session in a date range. It holds:

- who ran the agent (OS user, machine id) and when
- the agent, model and version, the project and its git remote
- each prompt in full, and every step with how it was allowed (`rule`, `auto`, `mode`, `you`, `rejected`, `blocked-*`) and whether it errored
- files changed, with diffs (cut at 16 KB each)
- risk findings, with the step a chain started from
- commits and pull requests, and subagents as their own entries

Secrets that the audit rules find are masked, as in session exports. Output is cut to the first 4,000 characters per step. One bundle holds up to 500 sessions; narrow the range or pick a project for more.

Get one from the replay (**Evidence**), from the Audit page (**Download evidence** for the range on screen), or the terminal:

```sh
agentcam evidence --session 5f017cac-10e0-495e-9f01-84d9291ad9d7
agentcam evidence --range 30d --project ~/code/payments-api --out march.json
```

### Checking a bundle

Each bundle carries a SHA-256 of its own content (`integrity.content_sha256`): the JSON with keys sorted, without the `integrity` field.

```sh
agentcam evidence verify march.json
```

It says `intact`, or that the file changed after it was made. The checksum shows the file wasn't edited since; it doesn't prove the logs it came from were complete, and anyone who can edit the file can also write a new checksum. Keep bundles somewhere write-once if that matters to your auditor.

## Findings for a SIEM

Findings as one JSON object per line, oldest first:

- `ndjson`: agentcam's own records (rule, title, severity, masked excerpt, session, step, agent, model, project, machine, OS user)
- `ocsf`: OCSF 1.3 Detection Finding (class 2004), which Splunk, Microsoft Sentinel, AWS Security Lake and others read

Never prompts, output or diffs.

```sh
agentcam export findings --format ocsf --range 30d --out findings.ndjson
# Ship only new ones: pass the last `ts` you sent.
agentcam export findings --format ocsf --since 2026-11-06T18:00:00Z
```

The Audit page has the same as downloads, at the severity and range on screen.

### From the team server

With a team server, findings from every machine are in one place:

- **Pull:** `GET /api/team/findings/export?format=ocsf&since=<RFC 3339>` with a viewer token (`Authorization: Bearer ac_view_...`).
- **Push:** an admin sets a webhook on the Settings page: generic NDJSON, OCSF, or Splunk HTTP Event Collector. Each sync sends the findings it brought. The URL must be https (or loopback); the token is stored on the server and never shown again. The last push and its result show under the form.

OS user names are included only when an admin has turned names on.
