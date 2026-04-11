# AI Agent Entry (Claude Code / Codex)

This file is the single entry point for AI agents operating on this repository.

## Primary Routing

1. Read `skills/index.json`.
2. Classify task into one domain:
   - `cargo-work` client workflow
   - `horsed` server workflow
   - cross-boundary workflow
3. Dispatch to the matching skill in `skills/`.
4. Use standard playbooks in `docs/agent-playbooks.md`.

## Task Classification -> Skill

- General Workhorse triage -> `skills/workhorse/SKILL.md`
- `cargo work` remote build/test/check/clippy/run/just -> `skills/workhorse-remote-build/SKILL.md`
- `cargo work ssh` / raw remote commands / forwarding / proxy -> `skills/workhorse-remote-access/SKILL.md`
- `get/scp/push/pull/ping/health/logs/job` -> `skills/workhorse-artifact-sync/SKILL.md`
- `horsed` bootstrap / first user / setup mode -> `skills/workhorse-horsed-setup/SKILL.md`
- `horsed` ops / service manager / troubleshooting -> `skills/workhorse-horsed-ops/SKILL.md`
- `horsed` code, migration, protocol, tests -> `skills/workhorse-horsed-dev/SKILL.md`

## Standard Commands

- Build binaries: `cargo build --bin cargo-work --bin horsed`
- Workspace tests: `cargo test --verbose`
- Health (human): `cargo work health`
- Health (machine): `cargo work health --json`
- Logs: `cargo work logs` / `cargo work logs -f`
- Jobs: `cargo work job list` / `cargo work job attach <job_id> -f`

## Success Criteria

- Build/test tasks: command exits with code `0`.
- Remote build tasks: expected artifact exists on remote and can be fetched with `cargo work get`.
- Health check (JSON mode): JSON parse succeeds and includes `status`, `protocol`, `ulimit_nofile` fields.
- Ops tasks: service state and logs match expected behavior.

## Safety Boundaries

- Low risk: read-only inspection (`ping`, `health`, `logs`, `job list`).
- Medium risk: remote command execution, file sync, interactive shell, forwarding/proxy.
- High risk: `horsed --dangerous`, service restart/replace, user/key admin mutation.

## Confirmation Policy

Require explicit confirmation from the user before high-risk actions:

- enabling `--dangerous`
- restarting/stopping production `horsed`
- overwriting binaries/state in remote deploy paths
- destructive user/key/admin operations

## Related Assets

- Agent skill index: `skills/index.json`
- Agent playbooks: `docs/agent-playbooks.md`
- Human-oriented guidance: `README.md`, `README.en.md`, `AGENTS.md`
