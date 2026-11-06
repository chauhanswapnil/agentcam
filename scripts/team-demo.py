#!/usr/bin/env python3
"""Fill a team server with made-up machines and session metadata.

For screenshots and trying the team pages. Everything here is fake: machine
ids, users, repos and findings. It talks to the server's own API, the same
way laptops do.

    agentcam team serve --data /tmp/agentcam-team-demo    # prints an admin token
    python3 scripts/team-demo.py http://127.0.0.1:4850 ac_view_...
"""

import json
import random
import sys
import time
import urllib.request
from datetime import datetime, timedelta, timezone

random.seed(7)
SERVER, ADMIN = sys.argv[1].rstrip("/"), sys.argv[2]
NOW = datetime.now(timezone.utc)


def call(path, body, token=None):
    req = urllib.request.Request(
        SERVER + path,
        data=json.dumps(body).encode(),
        headers={"content-type": "application/json", **({"authorization": f"Bearer {token}"} if token else {})},
    )
    with urllib.request.urlopen(req) as r:
        return json.loads(r.read())


def ts(d):
    return d.strftime("%Y-%m-%dT%H:%M:%S.000Z")


TEAMS = {
    "Platform": ["github.com/acme/api", "github.com/acme/infra", "github.com/acme/auth"],
    "Payments": ["github.com/acme/payments-api", "github.com/acme/ledger"],
    "Mobile": ["github.com/acme/ios", "github.com/acme/android"],
    "Data": ["github.com/acme/pipelines", "github.com/acme/warehouse"],
}
SIZES = {"Platform": 9, "Payments": 6, "Mobile": 5, "Data": 4}
USERS = iter(
    "ava ben chen dev eli fay gus hana ivan jo kai lena max nia omar pia quinn raj sam tia uma vic wes xia yuri zoe".split()
)
SWITCH = NOW - timedelta(days=33)
RULES = [
    ("secret-aws-key", "critical", "AWS_SECRET_ACCESS_KEY=****************"),
    ("git-force-push", "high", "git push --force origin main"),
    ("read-cloud-credentials", "high", "cat ~/.aws/credentials"),
    ("read-ssh", "high", "~/.ssh/id_ed25519"),
    ("curl-pipe-shell", "high", "curl -fsSL https://get.example.dev | sh"),
    ("git-reset-hard", "medium", "git reset --hard HEAD~3"),
    ("read-env-file", "medium", ".env.production"),
    ("write-outside-project", "medium", "~/Desktop/notes.md"),
    ("sudo", "medium", "sudo chown -R $(whoami) /usr/local"),
    ("package-install", "low", "npm install left-pad-ng"),
    ("outbound-domain", "low", "registry.npmjs.org"),
]
MCP = [("github", "http"), ("linear", "http"), ("sentry", "http"), ("postgres", "stdio"), ("figma", "http"), ("playwright", "stdio")]
SKILLS = ["release-notes", "db-migration", "api-review", "incident-summary", "frontend-design", "flaky-test-hunt"]

call("/api/team/settings", {"org_name": "Acme Engineering", "seat_price_usd": 150, "show_people": False}, ADMIN)

