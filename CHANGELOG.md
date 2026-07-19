# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.4](https://github.com/redstone-md/ConfAI/releases/tag/v0.0.4) - 2026-07-19

### Features

- The last four things only the command line could do ([`764d89c`](https://github.com/redstone-md/ConfAI/commit/764d89c85eb17eefba96c92cc61aeccc28353bf9))
- Write a preset, a server or an undo into every agent at once ([`96767ac`](https://github.com/redstone-md/ConfAI/commit/96767ac56b157841955a8941a1aab532bd1a714e))
- Refresh the model catalogue, and preview a sync before running it ([`552d832`](https://github.com/redstone-md/ConfAI/commit/552d8327b55c49f6d1ddcc25831aff0fa85298f1))
- The preset picker shows the card, not just the row ([`9705416`](https://github.com/redstone-md/ConfAI/commit/9705416f74305c56c66858b0bf9fb3572c65865b))
- Name the upgrade commands in the update report ([`2223c60`](https://github.com/redstone-md/ConfAI/commit/2223c608d6e8afcfe4f0307d87708f472d782b84))
- Close the CLI/TUI capability gap ([`8ecbb97`](https://github.com/redstone-md/ConfAI/commit/8ecbb9768e558494eb1c24581be9edd5b3f9a88e))
- List the keyless actions in the help screen too ([`392ec09`](https://github.com/redstone-md/ConfAI/commit/392ec094f94c0cceb8211879600a714cfb25f043))
- Search and install MCP servers from the registry, in the MCP lens ([`bf54eca`](https://github.com/redstone-md/ConfAI/commit/bf54eca3123443e930d4a5e6bb64c1a7c8d9cbd9))
- Open an agent's config in $EDITOR without leaving the view ([`2c073b1`](https://github.com/redstone-md/ConfAI/commit/2c073b1e767b3d65bdcc3c76c755dd7918514796))
- Undo the last write, from the interface that made it ([`c7b32c7`](https://github.com/redstone-md/ConfAI/commit/c7b32c76f846f2327c2ada1051a8d6b59d0b08cf))
- Check for a new release on demand ([`cf97a58`](https://github.com/redstone-md/ConfAI/commit/cf97a581b9d60d9c52588d9472393eeda192cb95))
- Doctor, as a report you can read in the interface ([`e520d6b`](https://github.com/redstone-md/ConfAI/commit/e520d6b6f2cdd1baadce98c9d7a494977f2b9993))
- The lens is a row of clickable tabs ([`5e80179`](https://github.com/redstone-md/ConfAI/commit/5e80179cb041558e72b98131defe34c9ad12ea4e))
- Search and install MCP servers from the official registry ([`bc03052`](https://github.com/redstone-md/ConfAI/commit/bc03052e4ec1ee215a60d71a4c05f11dc154666c))

### Fixes

- Changing an opencode provider's wire protocol had no effect ([`f2b2d28`](https://github.com/redstone-md/ConfAI/commit/f2b2d28d7968d7632ee8f28b75e3300fae71719e))
- Keep the agent worktree out of the repository ([`f7e5909`](https://github.com/redstone-md/ConfAI/commit/f7e59096ba33766019d360243d97f04d1349e9f5))
- Make a test unable to write to a real config ([`1864ec2`](https://github.com/redstone-md/ConfAI/commit/1864ec2b1986584409fefb9487f17436ecfde704))

### Other

- Format the registry command bodies ([`2eced11`](https://github.com/redstone-md/ConfAI/commit/2eced1132545e66bbdb1d68c5623f0fabafc89d9))

Full changelog: [v0.0.3...v0.0.4](https://github.com/redstone-md/ConfAI/compare/v0.0.3...v0.0.4)



## [0.0.3](https://github.com/redstone-md/ConfAI/releases/tag/v0.0.3) - 2026-07-19

### Features

- Skills as a third lens in the interactive view ([`143d88b`](https://github.com/redstone-md/ConfAI/commit/143d88b096f89a64b67d2a5ecb215c10fba8ab2f))
- MCP in the interactive view, and skill management ([`b3405ed`](https://github.com/redstone-md/ConfAI/commit/b3405ed360c48e788add807856c360635c5a5b36))

### Fixes

- Point the upgrade hint at installers that exist ([`63155ee`](https://github.com/redstone-md/ConfAI/commit/63155eeb4600b16370335392aeadbb3013702086))

### Documentation

- MCP, skills and the three-lens key map, in six languages ([`8a0aa09`](https://github.com/redstone-md/ConfAI/commit/8a0aa09a9f7c0827791ce421c1ab5263c8b7e984))

Full changelog: [v0.0.2...v0.0.3](https://github.com/redstone-md/ConfAI/compare/v0.0.2...v0.0.3)



## [0.0.2](https://github.com/redstone-md/ConfAI/releases/tag/v0.0.2) - 2026-07-18

### Features

- Manage the MCP servers an agent launches ([`ab1e0a5`](https://github.com/redstone-md/ConfAI/commit/ab1e0a53bff2a9dfb2cbe5175155f7acb58c04d2))

### Fixes

- Satisfy the lint gate CI actually enforces ([`f6590a6`](https://github.com/redstone-md/ConfAI/commit/f6590a68550f39e69f4e78b48c26296c0a085919))

Full changelog: [v0.0.1...v0.0.2](https://github.com/redstone-md/ConfAI/compare/v0.0.1...v0.0.2)



## [0.0.1] - 2026-07-18

First tagged build. Everything below works and is covered by tests, but the
version says what it means: the interfaces are still free to move.

### Added

- One CLI over the configs of Codex, Claude Code and opencode. `confai list`
  shows which agents are installed, how many endpoints each has, which one is
  active, and where its config lives.
- Endpoint management with `confai provider`: `list` (with `--check`), `add`,
  `remove`, `use`, `check` and `sync`. `--agent` targets one agent and `--all`
  targets every installed one.
- `confai provider sync <id>` pulls an endpoint's model list from `/v1/models`
  and fills in context and output limits from models.dev, caching the catalogue
  for a day. Syncing merges, so nothing you configured is lost; `--prune` drops
  models the endpoint no longer serves and moves the selection to a surviving
  model if it removed the selected one. `--dry-run` shows what would change.
- `confai provider models <id>` lists what an endpoint serves with context and
  output limits and models.dev prices, and `--select` makes one the agent's
  model. This works for Codex and Claude Code too, which record a model but no
  model list.
- Presets: agent-neutral endpoint recipes applied with
  `confai preset apply <id>`, plus `preset list` and `preset show`. Built-in
  presets live in `presets/` and are baked into the binary at build time; user
  presets in `~/.confai/presets/` override a built-in with the same id.
  Twenty-six are included, covering OpenCode Zen, OpenRouter, OpenAI,
  Anthropic, Groq, xAI, Mistral, Cerebras, Together, DeepSeek, DeepInfra,
  Fireworks, Moonshot, Z.ai, Chutes, Baseten, Vercel AI Gateway, Venice,
  Novita, Ollama, LM Studio and Byesu.
- Interactive TUI, launched by running `confai` with no arguments: two panes of
  agents and endpoints, a `Ctrl+P` command palette, filtering, health checks,
  model selection and sync, preset application, and mouse support. Keys are
  matched by physical position so they work on non-Latin keyboard layouts.
- `confai update` reports whether a newer release exists and summarises it.
  Ordinary commands print a short notice from a cache checked at most once a
  day, so a run with a warm cache costs nothing. `CONFAI_NO_UPDATE_CHECK`
  disables it.
- `confai model`, `confai path`, `confai edit`, `confai doctor`, `confai about`
  and `confai undo`.
- File-safety guarantees: comments, key order and unknown keys survive an edit,
  because every backend edits the parsed document in place. Every write is
  backed up next to the original as `<name>.confai.bak` and replaces the file
  atomically, and `confai undo` restores it. JSON containing comments is refused
  rather than silently rewritten without them.
- Agent-specific handling: a roster of unused endpoints for Claude Code in
  `~/.confai/agents/`, since its config holds only one at a time; and, for
  opencode, reading keys from both `opencode.json` and
  `~/.local/share/opencode/auth.json`, updating an inline key where it already
  is, and showing but never overwriting an OAuth session.
- `CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG` and `XDG_CONFIG_HOME` are
  honoured, matching the agents' own behaviour.

[Unreleased]: https://github.com/redstone-md/ConfAI/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/redstone-md/ConfAI/releases/tag/v0.0.1
