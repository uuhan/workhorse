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
