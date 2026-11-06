#!/usr/bin/env python3
"""Write made-up Claude Code and Codex session logs for demos and screenshots.

    python3 scripts/demo-data.py /tmp/agentcam-demo
    HOME=/tmp/agentcam-demo agentcam --db /tmp/agentcam-demo/index.db serve

Everything is fake: projects, prompts, file contents, and the one "secret"
is AWS's documented example key. Projects are written under <dir>/code so
What lasted and Instructions have real files to read. Dates are relative to
now, so the heatmap looks recent and one session is still running.
"""

import json
import random
import sys
import uuid
from datetime import datetime, timedelta, timezone
from pathlib import Path

random.seed(7)
HOME = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/agentcam-demo")
CODE = HOME / "code"
NOW = datetime.now(timezone.utc).replace(microsecond=0)
# The team moved to a newer model and Claude Code version about a month ago,
# and payments-api changed its AGENTS.md: things Compare can find.
SWITCH = NOW - timedelta(days=33)
AGENTS_CHANGE = NOW - timedelta(days=21)

PROJECTS = {
    "acme-web": ["src/app/page.tsx", "src/components/Checkout.tsx", "src/lib/cart.ts", "src/lib/api.ts", "tests/cart.test.ts", "package.json"],
    "payments-api": ["src/routes/charge.rs", "src/stripe.rs", "src/db/migrations/0007_refunds.sql", "src/main.rs", "Cargo.toml", "tests/charge.rs"],
    "mobile-app": ["App/Views/HomeView.swift", "App/Models/Workout.swift", "App/Services/Sync.swift", "AppTests/SyncTests.swift"],
    "infra": ["terraform/main.tf", "terraform/variables.tf", ".github/workflows/deploy.yml", "scripts/deploy.sh"],
    "docs-site": ["content/guide/intro.md", "content/guide/install.md", "astro.config.mjs"],
}
TASKS = {
    "acme-web": ["Fix the cart total rounding bug", "Add Apple Pay to checkout", "Make the product grid load faster", "Write tests for the cart reducer", "Migrate checkout to server actions"],
    "payments-api": ["Add partial refunds", "Retry failed webhooks with backoff", "Fix duplicate charge on double click", "Upgrade the Stripe client", "Add idempotency keys to /charge"],
    "mobile-app": ["Offline sync for workouts", "Fix the crash on the history screen", "Add a weekly distance widget", "Speed up app launch"],
    "infra": ["Move staging to the new VPC", "Add a deploy preview workflow", "Rotate the database credentials", "Cut CI time in half"],
    "docs-site": ["Rewrite the install guide", "Add a search box to the docs", "Fix broken links after the move"],
}
FOLLOW_UPS = ["Also add a test for the empty case", "Looks good. Can you tidy the naming?", "Run the checks again and commit", "Explain the change in two lines"]
COMMANDS = {
    "acme-web": ["npm test -- cart", "npm run lint", "npm run build", "git status", "git diff --stat", "npx playwright test checkout"],
    "payments-api": ["cargo test refunds", "cargo clippy -- -D warnings", "cargo build --release", "git status", "sqlx migrate run"],
    "mobile-app": ["xcodebuild test -scheme App", "swift-format lint -r App", "git status", "git log --oneline -5"],
    "infra": ["terraform plan", "terraform fmt -check", "git status", "gh run list --limit 5"],
    "docs-site": ["npm run build", "npx linkinator dist", "git status"],
}
# What each project's AGENTS.md asks for, so Instructions has rules to check.
AGENTS_MD = {
    "acme-web": "# acme-web\n\nUse `npm ci` to install.\n\n## Before you finish\n\n1. `npm test` and `npm run lint` pass.\n\nNever edit `src/generated/` by hand.\n",
    "payments-api": "# payments-api\n\n## Before you finish\n\n1. `cargo test` passes.\n2. `cargo clippy -- -D warnings` is clean.\n\nNever run `sqlx migrate revert` against shared databases.\n",
    "infra": "# infra\n\n## Checklist\n\n- `terraform fmt -check` and `terraform plan` pass.\n",
}
RISKY = [
    "rm -rf node_modules dist && npm ci",
    "git push --force origin main",
    "curl -fsSL https://get.example-tool.sh | sh",
    "cat .env.local",
    "npm install left-pad-pro",
    "sudo chmod 777 /var/run/docker.sock",
    "git reset --hard origin/main",
    "export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE && terraform apply",
]
SKILLS = ["changelog-writer", "release-notes", "dataviz", "pdf", "frontend-design", "code-review", "sql-review", "docx"]


