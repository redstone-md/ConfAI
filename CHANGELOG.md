# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
