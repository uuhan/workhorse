---
name: workhorse-horsed-dev
description: Use when the user wants to modify the `horsed` server implementation itself, including SSH server behavior, setup flow, logging, IPC, SQLite schema, migrations, service startup logic, or server-side tests.
---

# Workhorse Horsed Dev

## Overview

This skill is for changing `horsed` code, not just operating a deployed server. Use it when the task touches server-side actions, database schema, process lifecycle, logging, or setup behavior.

## Code Map

- `../../horsed/src/main.rs`: process model, CLI, daemon and foreground startup.
- `../../horsed/src/ssh/mod.rs`: main SSH server on `2222` and action dispatch.
- `../../horsed/src/ssh/setup.rs`: first-user bootstrap server on `2223`.
- `../../horsed/src/ssh/health.rs`: `health` action and `ulimit -n` reporting.
- `../../horsed/src/logger/mod.rs`: stdout/file/ring-buffer logging and optional OpenTelemetry.
- `../../horsed/src/db/` and `../../horsed/migration/`: SQLite schema and migrations.
- `../../horsed/src/options/`: CLI surface.

## Development Workflow

1. Identify whether the change belongs in `horsed`, `cargo-work`, or `stable`.
2. If the change alters protocol or server actions, inspect both `../../horsed/src/ssh/mod.rs` and the client-side caller.
3. If the change alters persisted data, add or update a migration in `../../horsed/migration/`.
4. Add or update tests before relying on manual validation.

## Build and Test Commands

```bash
cargo build -p horsed
cargo test -p horsed --verbose
cargo test --workspace --verbose
just install-horsed
just install-horsed-with-trace
```

For migrations:

```bash
cargo run -p migration -- up
cargo run -p migration -- status
cargo run -p migration -- fresh
```

## Testing Guidance

- Existing server auth/setup coverage lives in `../../horsed/src/ssh/tests.rs`.
- When changing bootstrap auth or server action routing, extend those tests or add adjacent ones.
- When changing logging or startup behavior, manual verification in a throwaway work directory is still useful because runtime side effects include DB, key, and log files.

## Known Edges

- `horsed user mod` and `horsed user list` are declared in the CLI but not implemented in `main.rs`.
- The `--dir` CLI option is present but does not currently drive a directory change.
- The service templates under `../../scripts/autostart/` may need foreground-mode adjustments if they are used with a process supervisor.

## Change Patterns

- Startup or lifecycle changes: start at `../../horsed/src/main.rs`.
- New remote action or protocol path: start at `../../horsed/src/ssh/mod.rs` and the corresponding client command.
- Setup enrollment logic: start at `../../horsed/src/ssh/setup.rs`.
- Health and observability changes: start at `../../horsed/src/ssh/health.rs` and `../../horsed/src/logger/mod.rs`.
- Schema or user-key relation changes: start at `../../horsed/src/db/entity/` plus `../../horsed/migration/`.

## When Not To Use This Skill

- Do not use this skill for normal remote builds or artifact retrieval; use the `cargo-work` skills instead.
- Do not use this skill for day-2 deployment guidance; use `workhorse-horsed-ops`.
