# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust workspace centered on remote build tooling.

- `horsed/`: server binary (`horsed`) and database/SSH handling logic.
- `cargo-work/`: client binary (`cargo-work`) implemented as a Cargo subcommand.
- `stable/`: shared library code used across workspace crates.
- `horsed/migration/`: SeaORM migrations for server-side schema changes.
- `scripts/`: helper scripts (release notes, install helpers, autostart units).
- `docs/`: project assets (logo, docs media).
- `ci/` and `zebra/`: local fixtures/resources; treat keys and DB files as test data only.

## Build, Test, and Development Commands
Use the workspace root for all commands.

- `cargo build --bin cargo-work --bin horsed`: build the two main binaries (matches CI).
- `cargo test --verbose`: run all workspace tests (matches CI on Linux/macOS/Windows).
- `cargo build --release --bin cargo-work --bin horsed`: produce release binaries.
- `just build`: convenience wrapper for `cargo build`.
- `just install`: install both binaries locally (`cargo-work`, `horsed`).
- `just install-horsed-with-trace`: install `horsed` with Tokio unstable tracing enabled.
- `cargo work just install-horsed`: run remote `just install-horsed` on the configured `horsed` target to update the server-side `horsed` binary.
- `cargo work -- systemctl --user restart horsed`: restart the remote user-level `horsed` service on Linux hosts after update.

## Frontend/Backend Update Workflow
Use this flow when updating both client (`cargo-work`) and server (`horsed`):

1. Update local client binary:
   - `just install-work`
2. Update remote server binary (force a shell available on the server):
   - `HORSED_SHELL=/bin/bash cargo work just install-horsed`
3. Restart remote `horsed` service:
   - `HORSED_SHELL=/bin/bash cargo work -- systemctl --user restart horsed`

Notes:
- If the server does not provide `nu`, do not use `HORSED_SHELL=nu`; prefer `/bin/bash` or `/bin/sh`.
- Remote install builds from the remote repository state (usually `main`). To deploy local changes, commit and push first.

## Coding Style & Naming Conventions
- Rust edition is `2021`; follow standard Rust formatting (`cargo fmt`).
- Use `snake_case` for functions/modules, `CamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for constants.
- Keep crate boundaries clear: reusable logic belongs in `stable/`, transport/server concerns in `horsed/`, client UX/CLI in `cargo-work/`.
- Prefer explicit error propagation (`anyhow`/typed errors) and structured logs with `tracing`.

## Testing Guidelines
- Primary test command: `cargo test --verbose`.
- Unit tests are colocated in modules (`#[cfg(test)]`) and crate-specific files (for example `horsed/src/ssh/tests.rs`).
- `rstest` is available for parameterized tests; use it when matrix-style input coverage is needed.
- Add regression tests for protocol, IPC, and path-handling fixes (common failure areas).

## Project Skills
- `./AI_AGENT.md`: single entry for AI agents, including routing rules, standard commands, success criteria, and safety boundaries.
- `./skills/index.json`: machine-readable skill index for deterministic task-to-skill routing and risk classification.
- `./skills/workhorse/SKILL.md`: top-level dispatcher for this repo; use first when the task broadly mentions Workhorse.
- `./skills/workhorse-cargo-work/SKILL.md`: entry skill for client-side `cargo work` usage.
- `./skills/workhorse-remote-build/SKILL.md`: remote Cargo and `just` build/test/lint/run workflows.
- `./skills/workhorse-remote-access/SKILL.md`: remote shell, one-off commands, PTY, proxy, and port forwarding.
- `./skills/workhorse-artifact-sync/SKILL.md`: artifact download, Git sync, ping, health, and log inspection.
- `./skills/workhorse-horsed/SKILL.md`: entry skill for server-side `horsed` tasks.
- `./skills/workhorse-horsed-setup/SKILL.md`: first-time `horsed` bootstrap, first user enrollment, and safe setup mode.
- `./skills/workhorse-horsed-ops/SKILL.md`: runtime operations, autostart templates, logs, and troubleshooting.
- `./skills/workhorse-horsed-dev/SKILL.md`: `horsed` server code, migrations, protocol changes, and tests.
- When a request involves `cargo work`, `horsed`, remote builds, artifact retrieval, server setup, or Workhorse internals, load the matching project skill before improvising from README text.
- Keep this section in sync with `./skills/`; when a skill is added, renamed, or removed, update `AGENTS.md` in the same change.

## Commit & Pull Request Guidelines
- Follow the existing commit style: concise, imperative subject with optional scope/prefix, e.g. `feat: ...`, `fix: ...`, `cargo-work: ...`, `horsed: ...`.
- Keep commits focused by crate or behavior.
- PRs should include:
  - what changed and why,
  - test evidence (`cargo test --verbose`, plus platform notes if relevant),
  - linked issue(s),
  - CLI screenshots/log snippets when behavior/output changes.