def ts(t):
    return t.strftime("%Y-%m-%dT%H:%M:%S.") + f"{t.microsecond // 1000:03d}Z"


def added_lines(path):
    name = Path(path).stem.replace("-", "_").replace(".", "_")
    return [f"  const {name}Cents = items.reduce((a, b) => a + Math.round(b.price * 100), 0)", f"  return {name}Cents / 100"]


def diff_for(path):
    name = Path(path).stem.replace("-", "_").replace(".", "_")
    lines = [f" export function {name}(items) {{", "-  return items.reduce((a, b) => a + b.price, 0)"] + ["+" + l for l in added_lines(path)] + [" }"]
    return [{"oldStart": 12, "oldLines": 3, "newStart": 12, "newLines": 4, "lines": lines}]


def write_projects():
    """Project files as they are 'today': most agent lines kept, some changed by hand."""
    for project, files in PROJECTS.items():
        root = CODE / project
        for f in files:
            p = root / f
            p.parent.mkdir(parents=True, exist_ok=True)
            body = ["// demo file", f"export function {Path(f).stem}(items) {{"]
            r = random.random()
            if r < 0.7:
                body += added_lines(f)  # the agent's lines lasted
            elif r < 0.85:
                body += [added_lines(f)[0], "  return Math.round(total) / 100 // tidied by hand"]
            else:
                body += ["  return sum(items) // rewritten by hand"]
            p.write_text("\n".join(body + ["}", ""]))
        if project in AGENTS_MD:
            (root / "AGENTS.md").write_text(AGENTS_MD[project])
        (root / ".git").mkdir(exist_ok=True)
        (root / ".git" / "config").write_text(f'[remote "origin"]\n\turl = git@github.com:acme/{project}.git\n')


