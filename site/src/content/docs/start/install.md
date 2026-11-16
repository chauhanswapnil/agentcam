---
title: Install
description: Install agentcam with Homebrew, npm, Cargo or the install script.
---

Pick one.

## Homebrew (macOS and Linux)

```sh
brew install chauhanswapnil/tap/agentcam
```

## npm (anywhere with Node 18+)

```sh
npx agentcam
```

## Install script (macOS and Linux)

Puts the binary in `~/.cargo/bin`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/chauhanswapnil/agentcam/releases/latest/download/agentcam-installer.sh | sh
```

## Windows (PowerShell)

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/chauhanswapnil/agentcam/releases/latest/download/agentcam-installer.ps1 | iex"
```

## Cargo

```sh
cargo install agentcam
```

## Binaries

Every platform's binary is on the [releases page](https://github.com/chauhanswapnil/agentcam/releases).

## Next

Run it. See [First run](../first-run/).
