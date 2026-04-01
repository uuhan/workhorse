---
name: workhorse-horsed-ops
description: Use when the user wants to operate a running `horsed` server, wire it into `systemd`, `launchd`, or Windows Task Scheduler, inspect logs and health, choose between foreground and daemon modes, or troubleshoot runtime problems around ports, file descriptors, working directory, and setup mode.
---

# Workhorse Horsed Ops

## Overview

This skill covers runtime operations for `horsed`. Use it for autostart setup, day-2 troubleshooting, log and health inspection, and service-manager-specific guidance.

## Runtime Model

- Main SSH service listens on `2222`.
- Setup mode listens on `2223` before first-user enrollment, or while `--dangerous` is active.
- `horsed` daemonizes by default when run without `-f`.
- `--show-log` writes logs to stdout; otherwise logs go to `horsed.log` with daily rotation and up to 15 files.

## Recommended Service Shape

For `systemd`, `launchd`, and Windows Task Scheduler, prefer foreground mode:

```bash
horsed -f
```

The repo already contains service templates:

- Linux: `../../scripts/autostart/linux/horsed.service`
- macOS: `../../scripts/autostart/macos/local.horsed.plist`
- Windows: `../../scripts/autostart/windows/horsed.xml`

When adapting them:

- Set the real binary path.
- Set the real working directory outside the process, because `--dir` is not currently wired through.
- Preserve a high file descriptor limit where supported. The Linux and macOS templates target `65535`.
- If the service manager should own stdout and stderr, add `--show-log` only when that is intentional.

## Inspection and Troubleshooting

- From the client side, use:

```bash
cargo work ping --count 3
cargo work health
cargo work logs
cargo work logs -f
cargo work job list
cargo work job attach <job_id> -f
```

- `health` reports `version`, `commit`, `os/arch/family`, default shell, and `ulimit -n` (Unix).
- `cargo work ping` is continuous when `--count` is omitted; for diagnostics and agent automation, always pass `--count` unless continuous probing is explicitly requested.
- If `health` appears to hang or return nothing, verify server version compatibility first, then retry with `RUST_LOG=info WH_DEBUG=1 cargo work health`.
- `logs` reads the in-memory ring buffer, not just the file on disk.
- `job attach` is the preferred path for attaching to one running build/test command by `job_id` without tailing unrelated service logs.
- If port `2223` is still open unexpectedly, check whether `horsed.key` is missing or `--dangerous` was enabled.

## Common Failure Modes

- Service manager starts `horsed` without `-f`, so the monitored process exits after daemonization.
- Work directory is wrong, so a new `horsed.db3` or `horsed.key` is created in the wrong place.
- File descriptor limit is too low for large build workloads.
- The first-user setup was never completed, so only setup mode is available.

## Administrative Commands

- `horsed user add --name NAME` and `horsed user del NAME` are implemented.
- `horsed user mod` and `horsed user list` exist in the CLI but do not currently have working behavior in `main.rs`.
