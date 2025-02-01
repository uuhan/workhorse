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

### 如何使用？

#### Horsed - 服务器端

在终端中运行 `horsed` 命令，它会启动一个监听在 2222 端口的服务器。

你可以从 [发布页面](https://github.com/uuhan/workhorse/releases) 下载二进制文件。

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

你可以从 [发布页面](https://github.com/uuhan/workhorse/releases) 下载二进制文件。

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

cargo-work 也支持显式传入机器地址:

```bash
cargo work --repo ssh://git@127.0.0.1:2222/uuhan/workhorse.git -- pacman install zig
cargo work build --repo ssh://git@127.0.0.1:2222/uuhan/workhorse.git --release
```

更多的帮助信息可以通过查看帮助获取:

```bash
cargo work --help
cargo work <SUBCOMMAND> --help
```
