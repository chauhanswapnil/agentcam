# Contributing to agentcam

Thanks for helping. agentcam is early, so the best contributions right now are bug reports with clear steps, log-format edge cases (as synthetic fixtures), and focused PRs.

## Setup

You need stable Rust (1.85+) and Node 20+.

```sh
git clone https://github.com/chauhanswapnil/agentcam
cd agentcam
npm --prefix web ci
npm --prefix web run build
cargo test
cargo run
```

See [AGENTS.md](AGENTS.md) for the repo layout, commands and the dev loop.

## Pull requests

1. Open an issue first for anything larger than a small fix, so we can agree on the approach.
2. Keep each PR to one change.
3. Run the checks in the "Before you finish a change" section of [AGENTS.md](AGENTS.md).
4. Add a line to `CHANGELOG.md` under "Unreleased" if users would notice the change.

## Never share real agent logs

Session logs contain secrets and private code. Do not paste them into issues or PRs, and do not add them as fixtures. Write a small synthetic log that shows the same shape instead.

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).

## License

By contributing, you agree that your contributions are licensed under the [Apache License 2.0](LICENSE), as stated in section 5 of the license.
