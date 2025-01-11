default:
  just --list

build:
  cargo build --color=always

install-work:
  cargo install --path ./cargo-work --bin cargo-work

install-horsed:
  cargo install --path ./horsed --bin horsed

install: install-work install-horsed
