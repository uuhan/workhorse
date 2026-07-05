---
name: workhorse
description: Use as the top-level entry skill for this repository. It classifies Workhorse requests first, then routes to the right dispatcher for `cargo work` client tasks or `horsed` server tasks, including remote builds, remote access, artifact sync, server setup, operations, and `horsed` development.
---

# Workhorse

## Overview

This is the top-level dispatcher for the Workhorse repository. Use it when a request mentions Workhorse in general, and decide first whether the task belongs to the client-side `cargo work` path or the server-side `horsed` path.

## First Split

- Route to `$workhorse-cargo-work` when the request is about remote builds, remote command execution, SSH forwarding, artifact download, Git sync through `cargo work`, or client-side health and logs.
- Route to `$workhorse-horsed` when the request is about server bootstrap, setup mode, runtime operation, autostart, logs from the server process, or changes to `horsed` implementation.

## Triage Checklist

1. Identify whether the user is controlling a remote server through `cargo work` or operating the `horsed` service itself.
2. If the task is phrased in terms of `cargo work ...`, `git remote horsed`, `get/scp/logs/health`, or remote Cargo commands, use `$workhorse-cargo-work`.
3. If the task is phrased in terms of `horsed`, port `2222` or `2223`, setup mode, `horsed.key`, `horsed.db3`, `systemd`, `launchd`, or service startup, use `$workhorse-horsed`.
4. If the request spans both, split the work into phases and hand off between the two dispatchers explicitly.

## Common Intents

- "帮我在远端编译这个 Rust 项目" -> `$workhorse-cargo-work`
- "用 `cargo work exec` 在远端跑一段脚本" -> `$workhorse-cargo-work`
- "帮我把远端产物拉回来" -> `$workhorse-cargo-work`
- "帮我首次配置一台 horsed 服务器" -> `$workhorse-horsed`
- "给 horsed 配开机自启" -> `$workhorse-horsed`
- "我要改服务端 setup 逻辑" -> `$workhorse-horsed`

## Shared Assumptions

- Workhorse has two distinct sides: `cargo-work` as the client and `horsed` as the server.
- Do not mix client and server guidance in one skill body unless the task genuinely spans both.
- Keep this skill thin and route quickly.

## Handoff Policy

- Load only the chosen dispatcher after classification.
- Load a second dispatcher only when a user flow crosses the client/server boundary, such as bootstrapping a new `horsed` server and then wiring a repo to use `cargo work`.
- Let the downstream dispatcher choose the final specialized skill.

## Dispatchers

- [workhorse-cargo-work](../workhorse-cargo-work/SKILL.md)
- [workhorse-horsed](../workhorse-horsed/SKILL.md)
