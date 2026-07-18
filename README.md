<p align="center">
  <img src="assets/logo.svg" alt="ConfAI — one editor for every AI agent's config" width="720">
</p>

<p align="center">
  <a href="https://github.com/redstone-md/ConfAI/actions/workflows/ci.yml"><img src="https://github.com/redstone-md/ConfAI/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/rust-1.88%2B-b8352d" alt="Rust 1.88+">
  <img src="https://img.shields.io/badge/licence-MIT-b8352d" alt="MIT">
</p>

<p align="center">
  <a href="https://redstone.md">redstone.md</a> ·
  <a href="https://github.com/redstone-md/ConfAI">source</a> ·
  <a href="CONTRIBUTING.md">contributing</a> ·
  <a href="LICENSE">MIT</a>
</p>

<p align="center">
  <b>English</b> ·
  <a href="docs/README.ru.md">Русский</a> ·
  <a href="docs/README.zh-CN.md">简体中文</a> ·
  <a href="docs/README.es.md">Español</a> ·
  <a href="docs/README.de.md">Deutsch</a> ·
  <a href="docs/README.ja.md">日本語</a>
</p>

---

Codex, Claude Code and opencode each keep their endpoints in a different file, in
a different format, with a different name for the same idea. Adding a provider or
switching between two of them means opening three files by hand. ConfAI does it
in one command, and never reformats what it did not change.

## Install

Linux and macOS:

```sh
curl -fsSL https://github.com/redstone-md/ConfAI/releases/latest/download/install.sh | sh
```

Windows:

```powershell
irm https://github.com/redstone-md/ConfAI/releases/latest/download/install.ps1 | iex
```

Both scripts work out your platform, download the matching release archive and
`SHA256SUMS`, verify the checksum, and only then put the binary in place. Piping
a script from the internet into a shell is a trust decision;
[INSTALL.md](INSTALL.md) shows how to read it first.

Through cargo, once the crate is published to crates.io — as of v0.0.1 it is not,
so these two do not resolve yet:

```sh
cargo install confai --locked    # builds from source, needs Rust 1.88+
cargo binstall confai            # fetches the release archive instead
```

