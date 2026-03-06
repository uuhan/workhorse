---
name: workhorse-remote-access
description: Use when the user wants remote shell access or ad-hoc command execution through `cargo work`, including raw `cargo work -- ...` commands, `cargo work ssh`, port forwarding with `-L` or `-R`, environment injection, PTY allocation, shell selection, and reverse proxy setup with `-x` or `--all-proxy`.
---

# Workhorse Remote Access

## Overview

This skill covers interactive access and one-off remote execution against a `horsed` server. Use it when the task is not a standard Cargo workflow and needs a shell, a custom command, forwarded ports, or proxy plumbing.

## Preconditions

- Resolve target repo and host the same way as other `cargo-work` commands: `--repo-name`, `--repo`, or Git remote `horsed`; host from `HORSED`, `--repo`, or Git remote `horsed`.
- If the command is interactive, prefer PTY-backed execution.
- If the user just needs Cargo subcommands like `build` or `test`, use `$workhorse-remote-build` instead.

## Command Patterns

For raw command execution:

```bash
cargo work -- ls -al
cargo work --repo ssh://git@127.0.0.1:2222/uuhan/workhorse.git -- uname -a
cargo work -t -- bash -lc 'htop'
```

For interactive shell access:

```bash
cargo work ssh
cargo work ssh bash
cargo work --shell nu -- ls
```

For forwarding and proxying:

```bash
cargo work ssh -L 3000:127.0.0.1:3000
cargo work ssh -R 8080:127.0.0.1:8080
ALL_PROXY=socks5://127.0.0.1:1080 cargo work -x -- curl -I https://example.com
```

## Decision Rules

1. Use `cargo work -- <cmd>` for one-shot remote commands.
2. Use `cargo work ssh` when the user wants an interactive shell or port forwarding.
3. Use `-t` when the command expects a terminal UI or interactive input.
4. Use `--env KEY=VALUE` for small environment overrides.
5. Use `--shell` or `HORSED_SHELL` when the default interpreter is wrong for the remote platform.
6. Use `-x` when the server should route outbound traffic through the local proxy. Use `--all-proxy=...` when the proxy URL should be explicit.

## Notes

- `cargo work ssh` defaults to an interactive shell on the server when no command is provided.
- Reverse proxy mode picks a random remote port and exports `ALL_PROXY`, `HTTP_PROXY`, and `HTTPS_PROXY` back into the remote environment.
- If `-x` is set without local `ALL_PROXY` or `all_proxy`, the command warns and continues without proxy setup.

## Examples

```bash
# Start an interactive shell on the default server
cargo work ssh

# Run a command with extra env
cargo work --env RUST_LOG=debug -- printenv RUST_LOG

# Forward local 3000 to the remote app
cargo work ssh -L 3000:127.0.0.1:3000

# Publish a local service through the server
cargo work ssh -R 8080:127.0.0.1:8080
```
