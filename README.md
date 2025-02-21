<p align="center">
    <img src="docs/logo.svg" alt="asterinas-logo" width="620"><br>
    <br/>
    <a href="https://github.com/uuhan/workhorse/actions/workflows/ci.yml"><img src="https://github.com/uuhan/workhorse/actions/workflows/ci.yml/badge.svg?event=push" alt="CI" style="max-width: 100%;"></a>
    <a href="https://github.com/uuhan/workhorse/actions/workflows/release.yml"><img src="https://github.com/uuhan/workhorse/actions/workflows/release.yml/badge.svg?event=release" alt="Release" style="max-width: 100%;"></a>
    <br/>
</p>

[English](README.en.md)

### 牛马 (Workhorse)

workhorse [ˈwɜrkhɔrs]

n. 驮马，做粗工者，重负荷机器

一款为极客设计和使用的持续集成工具，核心能力包括本地开发和远程构建。

#### 口号

你就安心写代码，编译的事情交给 [牛马](https://github.com/uuhan/workhorse/)

### 支持的平台

- Linux
- MacOS
- Windows

### 安装

你可以从 [发布页面](https://github.com/uuhan/workhorse/releases) 下载二进制文件。

或者，你可以使用 `cargo` 安装：

```bash
cargo install --git https://github.com/uuhan/workhorse.git horsed
cargo install --git https://github.com/uuhan/workhorse.git cargo-work
```

### 如何使用？

#### Horsed - 服务器端

在终端中运行 `horsed` 命令，它会启动一个监听在 2222 端口的服务器。

```bash
# 在一个干净的目录中，存储所有文件。
horsed
# 然后会在当前目录生成两个文件：
# horsed.db3 - 数据库文件
# horsed.log - 日志文件
```

第一次运行时，`horsed` 会启动一个 **SETUP SERVER**，并记录第一次使用的 SSH 公钥。你需要连接到 2223 端口成为服务器的 **第一个用户**。

```bash
ssh -p 2223 <YOUR NAME>@<THE HORSED SERVER>
# 例如： ssh -p 2223 uuhan@127.0.0.1
```

之后，**SETUP SERVER** 会退出，你就可以开始使用 `horsed` 服务器了，当前目录下会生成一个 **horsed.key** 文件，它是服务器的私钥。

现在，`horsed` 服务器已经准备好接收来自客户端的连接。

##### 危险模式!

horsed 支持参数 `--dangerous`, 目前只能在前台模式启用, 启用之后维护服务会常驻,

**任意** 连接到 2223 端口的客户端都能录入他的公钥信息, 请小心使用!

```bash
horsed -f --show-log --dangerous
```

其他参数请参考 `horsed --help` 命令。

#### 客户端

Workhorse 将普通的 `<Action>@<The Horsed Server>` 视为远程操作执行器。

当前支持的操作有：

- git：通过 SSH 协议作为远程 git 仓库使用
- cmd：在远程服务器上执行命令
- cargo：在远程服务器上执行 cargo 命令
- apply：接受 git 补丁并应用到工作树
- just：运行 _justfile_ 中定义的 just 命令
- get：从远程服务器获取构建产物
- scp：类似 scp，将文件从远程服务器复制到本地
- ssh: 本地(-L)、反向(-R)端口转发

Workhorse 设计支持两种客户端：

##### 1. 普通的 SSH 客户端工具

你可以使用常规的 `ssh` 命令连接到 `horsed` 服务器，它将像平常一样工作。SSH 客户端需要支持 `SetEnv` 命令来设置环境变量，OpenSSH 的最低版本应为 8.7（2021-08-20）或更高版本。

```bash
# 这将运行 `ls` 命令，并将输出返回到本地终端
ssh -p 2222 cmd@127.0.0.1 -- ls
# horsed.db3
# horsed.key
# horsed.log
```

##### 2. `cargo-work` 客户端工具

目前，Workhorse 客户端是一个 cargo 子命令，专为 Rust 项目构建。你可以远程运行几乎任何 cargo 命令，例如：

```bash
# 这将远程构建你的 Rust 项目，酷吧 :)
cargo work build --release
```

你无需对项目进行更多配置，唯一需要做的是在当前的 git 仓库中添加一个名为 `horsed` 的远程目标：

```bash
git remote add horsed ssh://git@<THE HORSED SERVER>:2222/<YOUR NAME>/<YOUR REPO NAME>.git
# 例如： git remote add horsed ssh://git@127.0.0.1:2222/uuhan/workhorse.git
# 推荐将 horsed 仓库远程添加到你的 origin 远程。
# 然后，每次你推送到 origin，它也会推送到 horsed 仓库。
git remote set-url --add origin ssh://git@127.0.0.1:2222/uuhan/workhorse.git
```

然后你可以远程运行任何 cargo 命令：

```bash
git push horsed
cargo work build
# 会有很多 cargo 输出...
```

构建完成后，你可以从 horsed 服务器获取构建产物：

```bash
# 从 horsed 服务器获取文件
cargo work get target/debug/your-build-artifcat
# 文件将显示在当前目录，路径为：
# target/debug/your-build-artifcat
```

你还可以获取整个目录：

```bash
# 从 horsed 服务器获取目录
cargo work get target
# 目录将显示在当前目录，路径为：
# target.tar
```

你也可以执行任意的远程命令:

```bash
# 运行远程命令, -- 后面的内容将作为命令执行
cargo work -- scoop install vcpkg
```

默认 Windows 系统使用 `powershell.exe`, 非 Windows 系统使用 `bash` 执行命令,
你也可以使用 `--shell` 来指定你喜欢的解释器:

```bash
# 使用 nushell 作为 shell
cargo work -s nu -- ls
# 也可以使用环境变量 `HORSED_SHELL` 指定 shell
export HORSED_SHELL=nu
cargo work ls
```

cargo-work 也支持显式传入机器地址:

```bash
cargo work --repo ssh://git@127.0.0.1:2222/uuhan/workhorse.git -- pacman install zig
cargo work build --repo ssh://git@127.0.0.1:2222/uuhan/workhorse.git --release
```

你也可以为 git 仓库配置多个 remote:

```bash
git remote add horsed-win http://git@127.0.0.1:2222/uuhan/workhorse.git
git remote add horsed-linux http://git@127.0.0.1:2222/uuhan/workhorse.git
git remote add horsed-macos http://git@127.0.0.1:2222/uuhan/workhorse.git

# 然后通过传递 --remote 来指定远程仓库
cargo work build --remote horsed-win
cargo work build --remote horsed-linux
cargo work build --remote horsed-macos
```

你可以进行正向, 反向端口转发:

```bash
# 正向转发本地 3000 端口到远程机器，所有本地请求会去往服务器
cargo work ssh -L 3000:127.0.0.1:3000
# 或者使用标准的 ssh 工具, 实现上保证 ssh 协议兼容性
ssh -L 3000:127.0.0.1:3000
```

```bash
# 反向转发服务器 3000 端口到本地，所有对服务器的请求会来到本地
cargo work ssh -R 3000:127.0.0.1:3000
# 或者使用标准的 ssh 工具，实现上保证 ssh 协议兼容性
ssh -R 3000:127.0.0.1:3000
```

同时 `cargo work` 指令也支持反向 HTTP 代理, 这在有时候会比较有用:

```bash
# -x, --enable-proxy 会在 `horsed` 端启用一个随机端口的反向代理,
# 连接到你当前的 ALL_PROXY 的代理, 在执行命令的时候会使用这个代理
cargo work build -x
all_proxy=socks5://127.0.0.1:1080 cargo work -x -- curl -v https://google.com
# 你也可以手动指定代理地址
cargo work build --all-proxy=socks5://127.0.0.1:1080
```

更多的帮助信息可以通过查看帮助获取:

```bash
cargo work --help
cargo work <SUBCOMMAND> --help
```
