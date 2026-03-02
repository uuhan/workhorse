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

## Commit & Pull Request Guidelines
- Follow the existing commit style: concise, imperative subject with optional scope/prefix, e.g. `feat: ...`, `fix: ...`, `cargo-work: ...`, `horsed: ...`.
- Keep commits focused by crate or behavior.
- PRs should include:
  - what changed and why,
  - test evidence (`cargo test --verbose`, plus platform notes if relevant),
  - linked issue(s),
  - CLI screenshots/log snippets when behavior/output changes.
