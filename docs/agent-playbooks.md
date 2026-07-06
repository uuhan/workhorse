# Agent Playbooks

## 1) Remote Build Playbook

### Preconditions
- Target server is resolvable via `--repo`, `--repo-name`, or git remote `horsed`.
- SSH key is available (`--ssh-key` or default key path).

### Steps
1. `cargo work ping --count 1`
2. `cargo work build --release` (or `cargo work test` for test tasks)
3. If needed, attach output: `cargo work job list` then `cargo work job attach <job_id> -f`

### Fallback
- If repo/host resolve fails: provide `--repo ssh://git@HOST:2222/ns/repo.git`.
- If command hangs: retry with `RUST_LOG=info WH_DEBUG=1` to collect staged traces.

### Acceptance Signals
- Exit code is `0`.
- Output contains successful completion from Cargo.

## 2) Horsed Deploy Playbook

### Preconditions
- Remote branch is up to date.
- Explicit confirmation if service restart is required.

### Steps (Linux/macOS)
1. `just install-work`
2. `HORSED_SHELL=/bin/bash cargo work just install-horsed`
3. `HORSED_SHELL=/bin/bash cargo work -- systemctl --user restart horsed`
4. `cargo work health --json`

### Steps (Windows)
1. `just install-work`
2. `HORSED_SHELL=powershell.exe cargo work just deploy-horsed`
3. `cargo work health --json`

### Fallback
- If `nu` is missing on server, use `/bin/bash`, `/bin/sh`, or `powershell.exe`.
- If post-restart health fails, inspect: `cargo work logs -f`.

### Acceptance Signals
- `health --json` returns parseable JSON with `status: "ok"`.

## 3) Artifact Retrieval Playbook

### Preconditions
- Remote build already completed.

### Steps
1. Retrieve a file: `cargo work get target/release/<artifact> -f`
2. Retrieve a directory: `cargo work get target -f`
3. Alternative stream copy: `cargo work scp <remote_file> <local_file>`

### Fallback
- If local file exists and retrieval fails, use `-f` or `--outfile`.

### Acceptance Signals
- Retrieved path exists locally.
- Artifact checksum/size matches expected output (when available).

## 4) Health/Logs Troubleshooting Playbook

### Preconditions
- Server is reachable.

### Steps
1. `cargo work ping --count 3`
2. `cargo work health --json`
3. `cargo work logs` (or `cargo work logs -f`)
4. `cargo work job list`

### Fallback
- If health output seems empty in normal mode: `RUST_LOG=info cargo work health`
- For deeper traces: `RUST_LOG=info WH_DEBUG=1 cargo work health`

### Acceptance Signals
- JSON includes stable fields: `status`, `protocol`, `ulimit_nofile`.
- Logs show expected service state transitions.

## 5) Remote Exec Playbook

### Preconditions
- Target server is resolvable via `--repo`, `--repo-name`, or git remote `horsed`.
- SSH key is available (`--ssh-key` or default key path).
- Remote host has `bash` and `base64 -d` available.

### Steps
1. Use `cargo work -- <cmd>` for a short one-line command.
2. Use a quoted heredoc for multi-line scripts or commands containing JSON/headers/quotes:

```bash
cargo work exec <<'EOF'
set -euo pipefail
printf '%s\n' '{"name":"demo app","ok":true}'
EOF
```

3. If environment-dependent tools such as `pnpm`, `nvm`, `fnm`, or cargo shims are missing, confirm they are configured from the remote login profile used by bash/zsh `-lc`.

### Fallback
- If `exec` fails because `bash` or `base64 -d` is missing, use `cargo work -- ...` with explicit command paths or a server-supported shell.
- If repo/host resolve fails, pass `--repo ssh://git@HOST:2222/ns/repo.git`.

### Acceptance Signals
- The command exits with code `0`.
- Script output matches the expected stdout/stderr without local-shell quote expansion.