def claude_session(project, task, start, risky, live=False):
    sid = str(uuid.UUID(int=random.getrandbits(128)))
    cwd = str(CODE / project)
    folder = HOME / ".claude" / "projects" / ("-" + cwd.strip("/").replace("/", "-"))
    folder.mkdir(parents=True, exist_ok=True)
    t = start
    out = []
    newer = start > SWITCH
    model = "claude-opus-5-5" if newer else "claude-sonnet-5-5"
    base = {"sessionId": sid, "cwd": cwd, "gitBranch": random.choice(["main", f"feat/{project}-{random.randint(10,99)}"]),
            "version": "2.5.2" if newer else "2.4.0", "userType": "external", "isSidechain": False, "entrypoint": random.choice(["cli", "cli", "claude-vscode", "claude-desktop"])}
    mode = random.choice(["auto", "auto", "acceptEdits", "default"])
    effort = random.choice(["high", "high", "medium", "xhigh"])

    def rec(kind, **kw):
        d = dict(base, type=kind, timestamp=ts(t), uuid=str(uuid.UUID(int=random.getrandbits(128))))
        d.update(kw)
        out.append(d)

    def step(sec):
        nonlocal t
        t += timedelta(seconds=sec)

    n = 0
    skill_now = [None]

    def assistant(blocks, out_tokens, **kw):
        nonlocal n
        n += 1
        extra = {"effort": effort}
        if skill_now[0]:
            extra["attributionSkill"] = skill_now[0]
        extra.update(kw)
        rec("assistant", message={"id": f"msg_{sid[:8]}_{n}", "model": model, "role": "assistant", "content": blocks,
            "usage": {"input_tokens": random.randint(5, 40), "output_tokens": out_tokens, "cache_read_input_tokens": random.randint(20000, 90000), "cache_creation_input_tokens": random.randint(0, 4000)}}, **extra)

    def approval():
        r = random.random()
        if r < 0.6:
            return {"permissionDecision": {"decision": "accept", "source": "config", "reasonType": "classifier"}}
        if r < 0.85:
            return {"permissionDecision": {"decision": "accept", "source": "config", "reasonType": "rule", "ruleSource": "userSettings"}}
        if r < 0.95:
            return {"permissionDecision": {"decision": "accept", "source": "config", "reasonType": "mode"}}
        return {"permissionDecision": {"decision": "accept", "source": "user_temporary"}}

    def tool(name, inp, result, error=False, structured=None, took=None, rejected=False):
        tid = f"toolu_{uuid.UUID(int=random.getrandbits(128)).hex[:20]}"
        assistant([{"type": "tool_use", "id": tid, "name": name, "input": inp, "caller": {"type": "direct"}}], random.randint(40, 400))
        step(took or random.uniform(0.2, 3))
        kw = {"toolUseResult": structured} if structured else {}
        if rejected:
            kw["toolDenialKind"] = "user-rejected"
        elif name == "Bash":
            kw.update(approval())
        else:
            kw["permissionDecision"] = {"decision": "accept", "source": "config", "reasonType": "rule", "ruleSource": "userSettings"}
        rec("user", message={"role": "user", "content": [{"type": "tool_result", "tool_use_id": tid,
            "content": "The user rejected this tool call." if rejected else result, "is_error": error or rejected}]}, **kw)
        step(random.uniform(1, 8))

    def prompt(text):
        rec("user", permissionMode=mode, turnOrigin="human", message={"role": "user", "content": text})

    out.append({"type": "custom-title", "customTitle": task, "sessionId": sid})
    rec("attachment", attachment={"type": "skill_listing", "names": SKILLS, "skillCount": len(SKILLS),
        "content": "\n".join(f"- {s}: does {s.replace('-', ' ')} work" * 6 for s in SKILLS)})
    files = PROJECTS[project]
    turns = 1 if live else random.choice([1, 2, 2, 3])
    for turn in range(turns):
        prompt(f"{task}. Keep the change small and run the tests when you're done." if turn == 0 else random.choice(FOLLOW_UPS))
        step(3)
        thinking = random.randint(800, 14000)
        assistant([{"type": "thinking", "thinking": f"Start by finding where {task.lower()} lives in {project}, then check the tests that cover it.", "signature": "x"}], 120, thinkingDurationMs=thinking)
        step(thinking / 1000)
        assistant([{"type": "text", "text": f"I'll look at how {project} handles this first."}], 30)
        if turn == 0:
            tool("Grep", {"pattern": task.split()[-1].lower()}, "\n".join(f"{f}:{random.randint(3, 200)}" for f in files[:3]))
            for f in random.sample(files, k=min(3, len(files))):
                tool("Read", {"file_path": f"{cwd}/{f}"}, f"// {f}\n" + "\n".join(f"line {i}" for i in range(12)))
        if random.random() < 0.25:
            sk = random.choice(SKILLS[:6])
            tool("Skill", {"skill": sk, "args": task.lower()}, f"Launching skill: {sk}")
            rec("attachment", attachment={"type": "invoked_skills", "skills": [{"name": sk, "path": f"skills/{sk}", "content": "x" * random.randint(3000, 18000)}]})
            skill_now[0] = sk
        for _ in range(random.randint(1, 4)):
            f = random.choice(files)
            patch = diff_for(f)
            if turn > 0 and random.random() < 0.6:
                # A follow-up rewrites a line the first turn added.
                old = added_lines(f)[1]
                patch = [{"oldStart": 13, "oldLines": 1, "newStart": 13, "newLines": 1, "lines": ["-" + old, "+" + old.replace("/ 100", "/ 100.0")]}]
            tool("Edit", {"file_path": f"{cwd}/{f}", "old_string": "a", "new_string": "b"}, "The file has been updated.",
                 structured={"filePath": f"{cwd}/{f}", "oldString": "a", "newString": "b", "structuredPatch": patch})
            if random.random() < 0.5:
                c = random.choice(COMMANDS[project])
                fail = random.random() < 0.2
                tool("Bash", {"command": c, "description": "check"}, "1 failing\n  expected 19.99, got 19.989999" if fail else "ok: all checks passed", error=fail, took=random.uniform(2, 40))
        skill_now[0] = None
        if risky and turn == 0:
            tool("Bash", {"command": risky}, "done", took=random.uniform(1, 6), rejected=random.random() < 0.4)
        if project == "payments-api" and abs((start - AGENTS_CHANGE).days) < 1 and turn == 0:
            tool("Edit", {"file_path": f"{cwd}/AGENTS.md", "old_string": "a", "new_string": "b"}, "The file has been updated.",
                 structured={"filePath": f"{cwd}/AGENTS.md", "structuredPatch": [{"oldStart": 1, "oldLines": 1, "newStart": 1, "newLines": 2, "lines": ["+2. `cargo clippy -- -D warnings` is clean."]}]})
        if live:
            break
        if random.random() < 0.2:
            tool("WebFetch", {"url": random.choice(["https://docs.stripe.com/api/refunds", "https://developer.apple.com/documentation/swiftdata", "https://react.dev/reference/react/useOptimistic"]), "prompt": "summarize"}, "Summary of the page.")
        # Newer sessions run the project's checks after editing more often.
        if random.random() < (0.85 if newer else 0.5):
            for c in COMMANDS[project][:2]:
                tool("Bash", {"command": c}, "ok: all checks passed", took=random.uniform(3, 30))
        assistant([{"type": "text", "text": f"## Done\n\n{task} is in. I changed **{random.randint(1, 5)} files** and the tests pass.\n\n- Kept the public API the same\n- Added a test for the edge case\n\nRun `git diff` to review."}], 260)
        step(random.uniform(40, 600))
    if not live and random.random() < 0.15:
        num = random.randint(100, 400)
        out.append({"type": "pr-link", "prNumber": num, "prUrl": f"https://github.com/acme/{project}/pull/{num}", "prRepository": f"acme/{project}", "timestamp": ts(t), "sessionId": sid})
    if not live and random.random() < 0.04:
        rec("assistant", isApiErrorMessage=True, error="rate_limit", message={"id": f"msg_{sid[:8]}_limit", "model": "<synthetic>", "role": "assistant",
            "content": [{"type": "text", "text": "You've hit your session limit · resets 3pm (America/Los_Angeles)"}]})
    with open(folder / f"{sid}.jsonl", "w") as fh:
        for d in out:
            fh.write(json.dumps(d) + "\n")


