# Agent and contributor guidance

Structured conventions for AI agents and humans working in this repository. For
install/usage, see [README.md](README.md). Prefer the linked sources below over
inventing project conventions.

## Docs

- [`.agents/docs/ARCHITECTURE.md`](.agents/docs/ARCHITECTURE.md) — high-level
  system architecture and diagrams
- [`README.md`](README.md) — install, setup, data-dir, CLI/HTTP, env vars
- [DeepWiki](https://deepwiki.com/deangrant/hexplace) — indexed project wiki
  (architecture, API, pipeline)

## Rules

- [`.agents/rules/`](.agents/rules/) (symlinked from [`.cursor/rules`](.cursor/rules))
- [`.agents/rules/hexplace-project.mdc`](.agents/rules/hexplace-project.mdc) —
  always-on out-of-scope limits and skill routing
- [`.agents/rules/rust-style.mdc`](.agents/rules/rust-style.mdc) — 100-col /
  500-line caps; points at style-guide-rust
- [`.agents/rules/crate-boundaries.mdc`](.agents/rules/crate-boundaries.mdc) —
  core → engine → HTTP ownership and dependency direction

## Skills

- [`.agents/skills/`](.agents/skills/) — canonical skill directory (no `.cursor` copy)
- [`.agents/skills/hexplace-architecture/`](.agents/skills/hexplace-architecture/) —
  crates, indexes, query paths, out of scope
- [`.agents/skills/hexplace-import/`](.agents/skills/hexplace-import/) — PBF
  import, staging/lock, schema bumps, fixtures
- [`.agents/skills/hexplace-verify/`](.agents/skills/hexplace-verify/) — CI-parity
  fmt / clippy / test checklist
- [`.agents/skills/solid-rust/`](.agents/skills/solid-rust/) — SOLID design in Rust
- [`.agents/skills/style-guide-rust/`](.agents/skills/style-guide-rust/) — Rust
  formatting, file size, docs, naming

## Commands

- [`.agents/commands/`](.agents/commands/) (symlinked from [`.cursor/commands`](.cursor/commands))
- `/verify` — run local CI checklist (fmt, clippy, test)
- `/architecture` — answer or plan against crate/index boundaries
- `/import-check` — review import/lock/schema/fixture mistakes

## Hooks

- Config: [`.cursor/hooks.json`](.cursor/hooks.json)
- `afterFileEdit` → [`.agents/hooks/rustfmt-after-edit.sh`](.agents/hooks/rustfmt-after-edit.sh)
  formats edited `*.rs` files (fail open)
