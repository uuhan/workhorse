---
name: workhorse-horsed-setup
description: Use when the user wants to initialize a new `horsed` server, choose a safe startup mode, bootstrap the first SSH user on port `2223`, understand generated server files like `horsed.db3`, `horsed.key`, and `horsed.log`, or use `--dangerous` during setup.
---

# Workhorse Horsed Setup

## Overview

This skill handles first-time `horsed` setup and bootstrap. Use it when the server does not exist yet, when the first user key must be enrolled, or when the user needs to understand what files and ports the server will create.

## Safe Defaults

- Start `horsed` in a clean dedicated working directory. The server writes `horsed.db3`, `horsed.key`, and `horsed.log` into the current directory.
- Use `horsed -f --show-log` during bootstrap so logs stay visible in the terminal.
- Expect the main service on port `2222`.
- Expect the temporary setup server on port `2223` until the first user completes enrollment.

## Bootstrap Workflow

1. Create or choose the server work directory.
2. Start the server in foreground mode:

```bash
horsed -f --show-log
```

3. Connect once to the setup server from another terminal:

```bash
ssh -p 2223 YOUR_NAME@SERVER
```

4. After setup exits, keep the main service running on `2222`.
5. Configure clients against the new repo URL, for example:

```bash
git remote add horsed ssh://git@SERVER:2222/USER/REPO.git
```

## Dangerous Mode

- `horsed -f --show-log --dangerous` keeps the setup server resident on `2223`.
- In this mode, any client that connects to `2223` can register a public key.
- Use it only for controlled one-off maintenance windows, never as the default deployment shape.

## Generated State

- `horsed.db3`: SQLite database in the current working directory.
- `horsed.key`: persistent Ed25519 private key for the main SSH server on `2222`.
- `horsed.log`: rolling log output when `--show-log` is not enabled.

## Caveats

- The CLI has a `--dir` option, but the current entrypoint does not actually `chdir` into it. Use the process working directory itself as the source of truth.
- Plain `horsed` daemonizes by default. For supervised environments and debugging, prefer `horsed -f`.
- The temporary setup server uses a transient key on `2223`; the persistent `horsed.key` belongs to the main service on `2222`.
