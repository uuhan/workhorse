### v0.2.0

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
- horsed: log directory compression ratio
- cargo-work: fix get file with correct uncompressed size
- workhorse: downgrade russh to 0.49.2
- workhorse: use russh v0.50.0, but it seems buggy 😢
- cargo-work: russh client breaks early
- cargo-work: russh client blocks
- cargo-work: ui
- cargo-work: use color_eyre instead of anyhow
- cargo-work: cargo work get --stdout to write data to stdout
- workhorse: protocol bump to v2
- workhorse: v0.2.0
- workhorse-2.0: The ui framework uses the demo2 example from ratatui & bump to russh-0.50
- CHANGELOGS.md

### v0.1.5

- workhorse: bump to v0.1.5
- workhorse: cleanup & fix cargo work just with path
- cargo-work: fix lock file exclusively
- horsed: fix one key associates only one user
- horsed: fix cmd dir if workspace not exists
- stable: buffer Writer return Err when Reader drops
- workhorse: bump to v0.1.4
