<p align="center">
  <img src="../assets/logo.svg" alt="ConfAI — 一个编辑器管理所有 AI 智能体的配置" width="720">
</p>

<p align="center">
  <a href="https://github.com/redstone-md/ConfAI/actions/workflows/ci.yml"><img src="https://github.com/redstone-md/ConfAI/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/rust-1.88%2B-b8352d" alt="Rust 1.88+">
  <img src="https://img.shields.io/badge/licence-MIT-b8352d" alt="MIT">
</p>

<p align="center">
  <a href="https://redstone.md">redstone.md</a> ·
  <a href="https://github.com/redstone-md/ConfAI">源码</a> ·
  <a href="../CONTRIBUTING.md">参与贡献</a> ·
  <a href="../LICENSE">MIT</a>
</p>

<p align="center">
  <a href="../README.md">English</a> ·
  <a href="README.ru.md">Русский</a> ·
  <b>简体中文</b> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.ja.md">日本語</a>
</p>

---

Codex、Claude Code 和 opencode 各自把端点存在不同的文件里，格式不同，同一个概念还
用不同的名字。加一个供应商，或者在两个之间切换，就得手动打开三个文件。ConfAI 用一
条命令搞定，并且绝不重新格式化它没有改动的部分。

## 安装

Linux 和 macOS：

```sh
curl -fsSL https://github.com/redstone-md/ConfAI/releases/latest/download/install.sh | sh
```

Windows：

```powershell
irm https://github.com/redstone-md/ConfAI/releases/latest/download/install.ps1 | iex
```

两个脚本都会判断平台，下载对应的发布压缩包和 `SHA256SUMS`，校验校验和，确认无误后
才把二进制文件放到位。把网上的脚本直接管道给 shell 是一个信任问题，需要你自己判
断；[INSTALL.md](../INSTALL.md) 说明了如何先读一遍再运行。

通过 cargo 安装，前提是 crate 已发布到 crates.io —— 截至 v0.0.1 尚未发布，所以下面
两条命令目前还无法使用：

```sh
cargo install confai --locked    # 从源码构建，需要 Rust 1.88+
cargo binstall confai            # 直接下载发布压缩包，不编译
```

