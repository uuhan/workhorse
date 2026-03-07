---
name: workhorse-remote-build
description: Use when the user wants to run Rust build workflows on a `horsed` server via `cargo work`, including remote `build`, `test`, `check`, `clippy`, `run`, `install`, `doc`, `metadata`, `rustc`, `zigbuild`, and `just` commands, with target selection through `--repo`, `--repo-name`, or `--remote`.
---

# Workhorse Remote Build

## Overview

This skill handles remote Rust workflows through `cargo work`. Use it for compile, test, lint, run, or `just` tasks that should execute on a `horsed` host instead of the local machine.

## Preconditions

- The local repo should resolve a target in this order: `--repo-name`, `--repo`, then Git remote `horsed`.
- Host resolution follows: `HORSED` env, `--repo` host, then Git remote `horsed`.
- Unless `--ssh-key` is passed, `cargo-work` looks for `~/.ssh/id_rsa` or `~/.ssh/id_ed25519`.
- Prefer `--repo ssh://git@HOST:2222/ns/repo.git` for one-off use. Prefer a stable `horsed` Git remote for repeated use.

## Core Commands

Use these as the default patterns:

```bash
cargo work build --release
cargo work test
cargo work check
cargo work clippy -- -D warnings
cargo work run -- --help
cargo work just build
```

Most build-like actions print a server-side `job_id=...` line. Use that id with `cargo work job attach <job_id> -f` from another terminal when you need to follow the same task output concurrently.

For an explicit host instead of a Git remote:

```bash
cargo work build --repo ssh://git@127.0.0.1:2222/uuhan/workhorse.git --release
```

Useful shared flags:

- `--remote horsed-linux`: choose a non-default Git remote.
- `--env KEY=VALUE`: inject remote environment variables.
- `--shell nu`: switch remote interpreter for script-style commands.
- `-x` or `--all-proxy=socks5://127.0.0.1:1080`: expose local proxy to the server.
- `-t`: allocate a PTY when the remote command needs one.

## Workflow

1. Resolve the target repo and host first. If neither `--repo` nor a `horsed` remote is present, stop and ask for the server URL.
2. Pick the narrowest remote command that matches the task. Use `build`, `test`, `check`, `clippy`, `run`, or `just` instead of falling back to raw shell commands.
3. Pass through Cargo arguments after `--` when the underlying subcommand needs them.
4. If the task also needs output retrieval, switch to `$workhorse-artifact-sync` after the remote job finishes.
5. If the user needs live output in multiple terminals, route to `$workhorse-artifact-sync` and use `cargo work job attach`.

## Examples

```bash
# Run tests on the default horsed remote
cargo work test

# Build with a named remote
cargo work build --remote horsed-linux --release

# Run clippy with extra args
cargo work clippy --all-targets -- -D warnings

# Run a just recipe on an explicit repo
cargo work just --repo ssh://git@10.0.0.8:2222/team/app.git deploy

# Start a build, then attach by job id in another terminal
cargo work check
cargo work job attach <job_id> -f
```

## Caveats

- `cargo work watch` exists in the CLI but the current implementation does not execute the requested command on file changes. Treat it as experimental and do not recommend it as the default path.
- When repo detection fails, the fix is usually to add `git remote add horsed ssh://git@HOST:2222/ns/repo.git` or pass `--repo`.
- Current patch sync includes both local worktree changes (`git diff HEAD`) and ahead-of-upstream commits (`<upstream>..HEAD`) when upstream exists.
- For `just install-horsed`, keep the `horsed` remote branch up to date (`cargo work push` or `git push horsed <branch>`) to avoid protocol/version drift between client and server baseline.
