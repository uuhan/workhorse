### Workhorse

A ci tool designed & used by geeks, with core capabilities in local development and remote builds.

#### Slogan

Just focus on writing code, and leave the compilation to [牛马](https://github.com/uuhan/workhorse/)

### Supported Platforms

- Linux
- MacOS
- Windows

### How to Use It?

#### Horsed - The Server Side

You just run the horsed command in your terminal, and it will start a server
listening on port 2222.

You can download the binary file from the [release page](https://github.com/uuhan/workhorse/releases)

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
- cargo: run cargo command in remote server
- apply: accept git patch and apply it to the working tree
- just: run just command defined in _justfile_
- get: get the build artifact from remote server
- scp: like scp, to copy files from remote server to local

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

You can download the binary file from the [release page](https://github.com/uuhan/workhorse/releases)

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

You can also run any command remotely:

```bash
# Run a command in horsed server, -- takes all the left args as command
cargo work -- scoop install vcpkg
```

The default intepreter is `powershell.exe` on Windows, and `bash` on Linux/MacOS.
You can also specify the interpreter by `--shell` option:

```bash
# use nushell as the interpreter
cargo work --shell nu -- ls
# or use the env variabel `HORSED_SHELL` to set the interpreter
export HORSED_SHELL=nu
cargo work ls
```

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

More help info can be found by running:

```bash
cargo work --help
cargo work <SUBCOMMAND> --help
```
