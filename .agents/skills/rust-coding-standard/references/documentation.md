# Rust Documentation

Follow these rules when writing or editing Rust doc comments and module docs.
They align with the [Microsoft Pragmatic Rust Guidelines —
Documentation](https://microsoft.github.io/rust-guidelines/guidelines/docs/)
(M-FIRST-DOC-SENTENCE, M-MODULE-DOCS, M-CANONICAL-DOCS, M-DOC-INLINE).

## First sentence (summary)

The first sentence of an item's doc comment is the summary shown in the module
overview. Keep that sentence on **one line** and at most **15 words** so docs
stay skimmable.

Use outer doc comments (`///`) for items. Use inner (`//!`) only for module-
or crate-level docs.

## Module documentation

- Document every **public module** with `//!`.
- Apply the same first-sentence rule to the module doc.
- Cover, when relevant:
  - What the module contains, when to use it, and when not to use it
  - Examples and subsystem specs (for example a formatting language)
  - Observable side effects and guarantees
  - Relevant implementation details (for example system APIs used)

## What to document

- Document what the signature cannot say: preconditions, side effects,
  ownership transfer, complexity, and why.
- Do not restate the type or the name. `/// Removes an element.` on
  `remove_element` is noise.
- Link items with `` [`Type`] `` so rustdoc cross-links.

## Canonical sections

- Keep the summary sentence first.
- Add these sections when they apply (after the summary and any extended
  prose):
  - `# Examples` — one or more usage examples
  - `# Errors` — when the item returns `Result`, describe known error
    conditions
  - `# Abort` — when the item can abort the process, describe when
  - `# Implementation note` — optional; use when implementation details (for
    example algorithm choice or borrow tricks) help future readers
  - `# Panics` — only when a public function can still panic on a path the
    build does not deny (out-of-bounds index or slice, division by zero,
    `assert!`). Prefer returning an error instead, and describe the condition.
  - Do **not** add `# Safety`: `unsafe_code` is forbidden.
- Do **not** use parameter tables. Describe parameters in prose (for example
  "Copies from `src` to `dst`").

## Examples

Examples use `?` with `fn main` hidden behind `# ` lines. Never use
`unwrap()`, because readers copy examples verbatim:

```rust
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let v = parse("42")?;
/// assert_eq!(v, 42);
/// # Ok(())
/// # }
/// ```
```

## Documentation for private items

Document non-trivial private items. Prefer documenting when unsure.

### When to document private functions

Add `///` doc comments for a private function when any of these apply:

- The body exceeds 12 meaningful lines (not counting braces or empty lines).
- The function enforces module-level invariants the rest of the module relies
  on.
- The name is not self-explanatory (common in builders, parsers, state
  machines, and helpers with lifetime tricks).
- The function involves subtle performance choices, borrow splitting, or
  pinning.
- Future debugging or refactoring will need the explanation.

Between 8 and 12 lines, document when any of the non-length reasons above
apply; otherwise the author's judgment decides.

### When to skip

Do not force doc comments on tiny helpers: under 8 lines, a clear name, and no
performance subtlety.

### Style for private items

- Write full sentences that start with a verb in third person (same style as
  public API).
- Lead with purpose ("why does this exist?"), then important semantics,
  preconditions, or postconditions.
- Prefer brevity: aim for 1–4 lines unless complexity demands more.
- Use markdown sections when helpful: `# Errors`, `# Implementation note`.

Examples of good private doc comments:

```rust
/// Computes the smallest power of two ≥ n without overflowing usize.
///
/// Returns `None` on overflow (very large n).
fn next_power_of_two(n: usize) -> Option<usize> { … }

/// Splits a mutable borrow into two disjoint parts without reborrowing the
/// whole struct.
///
/// Avoids temporary lifetime issues that block later split borrows.
fn split_two_disjoint_mut(
    &mut self,
    a: usize,
    b: usize,
) -> (&mut T, &mut T) { … }
```

### When editing or refactoring

- When you add complexity to a private function, add or improve its doc
  comment.
- When you simplify a heavily documented private function, keep or shorten the
  comment. Do not delete useful invariants.

### Decision list

- Under 8 lines and self-documenting name: usually skip.
- Over 12 lines, or subtle logic: document purpose first.
- Important for module coherence: document invariants and why.

## Re-exports

- When publicly re-exporting **crate items** with `pub use foo::Foo` (or
  `pub use foo::*`), add **`#[doc(inline)]`** at the use site so the re-export
  is inlined in the docs.
- Do **not** use `#[doc(inline)]` for `std` or third-party types; leave those
  as opaque re-exports so readers see they are external.
- Prefer explicit re-exports over globs. `#[doc(inline)]` does not override
  the preference against glob re-exports.

## Reference

- **Source**: [Documentation — Pragmatic Rust
  Guidelines](https://microsoft.github.io/rust-guidelines/guidelines/docs/)
