---
name: rust-coding-standard
description: >-
  Use this skill whenever you write, change, review, refactor, or debug Rust
  code (.rs files, Cargo crates, clippy or rustfmt output), even if the user
  does not mention style, design, or best practices. It sets a portable Rust
  standard: 100-column lines, 500-line files, doc comments, traits and
  dependency injection, ownership, Result and Option error handling, process
  exit codes, async, and tests.
compatibility: Requires cargo, rustfmt, and clippy for the handback checklist.
allowed-tools: Bash(cargo fmt:*) Bash(cargo clippy:*) Bash(cargo test:*) Read Grep
metadata:
  author: 9986059+deangrant@users.noreply.github.com
  version: "1.1"
---

# Rust Coding Standard

Follow this standard for every Rust change. Prefer matching the repository's
existing `rustfmt.toml`, `clippy.toml`, and workspace lint configuration when
present; never loosen those files to make code pass. Where the repo is silent,
apply the rules below.

Read this file first. Open a file under `references/` only when the task needs
more detail.

---

# Baseline expectations

Treat these as required practice. Repositories often enforce a subset via
`Cargo.toml`, `clippy.toml`, `rustfmt.toml`, and CI.

- **Toolchain**: use the repository's `rust-toolchain.toml` / `rust-version`
  when present; otherwise current stable. Use nightly only when a documented
  rustfmt option requires it (`cargo +nightly fmt`).
- **Lint inheritance**: prefer `[lints] workspace = true` and a single
  `[workspace.lints]` table. Avoid crate-level `#![deny(...)]` /
  `#![allow(...)]` in source.
- **Forbidden** (`forbid` when the workspace sets it): `unsafe_code`,
  `non_ascii_idents`.
- **Avoid panics in production paths**: do not call `.unwrap()`, `.expect()`,
  or `panic!`. Prefer `?`, `let-else`, and `ok_or` / `ok_or_else`.