Or by hand: take an archive for your target from the
[releases page](https://github.com/redstone-md/ConfAI/releases/latest), check it
against the `SHA256SUMS` published alongside it, and put the binary on your
`PATH`.

[INSTALL.md](INSTALL.md) has the rest — every target, the installer flags, how
PATH is handled, and how to uninstall.

## What it does

```
$ confai list
agent        detected         providers  active  model          config
Codex        binary + config  3          primary gpt-5.6-terra  ~/.codex/config.toml
Claude Code  binary + config  1          byesu   opus[1m]       ~/.claude/settings.json
opencode     binary + config  11         vendor              ~/.config/opencode/opencode.json
```

- One command switches every agent that has an endpoint: `confai provider use primary`.
- One preset writes the same endpoint into all of them: `confai preset apply byesu --all --use`.
- `confai provider sync` fills in the model list an endpoint actually serves,
  with its context and output limits.
- Comments, key order and unknown keys survive an edit. Every write is backed up,
  and `confai undo` puts it back.

Running `confai` with no arguments opens a two-pane browser: agents on the left,
that agent's endpoints on the right.

<p align="center">
  <img src="assets/screenshots/tui.png" alt="The ConfAI interactive view: agents on the left, endpoints on the right" width="900">
</p>

<details>
<summary><b>The command line</b> — every subcommand and flag</summary>

```sh
confai                                    # interactive view
confai list                               # what is installed, and where
confai provider list --check              # every endpoint, and whether it answers
confai provider add byesu \
    --agent codex \
    --base-url https://byesu.com/v1 \
    --api-key "$BYESU_API_KEY" \
    --wire-api chat --use
confai provider use primary               # switch every agent that has it
confai provider sync vendor --prune       # pull the model list from the endpoint
confai preset apply byesu --all --use     # one endpoint, every agent
confai doctor                             # does everything still parse and resolve
confai undo                               # put back what was there before
```

`--agent` targets one agent, `--all` targets every installed one. Without either,
read commands cover everything and write commands ask you to pick.

| Command | |
|---|---|
| `list` | which agents are installed and what they are pointed at |
| `provider list` | endpoints across the selected agents; `--check` calls each one |
| `provider add <id>` | add an endpoint, or edit the fields you pass on an existing one |
| `provider remove <id>` | remove an endpoint |
| `provider use <id>` | route an agent through one of its endpoints |
| `provider check [id]` | ask endpoints whether they are alive and what they serve |
| `provider models [id]` | what an endpoint serves, with limits and prices |
| `provider sync <id>` | pull the model list into the config |
| `preset list` · `preset show <id>` | what recipes exist, and what one would write |
| `preset apply <id>` | write a preset's endpoint into the selected agents |
| `model [model]` | show or set the model an agent uses |
| `path` · `edit` | print an agent's config path · open it in `$EDITOR` |
| `doctor` | check every config parses and every referenced provider resolves |
| `about` · `update` | version and state locations · whether a newer release exists |
| `undo` | restore the config backed up before the last write |

`provider add` takes `--base-url`, `--api-key`, `--wire-api` (`chat`, `responses`
or `anthropic`), `--name`, a repeatable `--set KEY=VALUE` for backend-specific
keys, and `--use` / `--sync` to select the endpoint and pull its models after
writing. `provider check` takes `--timeout` in seconds, 10 by default.
`provider models` takes `--select <model>` and `--refresh`. `provider sync` takes
`--prune`, `--dry-run` and `--refresh`. `preset apply` takes `--api-key`, `--use`
and `--sync`.

`list`, `doctor`, `about`, `update` and `preset list` take no agent selector —
they always cover everything.

</details>

<details>
<summary><b>The interactive view</b> — the command palette, the detail pane, the full key map</summary>

The command palette on `Ctrl+P` lists every action with the key that runs it, so
the shortcuts are learned by using it rather than by reading this page:

<p align="center">
  <img src="assets/screenshots/palette.png" alt="The command palette, listing every action with its key binding" width="900">
</p>

`Enter` on an endpoint shows everything recorded about it, including the model
list with its context and output limits:

<p align="center">
  <img src="assets/screenshots/detail.png" alt="The provider detail view, showing an endpoint's fields and model list" width="900">
</p>

| key | |
|---|---|
| `Ctrl+P` / `Ctrl+K` | command palette — every action, searchable |
| `↑` `↓` / `k` `j` | move · `Tab` `←` `→` switch pane |
| `Enter` | endpoint detail, including its model list |
| `/` or `Ctrl+F` | filter endpoints by id, host or model |
| `u` | route this agent through the selected endpoint |
| `m` | choose which model this agent uses |
| `a` `e` `d` | add · edit · delete |
| `c` / `C` | health-check this endpoint · all of them |
| `s` / `S` | sync models · sync and prune stale ones |
| `p` | apply a preset |
| `?` | about, and the full key map |
| `r` `q` | reload from disk · quit |

The mouse works: click to select, click again to open, wheel to scroll, click a
hint to run it.

Keys are matched by physical position, so they keep working on a non-Latin
layout — `й` is `q`, `Ы` is `S`. `/` has no equivalent position on a Cyrillic
layout, which is why `Ctrl+F` opens the filter too.

Edits go through the same load-edit-save path as the CLI, so the same guarantees
about your files hold.

</details>

<details>
<summary><b>What it will not do to your files</b> — comments, key order, backups</summary>

Configs are hand-written, and hand-written files have things in them that a
naive round trip destroys.

- **Comments survive.** Codex configs are edited through `toml_edit`, so a
  spare endpoint parked on a commented-out `base_url` is still there afterwards.
- **Only what you changed changes.** Key order, indentation and unknown keys are
  left alone, because every backend edits the parsed document in place instead of
  re-serialising its own idea of the file.
- **JSON with comments is refused, not mangled.** ConfAI would have to drop them,
  so it stops and says so.
- **Every write is backed up** next to the original as `<name>.confai.bak`, and
  replaces the file atomically. `confai undo` restores it.

</details>

<details>
<summary><b>Models and health</b> — where the model list comes from, and pruning</summary>

opencode will not offer a model it has not been told about, and it wants the
context limit spelled out. `confai provider sync <id>` calls the endpoint's
`/v1/models`, looks each id up on [models.dev](https://models.dev) for its
context and output limits, and writes the result — leaving `variants` and
anything else you had configured untouched. The catalogue is cached for a day;
`--refresh` re-downloads it.

Syncing is a merge, so a model the gateway has since retired stays in your config
until you say otherwise. `--prune` drops the ones the endpoint no longer serves,
and moves your model selection to a surviving one if it pruned the selected model:

```sh
confai provider sync vendor --prune --dry-run   # see what would go
confai provider sync vendor --prune
```

In the interactive view, `s` syncs and `S` syncs with pruning.

`confai provider models <id>` lists what an endpoint serves without writing
anything, and `--select` makes one of them the agent's model. This works for
Codex and Claude Code too, which record a model but no model list.

`confai provider check` is the same call without the writing: it reports whether
each endpoint is up, how fast it answered, and how many models it serves.

</details>

<details>
<summary><b>Presets</b> — one endpoint recipe, any agent</summary>

A preset is one endpoint described once, in agent-neutral terms, so the same
recipe applies to any agent:

```sh
confai preset list
confai preset show byesu
confai preset apply byesu --all --api-key sk-... --use --sync
```

Twenty-six built-in presets live in [`presets/`](presets/) — one TOML file each,
baked into the binary at build time — covering OpenCode Zen, OpenRouter, OpenAI,
Anthropic, Groq, xAI, Mistral, Cerebras, Together, DeepSeek, DeepInfra,
Fireworks, Moonshot, Z.ai, Chutes, Baseten, Vercel AI Gateway, Venice, Novita,
Byesu, Ollama and LM Studio. Adding one is a pull request that touches a single
new file. Your own presets go in `~/.confai/presets/`, and override a built-in
with the same id.

</details>

<details>
<summary><b>Agents</b> — the three config layouts, and what ConfAI does about each</summary>

| Agent | Config | Keys | Named providers | Model list | Switching |
|---|---|---|---|---|---|
| Codex | `~/.codex/config.toml` | same file | yes | no | `model_provider` |
| Claude Code | `~/.claude/settings.json` | `env` block | via ConfAI | no | `ANTHROPIC_*` |
| opencode | `~/.config/opencode/opencode.json` | `~/.local/share/opencode/auth.json` | yes | yes | `provider/model` |

`CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG` and `XDG_CONFIG_HOME` are
honoured, the same way the agents themselves honour them.

Claude Code points at one endpoint at a time, through environment variables in
its settings, and has nowhere to keep the endpoints you are not using. ConfAI
keeps that roster in `~/.confai/agents/claude.json` and writes only the selected
entry into the file Claude Code owns.

opencode is split over two files: providers in `opencode.json`, keys in
`~/.local/share/opencode/auth.json`, where `opencode auth login` puts them. ConfAI
reads both, so a health check goes out with the credential opencode would really
use rather than reporting a false 401. A new key is written to `auth.json`; a key
already sitting inline in `opencode.json` is updated where it is, because quietly
moving a secret from one file to another is its own kind of surprise. An OAuth
session in `auth.json` is shown but never overwritten — ConfAI tells you to run
`opencode auth logout` rather than silently ending it.

Adding an agent means one file in `src/agent/` implementing `Agent` and
`AgentConfig`; nothing above that layer knows which agent it is talking to.

</details>

<details>
<summary><b>Staying current</b> — the update check, and how to turn it off</summary>

`confai update` reports whether a newer release exists, summarises what changed
and prints how to upgrade.

Day to day you do not have to ask. After a command, ConfAI prints a two-line
notice on stderr if a newer release is out:

```
◆ 0.0.1 → 0.0.2 available
  · provider sync now prunes retired models
  · run `confai update` for the rest
```

That notice is rendered from a cache checked at most once a day, so a normal run
costs nothing — and when the cache is stale the check gets four hundred
milliseconds to answer before the run gives up and tries again tomorrow. A failed
check backs off for an hour rather than retrying on every invocation. Set
`CONFAI_NO_UPDATE_CHECK` to turn it off entirely.

ConfAI does not replace its own binary. `cargo` and the installers already do
that properly, and a tool that rewrites itself while holding your credentials
open is a worse trade than printing one line.

</details>

<details>
<summary><b>Contributing</b> — adding a preset or an agent</summary>

A preset is one new file in `presets/`. A new agent is one new file in
`src/agent/` implementing `Agent` and `AgentConfig` — the layers above it stay
untouched. Run `cargo test` and `cargo clippy --lib --bins --tests` before
opening a pull request. See [CONTRIBUTING.md](CONTRIBUTING.md).

</details>

## Licence

[MIT](LICENSE) © [redstone.md](https://redstone.md)
