---
name: workhorse-horsed
description: Use as the entry skill for `horsed` server tasks. It classifies the request first, then routes to the right sub-skill for first-time server setup, runtime operations and autostart, or `horsed` code, schema, and protocol development.
---

# Workhorse Horsed

## Overview

This is the dispatcher skill for the `horsed` side of Workhorse. Use it when a request mentions `horsed`, server bootstrap, setup mode on `2223`, daemon or foreground behavior, autostart, logs, health, or changes to the `horsed` server implementation.

## Routing Rules

Route to one primary sub-skill whenever possible:

- Use `$workhorse-horsed-setup` for first-time bootstrap, safe startup mode, setup server behavior on `2223`, first-user enrollment, `horsed.key`, `horsed.db3`, and `--dangerous`.
- Use `$workhorse-horsed-ops` for day-2 operation, logs, health, autostart templates, service managers, runtime troubleshooting, and admin command behavior.
- Use `$workhorse-horsed-dev` for server code changes in `horsed/src`, migrations, logging internals, IPC, SSH action dispatch, or tests.

## Triage Checklist

1. Determine whether the user is trying to bootstrap, operate, or modify the server.
2. Check whether the task is about the `horsed` process itself or about `cargo work` on the client side.
3. If the task spans setup and later operation, start with `$workhorse-horsed-setup` and then hand off to `$workhorse-horsed-ops`.
4. If the task spans runtime troubleshooting and code change, start with `$workhorse-horsed-ops` for symptom framing and then hand off to `$workhorse-horsed-dev`.
5. If the user only wants remote builds, remote commands, or artifact download, do not stay in this skill. Switch to the `cargo-work` dispatcher instead.

## Common Intents

- "帮我把 `horsed` 首次启动起来" -> `$workhorse-horsed-setup`
- "为什么 2223 还开着" -> `$workhorse-horsed-ops`
- "给我配 systemd/launchd/Windows 自启动" -> `$workhorse-horsed-ops`
- "我要改 `horsed` 的 setup 逻辑" -> `$workhorse-horsed-dev`
- "我要改服务端日志或 health 行为" -> `$workhorse-horsed-dev`

## Shared Assumptions

- `horsed` writes state into its working directory.
- Main SSH service is on `2222`; setup mode is on `2223`.
- Plain `horsed` daemonizes by default; supervised environments generally want `horsed -f`.
- Some CLI surface exists without full implementation, so always verify behavior against current code before promising it.

## Handoff Policy

- Keep this skill thin and route quickly.
- Load only one sub-skill unless the task genuinely crosses phases.
- If the task is both server-side and client-side, switch between `workhorse-horsed` and `workhorse-cargo-work` instead of mixing both domains in one skill body.

## Sub-skills

- [workhorse-horsed-setup](../workhorse-horsed-setup/SKILL.md)
- [workhorse-horsed-ops](../workhorse-horsed-ops/SKILL.md)
- [workhorse-horsed-dev](../workhorse-horsed-dev/SKILL.md)