def codex_session(project, task, start, risky, used):
    tid = str(uuid.UUID(int=random.getrandbits(128)))
    cwd = str(CODE / project)
    t = start
    day = HOME / ".codex" / "sessions" / start.strftime("%Y/%m/%d")
    day.mkdir(parents=True, exist_ok=True)
    out = []

    def ev(kind, payload):
        out.append({"timestamp": ts(t), "type": kind, "payload": payload})

    def item(it):
        ev("event_msg", {"type": "item_completed", "thread_id": tid, "turn_id": "t1", "item": it})

    def step(sec):
        nonlocal t
        t += timedelta(seconds=sec)

    version = "0.171.0" if start > SWITCH else "0.168.2"
    ev("session_meta", {"id": tid, "timestamp": ts(t), "cwd": cwd, "cli_version": version, "originator": "codex_cli", "git": {"branch": "main"}})
    ev("event_msg", {"type": "thread_settings_applied", "thread_settings": {"model": "gpt-5.5-codex", "service_tier": "priority"}})
    ev("turn_context", {"turn_id": "t1", "cwd": cwd, "model": "gpt-5.5-codex", "effort": random.choice(["medium", "high"]),
        "approval_policy": "on-request", "sandbox_policy": {"type": "workspace-write"}, "collaboration_mode": {"mode": "default"}})
    item({"type": "UserMessage", "id": "u1", "content": [{"type": "text", "text": task}]})
    step(4)
    item({"type": "Reasoning", "id": "r1", "summary_text": [f"**Planning:** {task.lower()}"]})
    files = PROJECTS[project]
    if random.random() < 0.2:
        step(2)
        item({"type": "CommandExecution", "id": "skill", "command": ["/bin/zsh", "-lc", f"cat {HOME}/.codex/skills/sql-review/SKILL.md"], "cwd": f"file://{cwd}",
              "status": "completed", "aggregated_output": "# SQL review", "exit_code": 0, "duration": {"secs": 0, "nanos": 0}})
    for i in range(random.randint(3, 8)):
        step(random.uniform(2, 20))
        c = random.choice(COMMANDS[project] + [f"cat {random.choice(files)}", f"rg -n TODO {random.choice(files)}"])
        fail = random.random() < 0.15
        item({"type": "CommandExecution", "id": f"e{i}", "command": ["/bin/zsh", "-lc", c], "cwd": f"file://{cwd}", "status": "failed" if fail else "completed",
              "aggregated_output": "error: test failed" if fail else "ok", "exit_code": 1 if fail else 0, "duration": {"secs": random.randint(0, 20), "nanos": 0}})
        if random.random() < 0.5:
            f = random.choice(files)
            step(3)
            item({"type": "FileChange", "id": f"f{i}", "changes": {f"{cwd}/{f}": {"type": "update", "unified_diff": "@@ -12,2 +12,3 @@\n-  return items.reduce((a, b) => a + b.price, 0)\n" + "".join("+" + l + "\n" for l in added_lines(f)) + " }\n"}}, "status": "completed", "stdout": "Success."})
    if risky:
        step(5)
        item({"type": "CommandExecution", "id": "risky", "command": ["/bin/zsh", "-lc", risky], "cwd": f"file://{cwd}", "status": "completed", "aggregated_output": "", "exit_code": 0, "duration": {"secs": 2, "nanos": 0}})
    step(6)
    item({"type": "AgentMessage", "id": "a1", "content": [{"type": "Text", "text": f"{task}: done. Tests pass."}], "phase": "final"})
    reset = int((start + timedelta(hours=5)).timestamp())
    week_reset = int((NOW + timedelta(days=3)).timestamp())
    ev("event_msg", {"type": "token_count",
        "info": {"total_token_usage": {"input_tokens": random.randint(200000, 900000), "cached_input_tokens": 150000, "output_tokens": random.randint(4000, 30000), "reasoning_output_tokens": random.randint(1000, 9000)}},
        "rate_limits": {"limit_id": "codex", "plan_type": "plus", "rate_limit_reached_type": None,
            "primary": {"used_percent": float(used), "window_minutes": 300, "resets_at": reset},
            "secondary": {"used_percent": float(min(99, used // 3 + 20)), "window_minutes": 10080, "resets_at": week_reset}}})
    ev("event_msg", {"type": "task_complete", "turn_id": "t1"})
    with open(day / f"rollout-{start.strftime('%Y-%m-%dT%H-%M-%S')}-{tid}.jsonl", "w") as fh:
        for d in out:
            fh.write(json.dumps(d) + "\n")


def write_skills():
    for root, name, desc in [
        (HOME / ".claude" / "skills", "changelog-writer", "Write a changelog entry from the staged diff."),
        (HOME / ".claude" / "skills", "release-notes", "Draft release notes from merged pull requests."),
        (HOME / ".codex" / "skills", "sql-review", "Review SQL migrations for locking and data loss."),
    ]:
        d = root / name
        d.mkdir(parents=True, exist_ok=True)
        (d / "SKILL.md").write_text(f"---\nname: {name}\ndescription: {desc}\n---\n\n# {name}\n\n" + "Steps to follow.\n" * 40)


def main():
    write_projects()
    write_skills()
    count = 0
    for days_ago in range(75, -1, -1):
        day = NOW - timedelta(days=days_ago)
        weekday = day.weekday() < 5
        n = random.choice([0, 1, 2, 3, 3, 4]) if weekday else random.choice([0, 0, 0, 1])
        for _ in range(n):
            project = random.choices(list(PROJECTS), weights=[5, 4, 3, 2, 1])[0]
            task = random.choice(TASKS[project])
            start = day.replace(hour=random.randint(8, 20), minute=random.randint(0, 59), second=0) - timedelta(hours=8)
            if start > NOW - timedelta(hours=1):
                start = NOW - timedelta(hours=random.randint(2, 6))
            risky = random.choice(RISKY) if random.random() < 0.12 else None
            if random.random() < 0.3:
                codex_session(project, task, start, risky, used=min(96, 30 + (75 - days_ago) % 9 * 8))
            else:
                claude_session(project, task, start, risky)
            count += 1
    # The payments-api AGENTS.md change, and one session still running.
    claude_session("payments-api", "Tighten the refund checks", AGENTS_CHANGE, None)
    codex_session("infra", "Cut CI time in half", NOW - timedelta(minutes=50), None, used=78)
    claude_session("acme-web", "Add Apple Pay to checkout", NOW - timedelta(minutes=4), None, live=True)
    count += 3
    # A guard rule and hook configs, so the Audit page shows the guard
    # working (the configs only need to mention `agentcam guard`).
    hook = {"hooks": {"PreToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": "agentcam guard"}]}]}}
    (HOME / ".claude" / "settings.json").write_text(json.dumps(hook, indent=2))
    (HOME / ".codex" / "hooks.json").write_text(json.dumps(hook, indent=2))
    (HOME / "guard.yaml").write_text(
        "- id: no-force-push\n"
        "  tool: Bash\n"
        "  pattern: '\\bgit\\s+push\\b.*\\s(--force|-f)\\b'\n"
        "  reason: Force pushes are blocked. Ask the user first.\n"
        f"  created: '{ts(NOW - timedelta(days=3))}'\n"
    )
    print(f"wrote {count} sessions to {HOME} (index with --db {HOME}/index.db)")


if __name__ == "__main__":
    main()
