<p align="center">
    <img src="docs/logo.svg" alt="asterinas-logo" width="620"><br>
    <br/>
    <a href="https://github.com/uuhan/workhorse/actions/workflows/ci.yml"><img src="https://github.com/uuhan/workhorse/actions/workflows/ci.yml/badge.svg?event=push" alt="CI" style="max-width: 100%;"></a>
    <a href="https://github.com/uuhan/workhorse/actions/workflows/release.yml"><img src="https://github.com/uuhan/workhorse/actions/workflows/release.yml/badge.svg?event=release" alt="Release" style="max-width: 100%;"></a>
    <br/>
</p>

[中文](README.md)

### Workhorse

A ci tool designed & used by geeks, with core capabilities in local development and remote builds.

#### Slogan

Just focus on writing code, and leave the compilation to [牛马](https://github.com/uuhan/workhorse/)

### v0.3.0: Added `./skills` Workflow Support

Starting from `0.3.0`, this repository ships with a built-in `./skills` directory. It organizes common `cargo-work` and `horsed` workflows into reusable skill entry points for both team collaboration and AI-assisted execution.

Recommended quick start:

1. Start with `skills/workhorse/SKILL.md` to route requests into either the client-side or server-side path.
2. For `cargo work`, remote builds, SSH forwarding, and artifact sync, use `skills/workhorse-cargo-work/SKILL.md` (then drill down into its sub-skills when needed).
3. For `horsed` setup, service operations, and server-side development, use `skills/workhorse-horsed/SKILL.md` (then drill down into setup/ops/dev sub-skills when needed).

If you add or rename any skill, update the Project Skills section in `AGENTS.md` in the same change so docs and directory structure stay aligned.

### AI Agent Entry (Claude Code / Codex)

- Unified entry: `AI_AGENT.md`
- Machine-readable skill index: `skills/index.json`
- Standard task playbooks: `docs/agent-playbooks.md`

### Supported Platforms

- Linux
- MacOS
- Windows

### Instalation

You can download the binary file from the [release page](https://github.com/uuhan/workhorse/releases)

Or, you can build it from source code:

```bash
cargo install --git https://github.com/uuhan/workhorse.git horsed
cargo install --git https://github.com/uuhan/workhorse.git cargo-work
```

### How to Use It?

#### Horsed - The Server Side

You just run the horsed command in your terminal, and it will start a server
listening on port 2222.

```bash
# In a clean directory which will store all the files.
horsed
# then two files will generated in the current directory
# horsed.db3 - the database file
# horsed.log - the log file
```

In the first time, horsed will start a **SETUP SERVER** that will record the first ssh public keys,
you should connect to port 2223 to be the **FIRST USER** of the server.

```bash
ssh -p 2223 <YOUR NAME>@<THE HORSED SERVER>
# e.g. ssh -p 2223 uuhan@127.0.0.1
```

Then the **SETUP SERVER** will quit, and you can start to use the horsed server,
and there will be a file **horsed.key** in the current directory, which is the private key of the server.

Now the horsed server is ready to accept the connections from the clients.

##### DANGEROUS MODE

horsed supports a DANGEROUS MODE, which means the server will accept any ssh public keys,

**ANY** client connect to port 2223 will record the public key, please use it with caution.

```bash
horsed -f --show-log --dangerous
```

Other options please refer to the `horsed --help` info.

#### The Client Side

Workhorse treats the usual <Action>@<The Horsed Server> as a remote action runner.

Currently the supported actions are:

- git: use as a remote git repository via ssh protocol
- cmd: run command in remote server
- exec: read a whole script from stdin and run it on the remote server, useful for AI agents, heredocs, JSON, and complex quoting
- cargo: run cargo command in remote server (build/check/clippy/test/run/doc, etc.)
- apply: accept git patch and apply it to the working tree
- just: run just command defined in _justfile_
- get: get the build artifact from remote server
- put: upload local files to the remote worktree
- scp: like scp, to copy files from remote server to local
- push/pull: push or pull code to/from the remote repository
- ping: check server connectivity
- health: inspect server health info (version/commit/os/shell/ulimit)
- logs: inspect server logs
- job: view remote jobs and attach to their output
- watch: watch file changes and auto-run commands
- admin: admin user/key management
- ssh: interactive shell, local(-L) and reverse(-R) port forward

Workhorse is designed to work with two kinds of clients:

##### 1. The Usual SSH Client Tool

You can use the usual ssh command to connect to the horsed server, and it will work as usual.
The ssh client should support `SetEnv` command to set the environment variables,
The minimal version of OpenSSH should be 8.7 (2021-08-20) or above.

```bash
# This will run `ls` command and pass back the output to the local terminal
ssh -p 2222 cmd@127.0.0.1 -- ls
# horsed.db3
# horsed.key
# horsed.log
```

##### 2. The `cargo-work` Client Tool

Currently workhorse client is a cargo subcommand, and is built for rust projects.
You can run almost any cargo command remotely, like:

```bash
# This will build your rust project remotely, cool :)
cargo work build --release
```

There is no more configure for your project, the only one thing is to add
a remote target named `horsed` in your current git repo:

```bash
git remote add horsed ssh://git@<THE HORSED SERVER>:2222/<YOUR NAME>/<YOUR REPO NAME>.git
# e.g. git remote add horsed ssh://git@127.0.0.1:2222/uuhan/workhorse.git
# It is recommended to add the horsed repo remote to your origin remote.
# Then every time you push to origin, it will also push to the horsed repo.
git remote set-url --add origin ssh://git@127.0.0.1:2222/uuhan/workhorse.git
```

Then you can run any cargo command remotely:

```bash
git push horsed
cargo work build
# a lot of cargo output...
```

After build, you can get the build artifact from the horsed server:

```bash
# Get a file from horsed server
cargo work get target/debug/your-build-artifcat
# The file will show in the current directory with the path:
# target/debug/your-build-artifcat
```

You can also get the whole directory:

```bash
# Get a directory from horsed server
cargo work get target
# The directory will show in the current directory with the path:
# target.tar
```

You can also upload local files into the remote worktree:

```bash
# Upload a local file to the remote repository work directory
cargo work put ./target/release/horsed target/release/horsed
```

You can also run any command remotely:

```bash
# Run a command in horsed server, -- takes all the left args as command
cargo work -- scoop install vcpkg
```

For AI-generated multi-line scripts, or commands containing JSON, HTTP headers, parentheses, or quotes, prefer `cargo work exec` and pass the whole script on stdin to avoid multi-layer shell escaping:

```bash
cargo work exec <<'EOF'
set -euo pipefail
printf '%s\n' '{"name":"demo app","ok":true}'
pnpm --version
EOF
```

`exec` transports the script with base64 and runs it with `bash` on the server. It is best suited for Linux/macOS bash environments; if the server does not have `bash` or `base64 -d`, continue to use `cargo work -- ...` or explicit server-side command paths.

The default intepreter is `powershell.exe` on Windows, and `bash` on Linux/MacOS.
You can also specify the interpreter by `--shell` option:

```bash
# use nushell as the interpreter
cargo work --shell nu -- ls
# or use the env variabel `HORSED_SHELL` to set the interpreter
export HORSED_SHELL=nu
cargo work ls
```

For bash/zsh, `cmd` remote commands now start the shell as a login shell (`-lc`), so PATH configured in login profiles works as expected for tools such as nvm, pnpm, fnm, and cargo shims. It does not load interactive shell files such as `.zshrc` by default, avoiding prompts, completion plugins, or TTY assumptions in script execution. Other shells such as `sh`, `dash`, and `nu` still use plain `-c`.

You can pass the horsed target apparently to any cargo command:

```bash
cargo work --repo ssh://git@127.0.0.1:2222/uuhan/workhorse.git -- pacman install zig
cargo work build --repo ssh://git@127.0.0.1:2222/uuhan/workhorse.git --release
```

You can also add more git remotes:

```bash
git remote add horsed-win http://git@127.0.0.1:2222/uuhan/workhorse.git
git remote add horsed-linux http://git@127.0.0.1:2222/uuhan/workhorse.git
git remote add horsed-macos http://git@127.0.0.1:2222/uuhan/workhorse.git

# Then pass the remote by `--remote` option:
cargo work build --remote horsed-win
cargo work build --remote horsed-linux
cargo work build --remote horsed-macos
```

cargo work provides a simple ssh connection feature:

```bash
# This will start a tty on the server and launch the interactive shell provided by the user
cargo work ssh bash
# If no shell is provided, it will default to bash (on Unix) or powershell (on Windows)
cargo work ssh
```

You can perform both forward and reverse port forwarding:

```bash
# Forward local port 3000 to the remote machine, all local requests will go to the server
cargo work ssh -L 3000:127.0.0.1:3000
# Or use the standard ssh tool, ensuring compatibility with the ssh protocol
ssh -L 3000:127.0.0.1:3000
```

```bash
# Reverse forward server port 3000 to local, all requests to the server will come to the local machine
cargo work ssh -R 3000:127.0.0.1:3000
all_proxy=socks5://127.0.0.1:7890 cargo work -x -- curl -v https://google.com
# Or use the standard ssh tool, ensuring compatibility with the ssh protocol
ssh -R 3000:127.0.0.1:3000
```

At the same time, the cargo work command also supports reverse HTTP proxies, which can be useful in certain cases:

```bash
# -x, --enable-proxy enables a reverse proxy with a random port on the `horsed` side,
# which connects to the proxy specified by your current ALL_PROXY. This proxy will be used during command execution.
cargo work build -x
all_proxy=socks5://127.0.0.1:1080 cargo work -x -- curl -v https://google.com
# You can also manually specify the proxy address.
cargo work build --all-proxy=socks5://127.0.0.1:1234
```

You can view the `horsed` server logs by running:

```bash
cargo work logs
# the following command will keep the log output updated in real-time
cargo work logs -f
```

You can inspect remote server health:

```bash
cargo work health
# If nothing is printed due to log level, use:
RUST_LOG=info cargo work health
# For trace-stage diagnostics:
RUST_LOG=info WH_DEBUG=1 cargo work health
# Machine-readable output (recommended for AI agents):
cargo work health --json
```

Admins can manage users and public keys with the `admin` subcommand:

```bash
# User management
cargo work admin users list
cargo work admin users add <name> [admin|user]
cargo work admin users enable <name>
cargo work admin users disable <name>
cargo work admin users role <name> <admin|user>
cargo work admin users delete <name>

# Public key management
cargo work admin keys list [user]
cargo work admin keys add <user> <alg> <key> [comment]
cargo work admin keys enable <alg> <key>
cargo work admin keys disable <alg> <key>
cargo work admin keys delete <alg> <key>
```

### Frontend/Backend Update Workflow (Recommended)

#### Linux / macOS Server

```bash
# 1) Update local client binary
just install-work

# 2) Sync code and build on remote
git push horsed main
HORSED_SHELL=/bin/bash cargo work just install-horsed

# 3) Restart remote horsed service
HORSED_SHELL=/bin/bash cargo work -- systemctl --user restart horsed
```

#### Windows Server

```bash
# 1) Update local client binary
just install-work

# 2) One-step remote deploy (build + auto-restart)
git push horsed main
HORSED_SHELL=powershell.exe cargo work just deploy-horsed
```

`deploy-horsed` runs `cargo build --release` on the remote, then uses a delayed background script to automatically stop the old process, copy the new binary, and start it — no manual steps required.

#### Notes

- If the server does not have `nu`, do not use `HORSED_SHELL=nu`; use `/bin/bash` or `/bin/sh` (Linux), or `powershell.exe` (Windows).
- Builds are based on the server-side repo branch. Before release, make sure the `horsed` remote is in sync (`git push horsed main`).

More help info can be found by running:

```bash
cargo work --help
cargo work <SUBCOMMAND> --help
```
