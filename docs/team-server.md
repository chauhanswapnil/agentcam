# Running a team server

A team server collects metadata from the laptops that join it and shows it to the people you give tokens to. It is the same `agentcam` binary, run with `agentcam team serve`. Your company runs it; nothing goes to us.

## What laptops send

Counts, times, tokens, cost, findings (with secrets masked on the laptop), commits, pull request links, file paths relative to the project, and agent setup by name (versions, MCP servers, plugins, skills, hooks, a few setting flags). Never prompts, replies, thinking, commands, output, tool inputs or diffs. On a laptop, the Outbox page (`/outbox`) shows exactly what would be sent.

## Start it

```sh
agentcam team serve --data /var/lib/agentcam-team --bind 127.0.0.1:4850
```

- `--data` is the folder for `team.db` (SQLite). It defaults to `agentcam-team` in the user's data folder.
- On first start it prints an admin token once. Keep it; the server stores only its SHA-256 hash.
- `--bind 127.0.0.1:4850` (the default) keeps it local. Put a reverse proxy with TLS in front and let the proxy reach it there. Bind `0.0.0.0:4850` only on a private network, behind TLS.

Laptops refuse `http://` to a server on another machine unless they join with `--allow-http`, since device tokens would travel in clear text.

### Behind a proxy

Any TLS proxy works. With Caddy:

```
agentcam.example.com {
    reverse_proxy 127.0.0.1:4850
}
```

With nginx, raise the body size so a first sync of many sessions fits (a request holds at most 1,000 sessions):

```
location / {
    proxy_pass http://127.0.0.1:4850;
    client_max_body_size 64m;
}
```

As a service, run it under its own user with `systemd` or `launchd`, with `--data` on a disk that is backed up.

## Add people and machines

| Task | Command (on the server machine) | Or in the UI |
|---|---|---|
| Invite laptops to a team | `agentcam team invite Platform --uses 50 --days 7` | Settings → Invite machines |
| Give someone the dashboards | `agentcam team token --name "Finance" --role viewer` | Settings → Viewer tokens |
| List machines | `agentcam team machines` | |
| Remove a machine and everything it sent | `agentcam team remove abc123` | |
| List viewer tokens | `agentcam team viewers` | |
| Revoke a viewer token | `agentcam team revoke 3` | |

On each laptop:

```sh
agentcam team join https://agentcam.example.com ac_inv_...
agentcam team status      # link, last sync, last error
agentcam team pause       # stop sending; resume starts again
agentcam team leave       # forget the link; the server keeps what it has
```

A joined laptop sends changes every 5 minutes while `agentcam serve` runs, or now with `agentcam team sync`.

## Tokens

| Prefix | Who holds it | What it can do |
|---|---|---|
| `ac_inv_` | Anyone you send an invite to | Trade itself for a device token, a set number of times before it expires |
| `ac_dev_` | One laptop (`team.json`, mode 600) | Send that machine's own metadata, nothing else |
| `ac_view_` | A person, in their browser | Read the dashboards; `admin` tokens also change settings and make invites |

The server stores only hashes of tokens. A lost laptop: `agentcam team remove` its machine. A leaked viewer token: `agentcam team revoke` it and make a new one. The last admin token can't be revoked, so make a new admin token first.

## People and privacy

Machines show as `Machine abc123`. An admin can turn on names (the OS user name) in Settings; the change goes to the audit log. No view ranks people. Cost is at API list prices, so compare it with seat prices, not invoices.

## Back up and upgrade

- `team.db` holds everything. Copy it while the server is stopped, or use `sqlite3 team.db ".backup backup.db"` while it runs.
- The schema upgrades itself on start (numbered migrations). Back up before you upgrade.
- Laptops send a share schema version. A server older than its laptops refuses their syncs with a message to update the server, so upgrade the server first.
