---
name: hexplace-verify
description: >-
  Run Hexplace CI-parity checks (fmt, clippy, tests) before finishing Rust work
  or opening a PR. Use when verifying changes, after features/fixes, or when
  the user runs /verify.
trigger: >-
  verify, CI parity, cargo fmt, clippy, cargo test, before PR, pass CI,
  quality bar, lint
---

# Hexplace verify (CI parity)

Match [`.github/workflows/lint.yml`](../../../.github/workflows/lint.yml) and
[`.github/workflows/test.yml`](../../../.github/workflows/test.yml). Run from
the repository root. Do not claim the work is done until these pass (or you
report remaining failures).

## Required

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Fix failures you introduced. Do not broaden scope into unrelated cleanup unless
asked.

## When touching dependencies

Also consider (matches [`.github/workflows/audit.yml`](../../../.github/workflows/audit.yml)):

```bash
cargo audit
```

## Style reminders while fixing

- Line width 100; avoid growing `.rs` files past ~500 lines
- Prefer `#[expect]` over `#[allow]`
- See skills `style-guide-rust` and `solid-rust` for deeper guidance
