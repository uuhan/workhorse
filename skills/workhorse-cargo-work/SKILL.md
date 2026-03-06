---
name: workhorse-cargo-work
description: Use as the entry skill for Workhorse `cargo work` tasks. It classifies the request first, then routes to the right sub-skill for remote Rust builds, remote shell or proxy access, or artifact retrieval, Git sync, and server inspection on a `horsed` server.
---

# Workhorse Cargo Work

## Overview

This is the dispatcher skill for the `cargo-work` side of Workhorse. Use it when a request mentions `cargo work`, `horsed`, remote Rust builds, remote command execution, artifact download, or Workhorse server inspection, and decide which specialized skill should take over.

## Routing Rules

Route to exactly one primary sub-skill whenever possible:

- Use `$workhorse-remote-build` for remote `build`, `test`, `check`, `clippy`, `run`, `install`, `doc`, `metadata`, `rustc`, `zigbuild`, or `just`.
- Use `$workhorse-remote-access` for `cargo work -- ...`, `cargo work ssh`, interactive shells, PTY-backed commands, port forwarding, remote env injection, or proxy bridging.
- Use `$workhorse-artifact-sync` for `get`, `scp`, `push`, `pull`, `ping`, `health`, or `logs`.

## Triage Checklist

1. Confirm whether the task is about build execution, remote access, or retrieval and inspection.
2. Confirm how the target server will be resolved: `--repo-name`, `--repo`, Git remote `horsed`, or `HORSED`.
3. If the request spans build plus download, start with `$workhorse-remote-build` and then switch to `$workhorse-artifact-sync`.
4. If the request spans access plus forwarding or proxy setup, keep it under `$workhorse-remote-access`.
5. If repo or host resolution is missing, ask for the server URL or remote name instead of guessing.

## Common Intents

- "帮我在远端跑 `cargo test`" -> `$workhorse-remote-build`
- "去服务器上执行一个命令" -> `$workhorse-remote-access`
- "开一个 SSH shell 并转发 3000 端口" -> `$workhorse-remote-access`
- "把远端构建好的二进制拉回来" -> `$workhorse-artifact-sync`
- "看看 horsed 现在健不健康" -> `$workhorse-artifact-sync`

## Shared Assumptions

- Most commands assume the current directory is inside a Git repository.
- Most commands fail if neither `--repo` nor a `horsed` Git remote can identify the target.
- The client will auto-discover `~/.ssh/id_rsa` or `~/.ssh/id_ed25519` unless `--ssh-key` is provided.
- `cargo work watch` exists but is not reliable enough to recommend as a default workflow.

## Handoff Policy

- Keep this skill thin. Do not restate all command details here.
- After classifying the request, load only the chosen sub-skill.
- Load a second sub-skill only when the task genuinely crosses boundaries, such as remote build followed by artifact download.

## Sub-skills

- [workhorse-remote-build](../workhorse-remote-build/SKILL.md)
- [workhorse-remote-access](../workhorse-remote-access/SKILL.md)
- [workhorse-artifact-sync](../workhorse-artifact-sync/SKILL.md)
