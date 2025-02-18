### v0.2.6

- workhorse: cargo work ssh to get a shell (no windows support)
- cargo-work: HorseOptions --key-hash-alg: sha256(default), sha512
- workhorse: ssh -L & -R compatible with openssh
- horsed: channel_open_direct_tcpip tracing
- cargo-work: watch command dev in progress
- cargo-work: set a fallback command for `cargo work`
- cargo-work: rename src/ssh to src/command
- horsed: remove `ssh-keys` crate
- horsed: fix test with different ports
- horsed: ssh server tests
- cargo-work: ssh command to do some ssh stuff, e.g. port forwarding
- stable: more rstest
- cargo-work: get file with `--outfile` option
- stable: add a thread pool
- horsed: just support os specified file `justfile.<os>`
- horsed: fix ping cleanup

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
