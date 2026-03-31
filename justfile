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

# 重启远程 horsed (仅限 Windows, 通过 cargo work 调用)
# 延迟 3 秒让 SSH 连接正常关闭, 然后 停止→拷贝→启动
[windows]
restart-horsed:
  @powershell -Command "Start-Process powershell -ArgumentList '-NoProfile -Command \"Start-Sleep 3; Stop-Process -Name horsed -Force -ErrorAction SilentlyContinue; Start-Sleep 1; Copy-Item $env:CARGO_HOME\\bin\\horsed.exe D:\\horsed.exe -Force; Set-Location D:\\; Start-Process .\\horsed.exe\"' -WindowStyle Hidden"
  @echo "horsed 将在 3 秒后重启"

# 一键部署: 构建 + 重启
[windows]
deploy-horsed: install-horsed restart-horsed

install: install-work install-horsed
  @echo "[{{os()}}-{{arch()}}] 安装成功: cargo-work, horsed"

changes:
  @bash scripts/changes.sh

get-release:
  cargo work get ./target/release/cargo-work -f
  cargo work get ./target/release/horsed -f