total = 0
for team, repos in TEAMS.items():
    code = call("/api/team/invites", {"team": team, "uses": 50}, ADMIN)["code"]
    for n in range(SIZES[team]):
        machine = "%032x" % random.getrandbits(128)
        user = next(USERS)
        dev = call("/api/team/join", {"invite": code, "machine_id": machine, "os_user": user})["device_token"]
        codex_user = random.random() < 0.45
        heavy = random.uniform(0.5, 1.6)
        sessions = []
        day = NOW - timedelta(days=90)
        sid = 0
        while day < NOW - timedelta(hours=2):
            # Adoption grows over the quarter.
            age = (NOW - day).days
            if random.random() < (0.35 + 0.5 * (90 - age) / 90) * heavy and day.weekday() < 5:
                for _ in range(random.randint(1, 3)):
                    sid += 1
                    agent = "codex" if codex_user and random.random() < 0.5 else "claude-code"
                    after = day > SWITCH
                    if agent == "claude-code":
                        model = "claude-opus-5-5" if after else "claude-opus-5"
                        version = "2.4.1" if after else "2.3.8"
                    else:
                        model = "gpt-5.5-codex" if after else "gpt-5.2-codex"
                        version = "0.71.0" if after else "0.64.2"
                    start = day.replace(hour=random.randint(9, 17), minute=random.randint(0, 59))
                    active = int(random.lognormvariate(14.2, 0.7))  # ~25 min
                    if after and agent == "claude-code":
                        active = int(active * 0.82)
                    steps = max(8, int(active / 25_000))
                    errored = int(steps * random.uniform(0.02, 0.09 if not after else 0.06))
                    tok_out = steps * random.randint(300, 900)
                    cost = round(tok_out * 0.00004 + steps * random.uniform(0.004, 0.02), 2)
                    added = random.randint(0, 260)
                    kept_share = random.uniform(0.62, 0.9) if after else random.uniform(0.55, 0.85)
                    kept = int(added * kept_share)
                    reworked = int((added - kept) * random.uniform(0.3, 0.7))
                    finds = []
                    if random.random() < 0.18:
                        r = random.choices(RULES, weights=[1, 2, 1, 1, 1, 2, 3, 4, 2, 6, 8])[0]
                        finds.append({"rule_id": r[0], "severity": r[1], "step": random.randint(1, steps), "excerpt": r[2]})
                    commits = random.choices([0, 1, 2, 3], weights=[4, 5, 2, 1])[0]
                    follow = None
                    if random.random() < 0.7:
                        f_ok = random.randint(1, 4)
                        follow = {"followed": f_ok, "not_followed": 1 if random.random() < (0.12 if after else 0.22) else 0}
                    repo = random.choice(repos)
                    sessions.append(
                        {
                            "session_id": f"{machine[:8]}-{sid}",
                            "parent_id": None,
                            "version": "v1",
                            "machine_id": machine,
                            "os_user": user,
                            "agent": agent,
                            "agent_version": version,
                            "model": model,
                            "project_remote": repo,
                            "project_name": repo.rsplit("/", 1)[1],
                            "branch": random.choice(["main", "feat/checkout", "fix/flaky-tests", "chore/deps"]),
                            "started_at": ts(start),
                            "ended_at": ts(start + timedelta(milliseconds=active * 1.4)),
                            "active_ms": active,
                            "retry_ms": int(active * random.uniform(0, 0.12)),
                            "steps": {"tool_call": steps, "prompt": random.randint(1, 6), "response": steps // 3},
                            "tool_calls": {"command": steps // 2, "edit": steps // 4, "read": steps // 4},
                            "errored_steps": errored,
                            "tokens": {"fresh_input": tok_out // 3, "output": tok_out, "cache_read": tok_out * 30, "cache_write": tok_out * 2},
                            "est_cost_usd": cost,
                            "peak_context": random.randint(40_000, 180_000),
                            "findings": finds,
                            "commits": [{"step": steps - 1, "hash": "%07x" % random.getrandbits(28), "message": "Update"}] * commits,
                            "files": [{"path": "src/app.ts", "action": "edit"}],
                            "turns": random.randint(1, 6),
                            "lasted": {"added": added, "kept": kept, "reworked": reworked, "changed": added - kept - reworked} if added else None,
                            "follow": follow,
                            "prs": [{"number": random.randint(100, 900), "repo": repo.split("/", 1)[1]}] if commits and random.random() < 0.5 else [],
                        }
                    )
            day += timedelta(days=1)
        flags = []
        if random.random() < 0.2:
            flags.append({"id": "approvals_skipped_by_default", "agent": "claude-code", "scope": "project"})
        if codex_user and random.random() < 0.3:
            flags.append({"id": "sandbox_off", "agent": "codex", "scope": "user"})
        if random.random() < 0.15:
            flags.append({"id": "broad_allow_rule", "agent": "claude-code", "scope": "user"})
        if random.random() < 0.4:
            flags.append({"id": "guard_hook_installed", "agent": "claude-code", "scope": "user"})
        items = [
            {"kind": "mcp_server", "agent": "claude-code", "name": m, "detail": t, "scope": "user"}
            for m, t in random.sample(MCP, random.randint(1, 4))
        ]
        items += [{"kind": "skill", "agent": "claude-code", "name": s, "detail": None, "scope": "user"} for s in random.sample(SKILLS, random.randint(0, 3))]
        items += [{"kind": "plugin", "agent": "claude-code", "name": p, "detail": None, "scope": "user"} for p in random.sample(["code-review@anthropic", "frontend@anthropic", "security@acme"], random.randint(0, 2))]
        items.append({"kind": "hook", "agent": "claude-code", "name": "prettier", "detail": "PostToolUse", "scope": "project"})
        agents = [{"agent": "claude-code", "version": random.choice(["2.4.1", "2.4.1", "2.4.0"]), "sessions": 12, "last_used": ts(NOW)}]
        if codex_user:
            agents.append({"agent": "codex", "version": random.choice(["0.71.0", "0.70.2"]), "sessions": 6, "last_used": ts(NOW)})
        inv = {"os": random.choice(["macos", "macos", "linux"]), "arch": "aarch64", "agentcam_version": "0.1.0", "agents": agents, "items": items, "flags": flags}
        for i in range(0, max(1, len(sessions)), 400):
            call(
                "/api/team/sync",
                {"schema_version": 2, "agentcam_version": "0.1.0", "sessions": sessions[i : i + 400], "inventory": inv if i == 0 else None},
                dev,
            )
        total += len(sessions)
print(f"Synced {total} sessions from {sum(SIZES.values())} machines in {len(TEAMS)} teams.")
