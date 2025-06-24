### v0.2.8

- cargo-work: 兼容 windows 的路径
- horsed: fix promise test case
- horsed: fix just args
- cargo-work: pass GIT_COMMIT & GIT_MESSAGE to horsed
- fix: rust version update
- horsed: fix windows compilation
- cargo-work: fix ssh ExtendedData output to stderr
- horsed: fix ssh bad arguments
- cargo-work: `pty` options in `HorseOptions` to support run command with an allocated pty
- cargo-work: alias `work` to `cargo`
- cargo-work: watch command dev in progress

### v0.2.7

- workhorse: logs to frontend with `logs` command
- trough: for cross platform pty support
- horsed: use updated winpty-rs
- cargo-work: sleep & check the terminal size
- cargo-work: window resize dev in progress
- horsed: fix windows pty input
- horsed: fix windows pty exit
- horsed: use conpty, remove winpty
- horsed: use thread safe PTY from winpty-rs
- horsed: impl windows-pty with winpty-rs
- cargo-work: fix pass correct tty size
- cargo-work: pass default log directive
- cargo-work: enable proxy for ssh command
- cargo-work: fix pass correct tty size
- cargo-work: use custom logger
- cargo-work: fix HorseOptions merge `enable_proxy` & `all_proxy`
- cargo-work: ping <REMOTE> support

### v0.2.6

- cargo-work: ping <REMOTE> support
- cargo-work: just, cmd support proxy
- cargo-work: cargo command support proxy
- cargo-work: fix ed25519 ssh private key with not hash alg
- cargo-work: push to default remote: horsed
- cargo-work: add support for http proxy (dev in progress)
- cargo-work: push <REMOTE> <BRANCH> just as git push do
- cargo-work: fix HorseOptions merge issue
- horsed: fix ssh -R dispatched socket handler
- workhorse: HorseOptions --env support pass K=V env pairs
- workhorse: pre-release v0.2.6-alpha.1
- workhorse: cargo work ssh to get a shell
- cargo-work: HorseOptions --key-hash-alg: sha256(default), sha512
- workhorse: ssh -L & -R compatible with openssh
- cargo-work: ssh -L & -R
- horsed: channel_open_direct_tcpip tracing
- cargo-work: set a fallback command for `cargo work`
- horsed: remove `ssh-keys` crate
- horsed: fix test with different ports
- horsed: ssh server tests
- cargo-work: ssh command to do some ssh stuff, e.g. port forwarding
- cargo-work: get file with `--outfile` option
- stable: add a thread pool
- horsed: just support os specified file `justfile.<os>`

### v0.2.5

- horsed: fix tracing registry
- horsed: add `opentelemetry` feature to support otlp exporter
- horsed: fix a bad dead lock issue

### v0.2.4

- horsed: more tracing logs
- workhorse: RUSTFLAGS="--cfg tokio_unstable" to add tokio-console support on port 6669
- horsed: fix just command kill on drop
- horsed: seems the stream handler may block?
- workhorse: cleanup deps
- horsed: fix bad cmd stdout & stderr copy logic
- horsed: kill cmd process after drop
- README

### v0.2.3

- README
- add some tracing logs
- horsed: use custom run_on_socket implement
- workhorse: use russh v0.49.3-alpha.0
- cargo-work: support specify shell intepreter
- horsed: fix cargo command fault tolerance
- horsed: fix just command stdout & stderr
- cargo-work: use ratatui-0.30.0-alpha.1
- cargo-work: get output to stdout if it is piped
- workhorse: pre-release for v0.2.3
- russh: use patched repo to fix "early eof" issue

### v0.2.2

- cargo-work: use russh client as default ssh client
- cargo-work: fix just command with russh client
- cargo-work: cmd & just & scp with russh client
- cargo-work: get file with russh client
- stable: buffer Writer flush()

### v0.2.1

- workhorse: remove russh-keys crate
- horsed: use powershell.exe insted of cmd.exe
- horsed: fix ssh get file hangs
- cargo-work: work init support
- workhorse: bump to v0.2.1
- cargo-work: push & pull support <BRANCH> arg
- stable: Head & Body fn read
- horsed: fix just command run with no window

### v0.2.0

- cargo-work: merge HorseOptions
- workhorse: downgrade russh to 0.49.2
- workhorse: add cargo clean subcommand
- cargo-work: git push & pull stdout
- cargo-work: pull <REMOTE>
- cargo-work: push <REMOTE>
- cargo-work: ping -c <COUNT>
- cargo-work: record total time
- workhorse: ping test
- use russh-0.50.0-workhorse
- cargo-work: HorseOptions --remote to specify horsed remote
- workhorse: cleanup windows code
- horsed: fix git receive-pack command
- horsed: fix git receive-pack command
- horsed: log directory compression ratio
- cargo-work: fix get file with correct uncompressed size
- workhorse: downgrade russh to 0.49.2
- workhorse: use russh v0.50.0, but it seems buggy 😢
- cargo-work: russh client breaks early
- cargo-work: russh client blocks
- cargo-work: use color_eyre instead of anyhow
- cargo-work: cargo work get --stdout to write data to stdout
- workhorse: protocol bump to v2
- workhorse: v0.2.0
- workhorse: The ui framework uses the demo2 example from ratatui & bump to russh-0.50

### v0.1.5

- workhorse: bump to v0.1.5
- workhorse: cleanup & fix cargo work just with path
- cargo-work: fix lock file exclusively
- horsed: fix one key associates only one user
- horsed: fix cmd dir if workspace not exists
- stable: buffer Writer return Err when Reader drops
- workhorse: bump to v0.1.4
