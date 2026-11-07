# Audit rules

agentcam checks every step of every session against a set of rules when it indexes your logs. A match is a finding: it shows on the replay timeline, on the step, and on the Audit page. Findings are things worth a second look, not verdicts; agents do risky-looking things for good reasons all the time.

73 rules ship built in (`crates/agentcam-core/rules/default.yaml`):

| Group | What they flag |
|---|---|
| Secrets | AWS, GitHub, Anthropic, OpenAI, Slack, Stripe and Google keys, private keys. Masked before they're stored. |
| Sensitive files | `.env`, `~/.ssh`, cloud and package-manager credentials, keychains, browser profiles, secret managers, another process's environment |
| Dangerous commands | `rm -rf` on home or root, `sudo`, `chmod 777`, `curl \| sh`, force pushes, `git reset --hard`, skipped git hooks, cleared shell history, reverse shells and tunnels, decoded base64 run in a shell, agents started with approvals off |
| Privilege and persistence | sudoers changes, setuid bits, containers with host access, shell startup files, cron and launch agents, git hooks, `authorized_keys` |
| Recon and impact | cloud metadata services, privilege discovery, network scans, disk wipes, fork bombs, killing every process, miners |
| Source control and settings | git remote rewrites, git options that run programs, changes to agent settings or agentcam's own files |
| Data out | uploads, credential files or environment variables sent over the network, command output in DNS lookups, domains contacted, writes outside the project |
| Outside content | instructions or hidden characters in tool output, commands built from a tool result or web page |
| Chains | a secret read before an upload, a command right after outside content held instructions, data sent out while approvals were off, another route tried after a call was stopped |

## Your own rules

Add rules on the Audit page, or in `rules.yaml` next to the index (`~/Library/Application Support/agentcam/rules.yaml` on macOS). A rule with the same `id` as a built-in one replaces it. Saving re-checks every session.

### Pattern rules

```yaml
- id: deploy
  title: Deployed to production
  severity: high          # low, medium, high, critical
  categories: [command]   # command, read, edit, write, search, web, mcp, subagent, other
  tags: [deploy]          # for sequences, below
  pattern: '\b(fly deploy|vercel --prod|kubectl apply)\b'
  unless: '--dry-run'     # a match is dropped when this matches too
```

| Field | Meaning |
|---|---|
| `field` | What the pattern reads: `title` (the command, path or URL; the default), `body` (prompt, reply or tool output), `input` (tool input as JSON), `path` (each file the step touched), `any` |
| `kinds` | Which steps: `prompt`, `response`, `thinking`, `tool_call` (default), `system` |
| `actions` | For `path`: `read`, `edit`, `create`, `delete` |
| `secret: true` | Mask the match before it's stored |
| `when_tagged` | Only check steps with one of these context tags (see below) |

If the pattern has a capture group, the first group becomes the excerpt. Commands are matched without heredoc bodies that are data or another language's code (`python3 - <<'EOF'`), so a script that mentions `/etc/sudoers` isn't read as a command.

### Sequence rules

A sequence matches steps in order within a window. Each link matches a step by tag or by rule id. The finding sits on the last step and links back to the first, on the replay ("follows step 12") and in exports.

```yaml
- id: deploy-after-secret
  title: Deployed after reading a secret
  severity: high
  sequence:
    - tags: [secret-read]
    - rules: [deploy]
  within: 20              # steps; default 30
```

Tags that built-in rules leave: `secret-read`, `egress`, `bypass`, `injection`. Every step also gets context tags:

| Tag | When |
|---|---|
| `outside-content` | Its output came from outside the machine or the person: an MCP tool, a web fetch or search, `gh`/`glab` reading issues or PRs, a `curl`/`wget` download. Also `outside-mcp`, `outside-web`, `outside-tracker`. |
| `guardrails-off` | Its turn ran with approvals skipped (`bypassPermissions`, or Codex `never` with full access) |
| `denied` | The call was rejected by you or blocked |
| `command`, `edit`, `web`, `mcp` | Its tool |

### Outside-content rules

```yaml
- id: command-from-tool-output
  title: Ran code or sent data based on a tool result
  severity: high
  outside_content: [mcp, tracker]   # or web
  within: 15
```

This flags a command that runs a package, runs a download, or sends data, when that package, URL or host appeared in that kind of tool output in the last `within` steps and never in one of your prompts. It's the pattern behind attacks such as fake Sentry errors that tell an agent to run a package. Plain downloads don't count, nor does fetching more from the site the agent was already reading.

## Checking a new rule

Before you rely on a rule, run it over your own history: save it, let agentcam re-check, and look at what it found on the Audit page. A rule that fires on ordinary work teaches people to ignore findings.
