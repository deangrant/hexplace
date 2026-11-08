# External references

Align with these style guides and checklists when applying the
rust-coding-standard skill. Part 3 (idioms) is also informed by
[BeautifulRust rust-best-practices](https://github.com/morganlinton/BeautifulRust/tree/main/skills/rust-best-practices),
adapted to this repository's conventions (exact pins, no `thiserror`, exit
codes, stdout / stderr split).

## Official Rust style guide

- **URL**: https://doc.rust-lang.org/beta/style-guide/index.html
- **Use it for**: line length 100, 4-space indent, block indent, trailing
  commas, blank lines, trailing whitespace, comments, attributes, and small
  items.

## Rust API guidelines

- **URL**: https://rust-lang.github.io/api-guidelines/
- **Checklist**: https://rust-lang.github.io/api-guidelines/checklist.html
- **Use it for**: naming, interoperability, documentation, predictability,
  flexibility, type safety, dependability, debuggability, and future proofing.

## Microsoft Pragmatic Rust Guidelines

- **URL**: https://microsoft.github.io/rust-guidelines/
- **Checklist**:
  https://microsoft.github.io/rust-guidelines/guidelines/checklist/index.html
- **Documentation**:
  https://microsoft.github.io/rust-guidelines/guidelines/docs/
- **Use it for**: first sentence at most 15 words (M-FIRST-DOC-SENTENCE),
  module docs (M-MODULE-DOCS), canonical sections (M-CANONICAL-DOCS), and
  `#[doc(inline)]` for crate re-exports (M-DOC-INLINE). This repository
  encodes those rules in [documentation.md](documentation.md). Also covers
  universal, library, apps, FFI, safety, performance, and AI guidance. Prefer
  regular functions where appropriate (M-REGULAR-FN).

## Google Rust Style Guide

- **URL**: https://google.github.io/styleguide/rustguide.html
- **Use it for**: formatting, imports, module layout, accessor naming, and
  application versus library guidance.

## Effective Rust

- **URL**: https://www.lurklurk.org/effective-rust/
- **Use it for**: choosing between alternatives: generics versus `dyn`,
  `Result` versus panic, iterators versus loops, borrow-checker tactics,
  dependencies, and CI.

## Rust Design Patterns

- **URL**: https://rust-unofficial.github.io/patterns/
- **Use it for**: idioms, patterns, and anti-patterns with code.

## Comprehensive Rust (Google)

- **Course**: https://google.github.io/comprehensive-rust/
- **Repo**: https://github.com/google/comprehensive-rust
- **Use it for**: a course that follows the official Rust style.

## rustfmt

- **URL**: https://rust-lang.github.io/rustfmt/
- **Use it for**: config and options.
