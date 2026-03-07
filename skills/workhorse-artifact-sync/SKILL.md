---
name: workhorse-artifact-sync
description: Use when the user wants to fetch files or build outputs from a `horsed` server with `cargo work get` or `scp`, sync Git state with `cargo work push` or `pull`, or inspect server status with `cargo work ping`, `health`, and `logs`.
---

# Workhorse Artifact Sync

## Overview

This skill handles file retrieval, Git sync, and server inspection for `cargo-work`. Use it after a remote build, or whenever the task is about connectivity, logs, or moving artifacts back to the local machine.

## Preconditions

- Repo and host resolution follow the same `cargo-work` rules: `--repo-name`, `--repo`, or Git remote `horsed`; host from `HORSED`, `--repo`, or Git remote `horsed`.
- For `push`, the local Git remote defaults to `horsed`.
- For `get` and `scp`, prefer forward-slash remote paths even when the server is Windows-backed.

## Retrieval Commands

Use `get` for outputs inside the remote worktree:

```bash
cargo work get target/release/horsed
cargo work get target -f
cargo work get target/release/horsed --stdout > horsed.bin
```

Use `scp` for remote file copy streams:

```bash
cargo work scp target/release/horsed ./horsed.remote
```

## Git Sync and Inspection

```bash
cargo work push
cargo work pull main
cargo work ping --count 3
cargo work health
cargo work logs
cargo work logs -f
```

## Workflow

1. If the task is "build then download", run the remote build first with `$workhorse-remote-build`.
2. Use `cargo work ping` or plain `cargo work` for a quick connectivity check.
3. Use `health` when the user needs server readiness details such as `version`, `commit`, `os/arch/family`, default shell, and `ulimit -n`.
4. Use `logs` for service-side debugging.
5. Use `get` when the file lives under the remote worktree and should be materialized locally with path awareness.
6. Use `scp` when a raw file stream is sufficient.
7. Use `push` and `pull` only when the user intends to sync Git state, not artifacts.

## Behavior Details

- `cargo work get` writes directories as `.tar` archives.
- `cargo work get` will not overwrite an existing local file unless `-f`, `--outfile`, or `--stdout` is used.
- `cargo work scp` creates the destination file with `create_new`, so an existing destination path will fail.
- `cargo work push` and `cargo work pull` are thin wrappers around local `git push` and `git pull`.
- If `health` appears silent, check log level first. Use `RUST_LOG=info cargo work health` for visible output; add `WH_DEBUG=1` when you need trace-stage lines.

## Examples

```bash
# Quick server check
cargo work

# Download a built binary
cargo work get target/release/cargo-work -f

# Follow server logs while debugging a failed run
cargo work logs -f

# Push the current branch to the default remote
cargo work push
```