也可以手动安装：从
[发布页面](https://github.com/redstone-md/ConfAI/releases/latest)取回对应平台的压
缩包，用同时发布的 `SHA256SUMS` 校验，然后把二进制文件放进 `PATH`。

其余内容都在 [INSTALL.md](../INSTALL.md) 里：全部目标平台、安装脚本的参数、PATH 的
处理方式，以及如何卸载。

## 它做什么

```
$ confai list
agent        detected         providers  active  model          config
Codex        binary + config  3          primary gpt-5.6-terra  ~/.codex/config.toml
Claude Code  binary + config  1          byesu   opus[1m]       ~/.claude/settings.json
opencode     binary + config  11         vendor              ~/.config/opencode/opencode.json
```

- 一条命令切换所有拥有该端点的智能体：`confai provider use primary`。
- 一个预设把同一个端点写进所有智能体：`confai preset apply byesu --all --use`。
- `confai provider sync` 拉取端点实际提供的模型列表，连同上下文和输出上限一起写入。
- 注释、键的顺序和未知的键都会在编辑后保留。每次写入前都会备份，`confai undo` 可以
  还原。

不带参数运行 `confai` 会打开一个双栏浏览界面：左边是智能体，右边是该智能体的端点。

<p align="center">
  <img src="../assets/screenshots/tui.png" alt="ConfAI 交互界面：左侧为智能体，右侧为端点" width="900">
</p>

<details>
<summary><b>命令行</b> —— 全部子命令和参数</summary>

```sh
confai                                    # 交互界面
confai list                               # 装了什么，配置在哪
confai provider list --check              # 所有端点，以及是否响应
confai provider add byesu \
    --agent codex \
    --base-url https://byesu.com/v1 \
    --api-key "$BYESU_API_KEY" \
    --wire-api chat --use
confai provider use primary               # 切换所有拥有它的智能体
confai provider sync vendor --prune       # 从端点拉取模型列表
confai preset apply byesu --all --use     # 一个端点，所有智能体
confai doctor                             # 是否仍然都能解析并解析得通
confai undo                               # 还原成之前的样子
```

`--agent` 只作用于一个智能体，`--all` 作用于每个已安装的。两者都不给时，读取类命令
覆盖全部，写入类命令会让你选。

| 命令 | |
|---|---|
| `list` | 装了哪些智能体，它们分别指向什么 |
| `provider list` | 所选智能体的端点；`--check` 会逐个请求 |
| `provider add <id>` | 新增端点，或修改已有端点中你传入的字段 |
| `provider remove <id>` | 删除端点 |
| `provider use <id>` | 让智能体走它的某个端点 |
| `provider check [id]` | 询问端点是否存活、提供什么 |
| `provider models [id]` | 端点提供的模型，含上限与价格 |
| `provider sync <id>` | 把模型列表写进配置 |
| `preset list` · `preset show <id>` | 有哪些预设，以及某个预设会写入什么 |
| `preset apply <id>` | 把预设的端点写进所选智能体 |
| `model [model]` | 查看或设置智能体使用的模型 |
| `path` · `edit` | 打印智能体的配置路径 · 用 `$EDITOR` 打开 |
| `doctor` | 检查每个配置都能解析、引用的每个供应商都能解析得通 |
| `about` · `update` | 版本与状态目录 · 是否有更新的版本 |
| `undo` | 还原上次写入前备份的配置 |

`provider add` 接受 `--base-url`、`--api-key`、`--wire-api`（`chat`、`responses`
或 `anthropic`）、`--name`，可重复的 `--set KEY=VALUE` 用于后端特有的键，以及
`--use` / `--sync`，在写入后选中该端点并拉取模型。`provider check` 有 `--timeout`
（秒，默认 10）。`provider models` 有 `--select <model>` 和 `--refresh`。
`provider sync` 有 `--prune`、`--dry-run` 和 `--refresh`。`preset apply` 有
`--api-key`、`--use` 和 `--sync`。

`list`、`doctor`、`about`、`update` 和 `preset list` 不接受智能体选择参数 —— 它们
始终覆盖全部。

</details>

<details>
<summary><b>交互界面</b> —— 命令面板、详情视图、完整按键表</summary>

`Ctrl+P` 打开的命令面板列出每个动作及其对应按键，所以快捷键是在使用中记住的，而不
是靠读这一页：

<p align="center">
  <img src="../assets/screenshots/palette.png" alt="命令面板，列出每个动作及其按键" width="900">
</p>

在端点上按 `Enter` 会显示关于它的全部记录，包括带上下文和输出上限的模型列表：

<p align="center">
  <img src="../assets/screenshots/detail.png" alt="供应商详情视图，显示端点的字段和模型列表" width="900">
</p>

| 按键 | |
|---|---|
| `Ctrl+P` / `Ctrl+K` | 命令面板 —— 全部动作，可搜索 |
| `↑` `↓` / `k` `j` | 移动 · `Tab` `←` `→` 切换栏 |
| `Enter` | 端点详情，含模型列表 |
| `/` 或 `Ctrl+F` | 按 id、主机或模型过滤端点 |
| `u` | 让当前智能体走选中的端点 |
| `m` | 选择该智能体使用的模型 |
| `a` `e` `d` | 新增 · 编辑 · 删除 |
| `c` / `C` | 检查此端点 · 检查全部 |
| `s` / `S` | 同步模型 · 同步并清理已下线的 |
| `p` | 应用预设 |
| `?` | 关于，以及完整按键表 |
| `r` `q` | 从磁盘重新加载 · 退出 |

鼠标可用：单击选中，再次单击打开，滚轮滚动，点击提示条即可执行。

按键按物理位置匹配，因此在非拉丁键盘布局下依然有效 —— `й` 就是 `q`，`Ы` 就是
`S`。`/` 在西里尔布局上没有对应位置，所以 `Ctrl+F` 同样能打开过滤。

编辑走的是和 CLI 相同的「加载—修改—保存」路径，因此对文件的各项保证同样成立。

</details>

<details>
<summary><b>它不会对你的文件做什么</b> —— 注释、键顺序、备份</summary>

配置是手写的，而手写的文件里有些东西经不起一次粗糙的读写往返。

- **注释会保留。** Codex 的配置通过 `toml_edit` 编辑，所以停放在被注释掉的
  `base_url` 上的备用端点，改完之后仍然在那里。
- **只有你改的部分会变。** 键的顺序、缩进和未知的键都不动，因为每个后端都是就地
  编辑已解析的文档，而不是按自己的理解重新序列化整个文件。
- **带注释的 JSON 会被拒绝，而不是被弄坏。** ConfAI 只能丢掉这些注释，所以它选择
  停下来并告诉你。
- **每次写入前都会备份**，与原文件放在一起，命名为 `<name>.confai.bak`，并以原子
  方式替换原文件。`confai undo` 可以还原。

</details>

<details>
<summary><b>模型与健康检查</b> —— 模型列表从哪来，以及 --prune 的作用</summary>

opencode 不会提供它没被告知过的模型，而且要求明确写出上下文上限。
`confai provider sync <id>` 请求端点的 `/v1/models`，在
[models.dev](https://models.dev) 上逐个查出上下文和输出上限，然后写入结果 —— 不触
碰 `variants` 以及你配置过的其他内容。目录会缓存一天；`--refresh` 会重新下载。

同步是合并，所以网关已经下线的模型会留在你的配置里，直到你另行处理。`--prune` 删
掉端点不再提供的那些；如果被删的正是当前选中的模型，它会把选择移到一个仍然存在的
模型上：

```sh
confai provider sync vendor --prune --dry-run   # 看看会删掉什么
confai provider sync vendor --prune
```

在交互界面里，`s` 是同步，`S` 是同步并清理。

`confai provider models <id>` 只列出端点提供的模型而不写入任何东西，`--select` 则
把其中一个设为该智能体的模型。这对只记录模型、不记录模型列表的 Codex 和 Claude
Code 同样有效。

`confai provider check` 是同样的请求但不写入：它报告每个端点是否在线、响应有多快、
提供多少个模型。

</details>

<details>
<summary><b>预设</b> —— 一份端点配方，适用于任何智能体</summary>

预设就是把一个端点用与智能体无关的方式描述一次，于是同一份配方可以套用到任何智能
体上：

```sh
confai preset list
confai preset show byesu
confai preset apply byesu --all --api-key sk-... --use --sync
```

二十六个内置预设放在 [`presets/`](../presets/) 下 —— 每个一个 TOML 文件，构建时直
接编进二进制 —— 覆盖 OpenCode Zen、OpenRouter、OpenAI、Anthropic、Groq、xAI、
Mistral、Cerebras、Together、DeepSeek、DeepInfra、Fireworks、Moonshot、Z.ai、
Chutes、Baseten、Vercel AI Gateway、Venice、Novita、Byesu、Ollama 和 LM Studio。
新增一个只需一个 pull request，改动一个新文件。你自己的预设放在
`~/.confai/presets/`，同 id 会覆盖内置的那个。

</details>

<details>
<summary><b>智能体</b> —— 三种配置结构，以及 ConfAI 各自的处理方式</summary>

| 智能体 | 配置 | 密钥 | 具名供应商 | 模型列表 | 切换方式 |
|---|---|---|---|---|---|
| Codex | `~/.codex/config.toml` | 同一文件 | 有 | 无 | `model_provider` |
| Claude Code | `~/.claude/settings.json` | `env` 块 | 由 ConfAI 提供 | 无 | `ANTHROPIC_*` |
| opencode | `~/.config/opencode/opencode.json` | `~/.local/share/opencode/auth.json` | 有 | 有 | `provider/model` |

`CODEX_HOME`、`CLAUDE_CONFIG_DIR`、`OPENCODE_CONFIG` 和 `XDG_CONFIG_HOME` 都会被
遵循，方式与这些智能体自己的行为一致。

Claude Code 一次只指向一个端点，通过其设置里的环境变量实现，并且没有地方存放你当前
没在用的端点。ConfAI 把这份名册放在 `~/.confai/agents/claude.json`，只把选中的那条
写进 Claude Code 自己的文件。

opencode 分散在两个文件里：供应商在 `opencode.json`，密钥在
`~/.local/share/opencode/auth.json`，也就是 `opencode auth login` 写入的地方。
ConfAI 两个都读，所以健康检查用的是 opencode 实际会用的凭据，而不是报出一个假的
401。新密钥写进 `auth.json`；已经内联在 `opencode.json` 里的密钥就在原处更新，因为
悄悄把一个机密从一个文件挪到另一个文件本身就是一种意外。`auth.json` 里的 OAuth 会
话会被显示但绝不会被覆盖 —— ConfAI 会让你运行 `opencode auth logout`，而不是无声地
结束它。

新增一个智能体只需在 `src/agent/` 里加一个文件，实现 `Agent` 和 `AgentConfig`；这
一层之上的代码都不知道自己在和哪个智能体打交道。

</details>

<details>
<summary><b>保持更新</b> —— 更新检查如何工作，以及如何关掉</summary>

`confai update` 会报告是否存在更新的版本、概述改动内容，并打印升级方式。

日常不需要专门去问。命令执行完后，如果有更新的版本，ConfAI 会在 stderr 上打印两行
提示：

```
◆ 0.0.1 → 0.0.2 available
  · provider sync now prunes retired models
  · run `confai update` for the rest
```

这条提示由每天最多检查一次的缓存渲染而来，所以缓存命中时一次普通运行不会有任何开
销；缓存过期时，检查有四百毫秒的时间作答，超时就放弃，明天再试。检查失败后会退避
一小时，而不是每次调用都重试。设置 `CONFAI_NO_UPDATE_CHECK` 可以彻底关掉。

ConfAI 不会替换自己的二进制文件。`cargo` 和安装脚本已经把这件事做得很妥当，而一个
握着你的凭据、还要改写自身的工具，比多打印一行提示糟糕得多。

</details>

<details>
<summary><b>参与贡献</b> —— 添加预设或智能体</summary>

一个预设就是 `presets/` 里的一个新文件。一个新智能体就是 `src/agent/` 里的一个新文
件，实现 `Agent` 和 `AgentConfig` —— 它之上的各层都不用动。提交 pull request 前请
运行 `cargo test` 和 `cargo clippy --lib --bins --tests`。参见
[CONTRIBUTING.md](../CONTRIBUTING.md)。

</details>

## 许可证

[MIT](../LICENSE) © [redstone.md](https://redstone.md)
