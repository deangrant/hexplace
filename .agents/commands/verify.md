# Verify

Run Hexplace CI-parity checks using skill `.agents/skills/hexplace-verify`.

From the repository root:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`

If any step fails, fix issues introduced by the current work and re-run until
green (or report remaining failures clearly). Do not expand into unrelated
refactors unless asked.

When `Cargo.toml` / `Cargo.lock` dependencies changed, also run `cargo audit`
if available and note advisories.
