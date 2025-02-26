default:
  @just --list

build:
  @cargo build --color=always

install-work:
  @cargo install --locked --path ./cargo-work --bin cargo-work --color=always

install-horsed-with-trace:
  @env RUSTFLAGS="--cfg tokio_unstable" \
  cargo install --path ./horsed --bin horsed --color=always --features opentelemetry

install-horsed:
  @cargo install --locked --path ./horsed --bin horsed --color=always

install: install-work install-horsed
  @echo "[{{os()}}-{{arch()}}] 安装成功: cargo-work, horsed"

changes:
  @bash scripts/changes.sh

get-release:
  cargo work get ./target/release/cargo-work -f
  cargo work get ./target/release/horsed -f
