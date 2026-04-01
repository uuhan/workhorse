---
name: workhorse-artifact-sync
description: Use when the user wants to fetch files or build outputs from a `horsed` server with `cargo work get` or `scp`, sync Git state with `cargo work push` or `pull`, inspect server status with `cargo work ping`, `health`, and `logs`, or inspect and attach running job output with `cargo work job`.
---

# Workhorse Artifact Sync

## Overview

This skill handles file retrieval, Git sync, job inspection/attach, and server inspection for `cargo-work`. Use it after a remote build, or whenever the task is about connectivity, logs, running-job output, or moving artifacts back to the local machine.

## Preconditions

- Repo and host resolution follow the same `cargo-work` rules: `--repo-name`, `--repo`, or Git remote `horsed`; host from `HORSED`, `--repo`, or Git remote `horsed`.
- For `push`, the local Git remote defaults to `horsed`.
- For `get` and `scp`, prefer forward-slash remote paths even when the server is Windows-backed.
- `cargo work ping` runs continuously when `--count` is omitted. For a bounded check, use `cargo work ping --count <n>` or plain `cargo work` (which defaults to `ping --count 3`).

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
cargo work job list
cargo work job attach <job_id> -f
```

## Workflow

1. If the task is "build then download", run the remote build first with `$workhorse-remote-build`.
2. For a quick connectivity check, use plain `cargo work` or `cargo work ping --count <n>`. Do not run bare `cargo work ping` unless the user explicitly asks for continuous probing.
3. Use `health` when the user needs server readiness details such as `version`, `commit`, `os/arch/family`, default shell, and `ulimit -n`.
4. Use `job list` and `job attach` when the user needs stdout/stderr from a running or finished remote command.
5. Use `logs` for service-side debugging.
6. Use `get` when the file lives under the remote worktree and should be materialized locally with path awareness.
7. Use `scp` when a raw file stream is sufficient.
8. Use `push` and `pull` only when the user intends to sync Git state, not artifacts.

## Behavior Details

- `cargo work get` writes directories as `.tar` archives.
- `cargo work get` will not overwrite an existing local file unless `-f`, `--outfile`, or `--stdout` is used.
- `cargo work scp` creates the destination file with `create_new`, so an existing destination path will fail.
- `cargo work ping` without `--count` is an endless loop by design; always set `--count` in automated or agent-driven checks.
- `cargo work push` and `cargo work pull` are thin wrappers around local `git push` and `git pull`.
- `cargo work job list` returns JSON summaries, including `id`, `action`, `running`, and `exit_code`. Cargo tasks are classified as `cargo.<subcommand>` (for example `cargo.check`, `cargo.test`).
- `cargo work job attach <job_id>` replays buffered output and follows by default; use `--no-follow` for snapshot-only output.
- If `health` appears silent, check log level first. Use `RUST_LOG=info cargo work health` for visible output; add `WH_DEBUG=1` when you need trace-stage lines.

## Examples

```bash
# Quick server check
cargo work

# Download a built binary
cargo work get target/release/cargo-work -f

# Follow server logs while debugging a failed run
cargo work logs -f

# Attach to a running build from another terminal
cargo work job list
cargo work job attach job-xxxx -f

# Push the current branch to the default remote
cargo work push
```
