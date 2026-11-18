---
title: Try it with demo data
description: Look around agentcam with made-up sessions, without touching your real logs.
---

To look around without your own logs, make a folder of made-up sessions and point agentcam at it. Your real logs and index aren't touched.

You need Python 3 and nothing else.

```sh
git clone https://github.com/chauhanswapnil/agentcam && cd agentcam
python3 scripts/demo-data.py /tmp/agentcam-demo
HOME=/tmp/agentcam-demo agentcam --db /tmp/agentcam-demo/index.db --port 4848
```

The script writes fake sessions for both agents, plus fake project files, AGENTS.md files and skill folders, so What lasted, Instructions and Skills have something to show. One session stays "running", so Live has something too.

`--port 4848` keeps it apart from a real agentcam on 4747.

From a checkout without agentcam installed, use `cargo run --` in place of `agentcam`.

Every screenshot on this site comes from this demo data.
