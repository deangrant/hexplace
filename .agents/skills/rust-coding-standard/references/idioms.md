# Idioms and Robustness (detail)

Longer explanations and examples for Part 3 of [SKILL.md](../SKILL.md).
Adapted from common Rust guides (including BeautifulRust's rust-best-practices)
to this repository's conventions: exact dependency pins, no `thiserror`, the
five exit codes, and the stdout / stderr observability split.

Naming, lint overrides, and formatting live in [style.md](style.md) and
[SKILL.md](../SKILL.md). This file owns ownership, types, errors, concurrency,
tests, performance, dependencies, and refactoring.

## Ownership and borrowing

**Prefer unique ownership.** Reach for `Rc` / `Arc` only when shared ownership
is part of the design. Reach for `RefCell` / `Mutex` only when interior
mutability is. Shared ownership and interior mutability move checking from
compile time to run time, so use them on purpose.

**Do not clone to silence the borrow checker.** A clone that exists only to end
a borrow produces a silent copy whose later changes are lost. Try these first:

1. Extend a temporary with a named `let`, or shorten a borrow with a block.
2. Split a long expression into typed intermediates so the failing step is
   visible.
3. Move a field out of `&mut self` with `std::mem::take` (needs `Default`),
   `std::mem::replace`, or `Option::take` instead of cloning it.
4. Split a large struct into smaller ones so disjoint fields can be borrowed
   separately. The compiler tracks disjoint field borrows inside one function,
   but not across method calls.
5. Clone small, immutable data deliberately, and say so in a comment if it is
   not obvious.
6. Use indices into an owning `Vec`, or `Rc<RefCell<_>>` with `Weak`
   back-pointers, as a last resort. Avoid self-referential structs.

```rust
// Move out of a &mut enum without cloning.
fn a_to_b(e: &mut MyEnum) {
    if let MyEnum::A { name, x: 0 } = e {
        *e = MyEnum::B {
            name: std::mem::take(name),
        };
    }
}
```

**Accept the most general borrowed type.**

- Prefer `&str` over `&String`.
- Prefer `&[T]` over `&Vec<T>`.
- Prefer `&T` over `&Box<T>`.
- Prefer `Option<&T>` over `&Option<T>`.
- On parameters that only read a path, string, or byte buffer, `impl AsRef<Path>`,
  `impl AsRef<str>`, or `impl AsRef<[u8]>` is fine. Never store those on struct
  fields; store `PathBuf` and `String`.
- Library code that only iterates may take `impl IntoIterator<Item = T>`.
  Application code may simply take `&[T]`.

**Take ownership only when you need it.**

- If a function stores or consumes its argument, take `T`, not `&T` plus an
  internal `.clone()`.
- If a function only reads, take `&T`.
- If a function mutates in place, take `&mut T`.
- Return multiple values as a tuple or struct, not through `&mut`
  out-parameters. Reusing a caller-owned buffer, as `Read::read` does, is the
  exception.

**Prefer owned data in structs.** A struct holding `&'a T` forces a lifetime
onto every container. Owning a `String` or `Vec<u8>` is usually simpler. When a
type does carry a lifetime, give it a descriptive name (`'src`, `'conn`). `'_`
and `'a` are fine on simple functions.

**Keep `Rc`, `Arc`, `Box`, and `RefCell` out of public signatures.** Take `&T`,
`&mut T`, or `T` and let callers decide how to own things. A service that must
be shared implements a cheap `Clone` by wrapping its state in `Arc<Inner>`
internally.

## Types and API design

### Model the domain with types

- Use **newtypes** for semantic distinctions: `struct Miles(f64)` and
  `struct Kilometers(f64)` cannot be mixed, where a type alias can. A newtype
  that encodes an invariant validates in a fallible constructor (`TryFrom`,
  `FromStr`, `try_new`) and never offers an infallible `From` from the raw
  type.
- Avoid **`bool` parameters** in public or shared signatures. `report(true)` is
  unreadable at the call site. Use a two-variant enum or two methods. The only
  exception is a private helper whose call sites sit a few lines away.
- Prefer **named fields over tuples**, except single-field newtypes and very
  local code.
- Use **`Option` for absence**, never sentinels such as `-1` or `""`. Prefer
  **data-carrying enums** instead of "field X is only meaningful when Y is
  set".
- Use the strongest std type: `PathBuf` / `&Path` for paths, `Duration` for
  spans. Keep plain integers at API boundaries even when `NonZeroU32` is used
  internally.

### Derive and implement the standard traits

Downstream crates cannot add std traits to your types (orphan rule), so you
must. Derive whenever every field supports it: `Debug` (on every public type;
hand-write it to redact secrets), `Clone`, `Copy` (only small bitwise-copyable
types), `PartialEq` / `Eq`, `PartialOrd` / `Ord`, `Hash` (consistent with
`Eq`), `Default`. Implement `Display` for anything meant to be read. Add
`#[must_use]` to pure functions and to types whose discarding is a bug.

Implement `From` / `TryFrom`, never `Into` / `TryInto` directly. Prefer
`from` / `into` over `as`, which silently allows lossy casts. Do not put
derivable trait bounds on struct definitions: `#[derive(Clone)] struct Good<T>`,
not `struct Bad<T: Clone + Debug>`. Bounds belong on the `impl` blocks that
need them.

If you hand-write `Ord`, hand-write `PartialEq`, `PartialOrd`, and `Hash` to
agree with it.

### Constructors, builders, defaults

- Provide `new()` even when `Default` exists, and make them agree.
- Up to two optional parameters: inherent constructors (`new`, `with_a`).
  Beyond that, or when construction validates, add a builder:
  `Foo::builder()` returns `FooBuilder`, setters are named after fields and
  infallible, and `build() -> Result<Foo, Error>` does all validation.
- A constructor with four or more loose arguments should group them into
  semantic types.

### Visibility and evolution

- Keep items private by default. Fields stay private unless the struct is
  passive C-style data. Widen to `pub(crate)` or `pub(super)` before `pub`.
- Mark public enums and public-field structs that cross crate boundaries
  `#[non_exhaustive]` unless the set is a stable ontology.
- Seal traits you do not want implemented downstream, and say so in the docs.
- Expose each public item at exactly one path. Do not leak third-party types
  into the public API. Feature-gate `serde` impls behind a feature named
  `serde`.

### Naming additions

Summary naming rules live in [style.md](style.md). Additions for API design:

- If you implement `len()`, implement `is_empty()`.
- Name error types verb-object-error: `ParseBoolError`, not `BoolParseError`.
- Avoid weasel suffixes `Service`, `Manager`, `Factory`, `Helper`, `Util`.
  Name the thing it is. Do not repeat the module name in the type
  (`foo::Id`, not `foo::FooId`).

## Errors and panics

| Situation | Use |
| --- | --- |
| Absence is normal (`first()`, `checked_add`) | `Option<T>` |
| The caller may need to know what went wrong | `Result<T, E>` |
| A bug or violated invariant in production code | Return `Err` / `Internal` |

**The build denies panics.** `unwrap_used`, `expect_used`, `panic`, and
`panic_in_result_fn` are workspace `deny` lints, including in tests and
`main`. Do not write `unwrap()`, `expect(...)`, or `panic!`. Prefer `?`,
`let-else`, and `ok_or` / `ok_or_else`.

**Give every test assertion a reason-style message.** Use
`assert!(cond, "…")` / `assert_eq!(got, want, "…")`. This is a review
rule: Clippy's `missing_assert_message` is denied for non-test code and
does not fire inside test functions. A message states **why** the value
must hold or **which contract** it verifies. It does not restate the
expression, embed concrete inputs or outputs, use a generic label, or
name only the property under check.

```rust
// Good: names the rule or contract.
assert_eq!(
    Exit::Success as u8,
    0,
    "ADR 0009 maps Success to the stable Batch success exit code"
);
assert!(
    matches!(error, Error::Config(_)),
    "omitting a required container argument is a configuration fault"
);

// Bad: expression echo, generic label, or bare property name.
assert_eq!(Exit::Success as u8, 0, "Exit::Success as u8 == 0");
assert!(matches!(error, Error::Config(_)), "expected Config");
assert_eq!(columns.reg_id, &["r1"], "reg_id");
assert_eq!(got, want, "values should be equal");
```

Escape `{` / `}` as `{{` / `}}` when the text embeds Rust source that
contains braces. Never paste `Error::Transient { .. }` into a message;
rustc treats `{ .. }` as format syntax and fails the build. Longer
messages wrap under the 100-column limit and add lines: if a file
nears 500 lines, split along a real seam rather than shortening the
message into an echo.

**Pattern-match instead of check-then-unwrap:** `if let Some(v) = opt { .. }`
or `let Some(v) = opt else { return .. };`, never
`if opt.is_some() { opt.unwrap() }`.

**Recover lock poisoning; do not panic on it:**
`.lock().unwrap_or_else(std::sync::PoisonError::into_inner)`. `expect` is
denied, so recovering with `into_inner` is the repository pattern (see
`crates/common/src/blob/memory.rs`).

**Error types in this repository:**

- Implement `std::error::Error` (requires `Debug` + `Display`), be
  `Send + Sync + 'static`, and implement `source()` for wrapped causes. Never
  use `()`, `String`, or `Box<dyn Error>` as a library's public error type.
- **Hand-write `Display` and `Error`.** `thiserror` is deliberately absent from
  the workspace (see the root `Cargo.toml` comment).
- Write `Display` messages as lowercase, concise text with no trailing
  punctuation: `"invalid header length"`.
- Implement `From<Upstream>` so `?` converts. Use `map_err` only for foreign
  errors or to add context.
- Use **`anyhow` with `.context(...)` only at the binary edge**, never in a
  library's public API.
- **Map every failure onto the five exit codes** in
  [`crates/common/src/exit.rs`](../../../../crates/common/src/exit.rs): Config
  (1), Input (2), Transient (3), Internal (4). An unclassified error is
  `Internal`, not `Transient`: guessing retryable spends nodes discovering that
  a bug is still a bug.
- Destructors never fail, panic, or block. Provide `close(self) -> Result<()>`
  for fallible teardown and let `Drop` do best-effort cleanup.

## Everyday idioms

- Prefer **iterators over hand loops** when the pipeline is short and reads
  top-down. Keep an explicit `for` loop when the body is large, does several
  jobs, or needs awkward early returns. Bind a long chain to a named variable
  rather than putting it in a `for` header.
- Prefer **`Option` / `Result` combinators** (`map`, `and_then`, `ok_or_else`,
  `unwrap_or_else`, `as_ref`, `as_deref`) when they are clearer than `match`.
- Prefer **`format!` / `write!`** with inline args (`format!("{name}: {count}")`),
  not chained `+`.
- Add **explicit types where inference obscures intent** on `.into()`,
  `.parse()`, `.collect()`. Prefer `let x: Vec<u32> = ..` to a turbofish.
- Use **drop guards for cleanup** that must run on early return or unwind.
  Bind them to a name (`let _guard = ..`), because `let _ = ..` drops
  immediately.
- **Import style:** one `use` per module path; types and traits imported and
  used unqualified; free functions qualified by their module (`mem::take`,
  `fs::read`). No globs except `use super::*` in tests and `use Enum::*` in a
  function that matches on it.
- Turn **magic numbers** into named `const`s with a doc comment that says where
  the value comes from.
- **Logging:** structured diagnostics through `tracing` go to **stderr**. Wide
  events go to **stdout** through telemetry only. The build denies
  `print_stdout`, `print_stderr`, and `dbg_macro`: never `println!`,
  `eprintln!`, or `dbg!` (build scripts that emit `cargo:` lines are the
  exception). See `AGENTS.md` observability.

## Concurrency and async

- Public types, and the futures they return, should be `Send` (and usually
  `Sync`). Pin that with a compile-time check so a refactor cannot silently
  regress it:

  ```rust
  const _: () = {
      const fn assert_send<T: Send>() {}
      assert_send::<MyClient>();
  };
  ```

  Holding an `Rc` or a `RefCell` borrow across an `.await` makes a future
  `!Send`.
- Prefer message passing (channels) to shared state. When sharing is necessary:
  keep data with linked invariants under one lock, keep lock scopes to a few
  lines, never call user closures while holding a lock, and never return a
  `MutexGuard` from a public method.
- Write `async fn` rather than `fn -> impl Future`, except in traits or when
  setup must run outside the async block for a hot path.
- Long CPU-bound work inside async must yield or move to `spawn_blocking`.
- Library code does not spawn raw threads or assume a runtime; take an executor
  or `impl AsyncRead` / `impl Read` from the caller.
- **In this repository:** `BlobSource` and `BlobSink` are synchronous because
  the phases that use them are synchronous Rayon pipelines, even though the
  Azure SDK underneath is async. Prefer that pattern over leaking async into
  the transform pipeline.

## Unsafe and FFI

- The build forbids `unsafe_code` in `[workspace.lints.rust]`. Do not write
  `unsafe` blocks, `unsafe fn`, or `unsafe impl`. There is no override.
- Prefer safe std and crates already in the tree. If a third-party crate needs
  `unsafe` internally, do not re-export or copy that `unsafe` into this tree.
- FFI is out of scope for application crates here. Do not add C bindings.

## Tests

- Put unit tests in `#[cfg(test)] mod tests { use super::*; .. }` for private
  internals. Put integration tests in `tests/` for anything reachable through
  the public API. Prefer `tests/` when either would do. Doc examples are tests
  too.
- Test observable behavior and properties (monotonic, round-trips,
  idempotent), not restated constants or mirrored branches.
- Write a failing test before fixing a bug.
- Anything doing I/O, time, randomness, or network must be mockable through
  consumer-owned traits (see DIP in
  [design-principles.md](design-principles.md)), not a `Default` that performs
  I/O.
- Fuzz parsers of untrusted input with `cargo fuzz`.
- Assertions get a reason-style message so failures name the contract:
  `assert_eq!(got, want, "registration ids stay in CSV row order")`.
  See also the assertion-message rule above.

## Performance

Design for usability first and optimize after measuring. When you do:

- Prefer `Vec::with_capacity` or `collect()` over push loops.
- Reuse caller-owned buffers in hot APIs.
- Prefer `Box<[T]>`, `Box<str>`, `Arc<str>` for long-lived immutable sequences.
- Call `shrink_to_fit` on long-lived collections built without a reservation.
- Derive `Copy` for small plain types and never for large ones.

**In this repository:** jemalloc is present because Polars allocates heavily;
that is a measured carve-out, not a preference. For DataFrame work follow the
[`rust-polars`](../../rust-polars/SKILL.md) skill.

## Dependencies and crates

- Prefer the standard library and crates already in the tree. Fewer
  dependencies means a smaller binary, a shorter build, and less attack
  surface. See `AGENTS.md` "Dependencies: stdlib first".
- Use **exact `=` pins** hoisted into `[workspace.dependencies]`. The
  `version_*` build scripts read those pins; a non-exact pin makes the key
  disappear. Do **not** switch to version ranges.
- Configure lints once in `[workspace.lints]`; each crate opts in with
  `[lints] workspace = true`. Never put lint levels in source
  (`#![deny(...)]` / `#![allow(...)]`). Summary: [SKILL.md](../SKILL.md)
  "Enforced by the build".
- Keep features additive and bare-named: `std`, `serde`, `alloc`. Never
  `no-std` or a feature that removes a public item.
- Re-export individually (`pub use foo::{A, B};`), never with a glob.

## Refactoring existing code

- Fix every smell the request is about, not just the cheap ones: clones that
  only satisfy the borrow checker, `&String` / `&Vec<T>` parameters, any
  `unwrap` / `expect` / `panic!`, `Rc<RefCell<_>>` in signatures, `get_`
  getters, `bool` parameters, `+` string concatenation, error types without
  `Error`.
- Keep public behavior identical unless changing it is the point: the same
  operations, the same success and failure semantics, the same output bytes.
  Dropping an error variant or changing what a constructor takes is not
  welcome. If a signature must change, show the before and after and say why.
- Preserve quirks you do not understand and flag them, rather than silently
  "fixing" them.
- When the module has no tests, pin the current behavior with a few before you
  touch it.
