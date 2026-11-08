# Style (detail)

Part 1 detail for [SKILL.md](../SKILL.md). `rustfmt.toml` at the repository
root is the source of truth for formatting. These rules override conflicting
defaults.

## Core formatting

- Keep every line at or under **100 characters** (including indentation,
  comments, and strings). Prefer 80–90 characters when that improves
  readability.
- Keep every `.rs` file under **500 lines** (excluding blank lines and the
  license header). When a file approaches 450 lines, extract structs, enums,
  and `impl` blocks into modules; move large helpers; group related items.

## rustfmt alignment

Follow rustfmt defaults with these overrides:

- Use spaces only; never tabs. Indent with **4 spaces**.
- Set `max_width = 100`. Wrap comment prose by hand near 80–100 columns
  (`wrap_comments` is off, so rustfmt does not reflow comments).
- Put trailing commas in multi-line lists, arrays, tuples, and matches.
- Keep method chains within `chain_width = 80`.
- Use block indentation (not visual alignment) for function arguments and
  structs.
- Set `imports_granularity = "Module"` (or `"Crate"` consistently across the
  crate).
- Set `group_imports = "StdExternalCrate"`.
- Use a **single `#[derive(...)]`** with comma-separated traits (for example
  `#[derive(Debug, Clone)]`). Do not stack separate `#[derive(...)]`
  attributes. Keep derived trait names in a stable order so tooling diffs stay
  quiet.

## Blank lines and whitespace

- Separate items and statements with zero or one blank line. Do not leave two
  or more consecutive blank lines between items.
- Strip trailing whitespace on every line (code, comments, blank lines). When
  editing a string literal, preserve its intended value.

## Comments

- Prefer line comments (`//`) over block comments (`/* ... */`). Put a single
  space after `//`.
- Prefer a comment on its own line. When a comment follows code on the same
  line, put a single space before it.
- Write comments as complete sentences: start with a capital letter and end
  with a period. Inline block comments may be short notes without punctuation.
- Explain **why**, not what. Restating the next line is noise.
- Keep a full-line comment at or under 80 characters of comment text
  (including `//`, excluding indentation).

## Doc comments

Full rules: [documentation.md](documentation.md). Summary rules also appear in
[SKILL.md](../SKILL.md).

- Prefer outer doc comments (`///`) and line style. Use inner (`//!`) only for
  module- or crate-level docs.
- Put doc comments before attributes (for example `/// Summary.` then
  `#[derive(Debug)]`).
- Keep the first sentence on one line and at most 15 words.
- Document every public module with `//!`.
- Use canonical sections when they apply: `# Examples`, `# Errors`, `# Abort`.
  Add `# Panics` only for implicit panic paths the build does not deny. Do
  not add `# Safety`.
- Describe parameters in prose. Do not use parameter tables.
- For `pub use` re-exports of crate items, add `#[doc(inline)]` at the use
  site. Do not inline `std` or third-party types.
- Document non-trivial private functions (see [documentation.md](documentation.md)).

## Attributes

- Put each attribute on its own line, indented to the item. Prefer outer
  attributes. Format attribute argument lists like function calls. Put a space
  before and after `=` in `key = value` forms.

## Small items

- Use single-line formatting when a construct is small (for example
  `Foo { f1, f2 }`). Use multi-line block style when items are longer or more
  complex. Prefer the rustfmt heuristic over hand-aligned columns.

## Functions

- Prefer functions under about 100 lines and low cognitive complexity. Prefer
  even smaller functions in review (about 40–50 lines) when a clear seam
  exists. Honor `clippy.toml` limits when the repository sets them.
- Prefer early returns over deep nesting. Use `let-else` and `if let` to
  reduce nesting.
- Break long expressions early so indentation does not drift right.
- Use inherent methods for constructors (static, no `self`) and when there is
  a clear receiver. Otherwise prefer free functions unless an associated
  function improves clarity.

## Naming

- Keep a consistent word order. Omit the `get_` prefix on getters.
- Name ad-hoc conversions with `as_`, `to_`, or `into_`. Name iterators
  `iter`, `iter_mut`, or `into_iter`.
- Prefer concise names without filler words. Follow casing in RFC 430.

## Public types

- Implement `Debug` for every public type. Hand-write `Debug` when a field must
  be redacted.
- Implement `Display` for types meant to be read by users.

## Lint overrides

- Override a lint only with
  `#[expect(clippy::name, reason = "...")]` on the narrowest item. `expect`
  warns when the lint stops firing.
- Prefer `#[expect]` over `#[allow]`. Avoid blanket crate-level `#![allow]`.
- Prefer configuring lints in `[workspace.lints]` rather than putting lint
  levels in source.

## Tests

- Prefer focused `#[test]` functions with descriptive names.
- Avoid large test blocks that mix unrelated cases.
- Failure and panic rules live in [SKILL.md](../SKILL.md) Part 3 and
  [idioms.md](idioms.md).
