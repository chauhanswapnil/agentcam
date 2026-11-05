# agentcam

**A dashcam for your coding agents.**

agentcam reads the session logs that Claude Code and Codex already write on your machine and turns them into something you can look at: an overview of months of agent work, and a step-by-step replay of any session. It runs fully on your machine. No account, no telemetry, nothing leaves localhost.

> **Status:** early. The binary and web UI shell exist; parsers and views are being built now. Not ready for use yet.

## Planned

- **Overview:** activity heatmap, stats, projects and every session in one place.
- **Replay:** scrub a session's timeline and see each prompt, tool call, command and file diff.
- **Search:** full-text across prompts, commands, file paths and tool calls.
- **Risk audit (later):** flag secrets in transcripts, `.env` reads, `rm -rf`, force pushes and more, with rules in plain YAML.

Agents: Claude Code and Codex first. Cursor and others later.

## Install

Not published yet. To run from source:

```sh
git clone https://github.com/chauhanswapnil/agentcam
cd agentcam
npm --prefix web ci && npm --prefix web run build
cargo run --release
```

`agentcam` starts a local server on `http://127.0.0.1:4747` and opens your browser.

## Privacy

agentcam only reads files. It never edits or uploads your agent logs, and the server binds to `127.0.0.1` only. Agent logs can hold secrets (API keys, tokens, file contents), so treat anything agentcam shows you, and any screenshot of it, with the same care as the logs themselves.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). If you use an AI coding agent, point it at [AGENTS.md](AGENTS.md).

## License

[Apache License 2.0](LICENSE).
