# agentcam as an MCP server

`agentcam mcp` lets your agents look up past work on this machine: how a bug was fixed before, what a repo's AGENTS.md or CLAUDE.md asks and what tends to get missed, which edits to a file lasted, and which session wrote a line. It reads the same local index as the UI and never writes to it.

## Add it

Claude Code:

```sh
claude mcp add agentcam -- agentcam mcp
```

Codex, in `~/.codex/config.toml`:

```toml
[mcp_servers.agentcam]
command = "agentcam"
args = ["mcp"]
```

To use another index, add `--db <file>` before `mcp` (`agentcam --db /path/index.db mcp`) or set `AGENTCAM_DB`.

## How it runs

- It speaks JSON-RPC over stdio, one message per line. Only protocol messages go to stdout.
- It indexes new and changed logs when it starts, then again before a tool call if 30 seconds have passed, so sessions that end while it runs show up.
- Every answer has secrets masked by the same rules as session exports (`secret: true` rules in the audit rules).
- Answers are cut at about 60,000 characters; ask for less (a smaller `limit`, a line range) when you see "(cut; ask for less)".

## Tools

| Tool | Arguments | What it returns |
|---|---|---|
| `search_sessions` | `query`, `project`, `limit` (10, at most 50) | Best matching steps across prompts, replies, commands, output and file paths, with session id and step number |
| `recent_sessions` | `project`, `limit` | Latest sessions: title, agent, model, time, cost, counts |
| `get_session` | `session_id` | Turns, files changed, commits, pull requests, findings, lines still in the code, instruction rules not followed, subagents |
| `get_step` | `session_id`, `seq` | One step in full: input, output, file diffs |
| `file_history` | `path` | Every step that read or changed a file, newest first |
| `blame` | `path`, `start_line`, `end_line` | Each line of the file today and the session, step, model and prompt that wrote it. Only files an agent edited are read. |
| `project_rules` | `project` | The checkable rules in a repo's AGENTS.md / CLAUDE.md and how often past sessions followed each |
| `what_lasted` | `project`, `days` (90) | Lines agents added that are still there, reworked or changed since, by age, model and file |

`project` is the repo folder as an absolute path. Agent worktrees count toward their repo.
