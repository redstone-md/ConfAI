<p align="center">
  <img src="assets/logo.svg" alt="ConfAI — one editor for every AI agent's config" width="640">
</p>

<p align="center">
  <a href="https://redstone.md">redstone.md</a> ·
  <a href="https://github.com/redstone-md/ConfAI">source</a> ·
  <a href="LICENSE">MIT</a>
</p>

---

Codex, Claude Code and opencode each keep their endpoints in a different file, in
a different format, with a different name for the same idea. Adding a provider or
switching between two of them means opening three files by hand. ConfAI does it
in one command, and never reformats what it did not change.

```
$ confai list
agent        detected         providers  active  model          config
Codex        binary + config  3          primary gpt-5.6-terra  ~/.codex/config.toml
Claude Code  binary + config  1          byesu   opus[1m]       ~/.claude/settings.json
opencode     binary + config  11         vendor              ~/.config/opencode/opencode.json
```

## Install

```sh
cargo install --path .
```

Or grab a binary from the releases page and drop it on your `PATH`.

## Use

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
confai provider sync vendor --prune    # pull the model list from the endpoint
confai preset apply byesu --all --use     # one endpoint, every agent
confai doctor                             # does everything still parse and resolve
confai undo                               # put back what was there before
```

`--agent` targets one agent, `--all` targets every installed one. Without either,
read commands cover everything and write commands ask you to pick.

### Interactive view

`confai` with no arguments opens a two-pane browser: agents on the left, that
agent's endpoints on the right.

| key | |
|---|---|
| `Ctrl+P` / `Ctrl+K` | command palette — every action, searchable |
| `↑` `↓` / `k` `j` | move · `Tab` `←` `→` switch pane |
| `Enter` | endpoint detail, including its model list |
| `/` or `Ctrl+F` | filter endpoints by id, host or model |
| `u` | route this agent through the selected endpoint |
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

## What it will not do to your files

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

## Models and health

opencode will not offer a model it has not been told about, and it wants the
context limit spelled out. `confai provider sync <id>` calls the endpoint's
`/v1/models`, looks each id up on [models.dev](https://models.dev) for its
context and output limits, and writes the result — leaving `variants` and
anything else you had configured untouched. The catalogue is cached for a day.

Syncing is a merge, so a model the gateway has since retired stays in your config
until you say otherwise. `--prune` drops the ones the endpoint no longer serves,
and moves your model selection to a surviving one if it pruned the selected model:

```sh
confai provider sync vendor --prune --dry-run   # see what would go
confai provider sync vendor --prune
```

In the interactive view, `s` syncs and `S` syncs with pruning.

`confai provider check` is the same call without the writing: it reports whether
each endpoint is up, how fast it answered, and how many models it serves.

## Presets

A preset is one endpoint described once, in agent-neutral terms, so the same
recipe applies to any agent:

```sh
confai preset list
confai preset show byesu
confai preset apply byesu --all --api-key sk-... --use --sync
```

Built-in presets live in [`presets/`](presets/) — one TOML file each, baked into
the binary at build time. Adding one is a pull request that touches a single new
file. Your own presets go in `~/.confai/presets/`, and override a built-in with
the same id.

## Agents

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

## Contributing

A preset is one new file in `presets/`. A new agent is one new file in
`src/agent/` implementing `Agent` and `AgentConfig` — the layers above it stay
untouched. Run `cargo test` and `cargo clippy --lib --bins --tests` before
opening a pull request.

## Licence

[MIT](LICENSE) © [redstone.md](https://redstone.md)
