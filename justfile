default:
  just --list

build:
  cargo build --color=always

install-work:
  cargo install --path ./cargo-work --bin cargo-work --color=always

install-horsed:
  cargo install --path ./horsed --bin horsed --color=always

install: install-work install-horsed