- **Avoid ad-hoc process I/O**: no `println!`, `eprintln!`, or `dbg!` in
  library or application code (build scripts that emit `cargo:` lines are the
  exception). Prefer `tracing` (or the repo's logging stack) on stderr.
- **Async hygiene**: never hold a lock or a `RefCell` borrow across `.await`.
- **Size**: prefer small functions and types; avoid very large futures, stack
  arrays, or by-value moves of huge types when references suffice.
- **Docs**: document public items; implement `Debug` for public types.
- **Casts**: prefer `TryFrom`; otherwise
  `#[expect(clippy::cast_..., reason = "...")]` at the site.
- **Exit**: return `std::process::ExitCode` from `main`; never
  `std::process::exit`.
- **Files**: keep every `.rs` file under **500** lines. Split near 450 along a
  real seam.

---

# Part 1: Style

Detail: [references/style.md](references/style.md).
`rustfmt.toml` at the repository root is the source of truth for formatting
when present.

- Keep every line at or under **100 characters**.
- Keep every `.rs` file under **500 lines**. Split near 450 lines along a real
  seam.
- Use spaces only and **4-space** indent. Put trailing commas in multi-line
  constructs. Keep method chains within `chain_width = 80` when rustfmt sets
  it. Use one `#[derive(...)]` with comma-separated traits.
- Separate items with zero or one blank line. Strip trailing whitespace.
- Write comments that explain **why**, not what. Prefer `//` line comments.
  Wrap comment prose by hand near 80–100 columns (`wrap_comments` is often
  off).
- Put `///` outer docs before attributes. Document every public module with
  `//!`. Keep the first sentence on one line and at most 15 words. Full rules:
  [references/documentation.md](references/documentation.md).
- Prefer early returns and `let-else`. Omit the `get_` prefix on getters.
  Implement `Debug` for every public type.
- Override lints only with `#[expect(lint, reason = "...")]` on the narrowest
  item. Prefer `#[expect]` over `#[allow]`.

---

# Part 2: Design (SOLID)

Use this part when shaping crates for clarity and safe evolution. Rust replaces
nominal interfaces with **traits**. Ownership and borrowing decide who holds
resources. Detail:
[references/design-principles.md](references/design-principles.md),
[references/design-reference.md](references/design-reference.md),
[references/design-examples.md](references/design-examples.md).

- **Single Responsibility Principle (SRP)**: give each type or module one
  coherent reason to change. Split when responsibilities diverge.
- **Open/Closed Principle (OCP)**: extend with new `impl`s, generics, or enum
  arms. Avoid editing a stable core for every new behavior.
- **Liskov Substitution Principle (LSP)**: every `impl Trait` (or variant arm)
  must honor the behavioral contract callers rely on.
- **Interface Segregation Principle (ISP)**: let clients depend only on the
  methods they need. Prefer small traits.
- **Dependency Inversion Principle (DIP)**: high-level code depends on traits;
  concrete types implement them at the edges (`main`, tests). Place interfaces
  with the **consumer**.

**Traits as interfaces:** Object-safe traits can be `&dyn Trait` /
`Box<dyn Trait>`. Prefer generics when monomorphization is fine. Prefer `dyn`
when heterogeneous collections or smaller binaries matter.

**Ownership affects dependencies:** Passing `&T`, owned `T`, or `Arc<T>` encodes
lifetime and responsibility. Constructor injection with `impl Trait` or generic
bounds keeps allocation and sharing choices explicit.

---

# Part 3: Idioms and Robustness

Detail: [references/idioms.md](references/idioms.md).

- Model errors with `Result`, absence with `Option`, and resources with
  ownership and `Drop`. Make invalid states unrepresentable with enums,
  newtypes, and fallible constructors.
- Prefer unique ownership. Do not clone only to silence the borrow checker.
- Accept `&str`, `&[T]`, and `Option<&T>`. Do not accept `&String`, `&Vec<T>`,
  or `&Option<T>`.
- Avoid `bool` parameters in public or shared signatures.
- Omit the `get_` prefix on getters. Implement `From` / `TryFrom`; never
  implement `Into` / `TryInto` directly.
- Return errors with `?`. Never call `.unwrap()`, `.expect()`, `.expect_err()`,
  or `panic!`, including in tests and `main`.
- `#[expect(...)]` lint attributes are allowed; method calls named `expect`
  are not.
- In tests, use `assert!(…, "…")` / `assert_eq!(…, …, "…")` with messages, or
  `let-else` / `?` in helpers. Do not use `.expect()` or `.expect_err()`.
- Handle `None` with `let-else` or `ok_or` / `ok_or_else`.
- Recover a poisoned lock with
  `unwrap_or_else(std::sync::PoisonError::into_inner)`.
- Give every test assertion a message that states **why** the value must
  hold or **which contract** it verifies. Do not restate the expression,
  embed concrete inputs or outputs, use a generic label (`"expected Config"`,
  `"expected Ok"`, `"values should be equal"`), or a bare property name
  (`"reg_id"`).
- Prefer typed errors that implement `std::error::Error` + `Display` +
  `Debug` and are `Send + Sync + 'static`. `thiserror` is fine for derives;
  hand-written impls are fine too. Use `anyhow` (or similar) with context
  only at the binary edge, not in library public APIs.
- From `main`, return `std::process::ExitCode`. Map distinct failure classes
  (for example config versus I/O versus internal) to stable codes when the
  binary defines them. Treat unclassified errors as internal, not as
  "retryable", unless the contract says otherwise.
- Send diagnostics through `tracing` (or the repo logger) to **stderr**.
  Never use `println!`, `eprintln!`, or `dbg!` outside build scripts.
- Never hold a lock or a `RefCell` borrow across `.await`.
- Do not write `unsafe` blocks or functions unless the repository explicitly
  allows and reviews them.
- Prefer the standard library and crates already in the workspace. Hoist
  shared versions into `[workspace.dependencies]` when the repo uses that
  pattern.
- Keep CPU-bound synchronous pipelines sync at the trait boundary; do not
  force async through layers that never `.await`.

---

# Before you hand back code

List these workspace commands by default (adjust if the repo documents
different flags):

1. Format: `cargo fmt --all`
2. Lint: `cargo clippy --workspace --all-targets -- -D warnings`
3. Test: `cargo test --workspace`

You may also note a package-scoped `cargo test -p …`. Do not replace the
workspace list with only the package-scoped command.

Review-only checks (CI may not catch these):

- [ ] One primary responsibility per type or module (SRP).
- [ ] New behavior adds `impl`s or variants with minimal core edits (OCP).
- [ ] Every `impl Trait` keeps callers correct (LSP).
- [ ] Traits are as small as each client needs (ISP).
- [ ] High-level code depends on traits; wire concrete types at the boundary
  (DIP).
- [ ] Public APIs take `&str`, `&[T]`, or `impl AsRef<_>`.
- [ ] No clone exists only to silence the borrow checker.
- [ ] Errors are proper `Error + Display + Send + Sync + 'static` types.
- [ ] Binaries return `ExitCode`; unclassified failures are internal.
- [ ] Assertion messages state a reason or contract, not an expression
  echo or `"expected Config"`-style label.

---

# References

- **Style detail**: [references/style.md](references/style.md)
- **Documentation**: [references/documentation.md](references/documentation.md)
- **Design narrative**:
  [references/design-principles.md](references/design-principles.md)
- **Design reference**:
  [references/design-reference.md](references/design-reference.md)
- **Design examples**:
  [references/design-examples.md](references/design-examples.md)
- **Idioms detail**: [references/idioms.md](references/idioms.md)
- **External links**:
  [references/external-links.md](references/external-links.md)

Apply this skill in all Rust-related responses unless the user explicitly asks
to ignore it.
