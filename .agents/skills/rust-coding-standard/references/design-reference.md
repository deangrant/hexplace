# SOLID (Rust) — Reference

Quick lookup: tables, checklists, and Rust idioms. Concepts align with
[SKILL.md](../SKILL.md); code patterns in
[design-examples.md](design-examples.md).

---

## Principle summary

| Principle | Rust idiom | Key question | Reasons to change | Anti-pattern indicator |
| --------- | ---------- | ------------ | ----------------- | ---------------------- |
| **SRP** | Modules, ownership boundaries, focused traits | What single purpose does this type or module serve? | List change drivers; unrelated drivers in one place suggest a split | One file mixes auth, I/O, and UI because it is small |
| **OCP** | Traits, `impl`, enums, generics | Can I add behavior without editing stable core? | New features should touch extension points, not every central `match` | Every new feature edits one 500-line function |
| **LSP** | Trait contracts, enum invariants, documented semantics | Can any `impl` substitute without breaking callers? | Contract = preconditions, postconditions, and invariants; subtype must preserve them | `impl` returns wrong semantics, or panics where the trait promises `Result` |
| **ISP** | Small traits, `+` bounds, role interfaces | Does this client need every method on this trait? | Different clients → different trait slices | `unimplemented!()` on unused trait methods |
| **DIP** | Trait bounds, `dyn`, constructor injection, `pub` API | Does high-level code name concrete DB or HTTP types? | Policy modules should cite traits; details implement at edges | `use postgres::Client` deep inside domain logic |

---

## SRP — expanded checklist

**Too-many-responsibilities signs**

- [ ] Does one module handle unrelated concerns (for example HTTP + CSV +
  business rules)?
- [ ] Do changes for feature A routinely break or retest feature B?
- [ ] Does the same struct own I/O resources and encode business policy with
  no boundary?
- [ ] Do tests require heavy setup because the unit under test does everything?

**Cohesion indicators (good)**

- [ ] Do edits cluster in one module when a single requirement changes?
- [ ] Do names and imports read as one story at the module level?
- [ ] Can you describe the responsibility in one sentence without "and also"?

**Over-engineering warnings**

- [ ] Are types so thin that call chains obscure data flow?
- [ ] Is every private helper its own module with no reuse?
- [ ] Did splitting create duplicate DTOs and mapping glue without benefit?

**Reasons-to-change prompts**

- If API versioning changes, which modules move?
- If the persistence backend swaps, which modules move?
- If UI layout changes, which modules move?
- Overlap in answers suggests merged responsibilities.

---

## SRP — when to apply vs. over-engineering

| Apply | Over-engineering |
| ----- | ---------------- |
| A clear second axis of change appears (new integration, new policy). | Splitting for purity with no second implementation or test pain. |
| Tests are hard because of mixed concerns. | Extracting helpers that are only used once and never reused. |
| Ownership of a resource is ambiguous. | One logical service artificially cut into five crates. |

Prefer cohesive modules that team members can reason about. Split when change
pressure or testability justifies a boundary—not when a diagram looks cleaner.

---

## OCP — expanded checklist

**Open-for-extension signs**

- [ ] Is new behavior added via new types (`impl Trait`), new enum variants, or
  new strategy structs?
- [ ] Do stable algorithms depend on traits or small closed sets of variants,
  not on stringly-typed tags scattered everywhere?

**Closed-for-modification signs**

- [ ] Do core loops and domain rules rarely change when adding a new export
  mode or format?
- [ ] Do reviewers see most additions in new files or new arms, not edits to
  fragile baselines?

**Extension mechanism selection**

| Need | Consider |
| ---- | -------- |
| Compile-time variants, performance | Generics, `impl Trait` arguments |
| Runtime plugin / heterogeneous list | `Box<dyn Trait>`, `&dyn Trait` |
| Closed set of cases with shared data | Enum + `match`, delegated methods |
| Optional layers of behavior | Decorator-style wrappers implementing the same trait |

**Anti-pattern indicators**

- [ ] Does adding a feature require editing many unrelated `if` branches?
- [ ] Does a "god" `match` or `if` chain know every product-specific detail?
- [ ] Were traits introduced for one implementation "just in case"?

---

## OCP — when to apply vs. over-engineering

| Apply | Over-engineering |
| ----- | ---------------- |
| Multiple known variants or a real roadmap for plugins. | Trait hierarchy for a single caller and one implementation. |
| Stable core already identified and regression-tested. | Abstraction before any second use ("future-proofing"). |
| Enum or trait boundary matches product language. | Dynamic dispatch when generics would be simpler and faster. |

---

## LSP — expanded checklist

**Substitutability verification**

- [ ] For each `impl Trait`, would generic code behave correctly without
  special cases?
- [ ] Do error paths and side effects match what callers expect from
  documentation?
- [ ] Do associated types and return types avoid surprising callers (for
  example stricter requirements)?

**Contract rules checklist**

- [ ] Preconditions: not stricter than the trait documents (do not require more
  from callers).
- [ ] Postconditions: not weaker than promised (do not return less information
  or weaker guarantees).
- [ ] Invariants: preserved across public methods for every implementation.

**Anti-pattern indicators**

- [ ] Does an `impl` use `panic!` or unreachable paths where the trait implies
  a recoverable `Result`?
- [ ] Do implementations ignore parameters or return dummy data?
- [ ] Does code downcast (`Any`, concrete matches) to recover behavior the
  trait should express?

**Testing guidance**

- [ ] Share property tests or contract tests across implementations.
- [ ] Write fakes and stubs that match production semantics, not only types.

---

## LSP — when to apply vs. over-engineering

| Apply | Over-engineering |
| ----- | ---------------- |
| Public trait used by multiple crates or generic algorithms. | Formal proof-style contracts for internal helpers. |
| `dyn Trait` or plugin boundaries—substitution bugs are costly. | Re-documenting obvious `Clone`/`Debug` behavior on every `impl`. |

Avoid unbounded associated type gymnastics or trait bounds that encode
business rules better expressed in tests and docs.

---

## ISP — expanded checklist

**Fat trait indicators**

- [ ] Do many methods go unused, with most implementors using
  `unimplemented!()` or no-ops?
- [ ] Does client code call only one or two methods but import the whole trait?
- [ ] Do names like `Manager`, `Handler`, or `Service` mix unrelated verbs?

**Well-segregated trait indicators**

- [ ] Does the trait name reflect a role (`Readable`, `Switchable`,
  `ExportRequest`)?
- [ ] Is each client's `use` list minimal, with bounds that match actual calls?
- [ ] Does the same concrete type implement several small traits on purpose?

**Refactoring guide for fat traits**

1. List call sites and which methods each uses.
2. Cluster methods by client usage, not by implementor type.
3. Extract traits; provide blanket `impl` or inherent methods only if needed.
4. Prefer composition (`T: A + B`) over a merged mega-trait at call sites.

---

## ISP — when to apply vs. over-engineering

| Apply | Over-engineering |
| ----- | ---------------- |
| Different subsystems need different subsets of methods. | One method per trait so every function lists six bounds. |
| A mock or fake would implement half the trait with stubs. | Traits split when a single client always uses all methods together. |

---

## Trait composition patterns

| Pattern | Syntax / form | Typical use |
| ------- | ------------- | ----------- |
| Multiple bounds | `fn f<T: Read + Write>(t: T)` | API needs exactly those capabilities |
| Supertrait | `trait Serial: Read + Write {}` | Named bundle of roles |
| Combined trait alias (custom) | `trait Combined: TraitA + TraitB {}` | Shorthand for repeated bounds |
| Multiple impls | `impl A for T {}` + `impl B for T {}` | Same type, different capability facets |
| Trait object (single) | `&dyn Trait` | Erased type, vtable dispatch |
| Trait object + auto traits | `&(dyn MyTrait + Send + Sync)` | **E0225:** in a trait object only the first trait is the principal type; any additional traits must be auto traits (for example `Send`, `Sync`), not arbitrary second traits like `dyn Read + Write`. |
| Two capabilities behind one `dyn` | `trait TraitAB: TraitA + TraitB {}` and `impl<T: TraitA + TraitB> TraitAB for T {}`, then `&dyn TraitAB` | One object-safe aggregate trait avoids invalid `dyn TraitA + TraitB` syntax. |

---

## DIP — expanded checklist

**Correct dependency direction**

- [ ] Does domain or policy code import traits or stable module APIs, not vendor
  SDKs?
- [ ] Do infrastructure crates (`db`, `http`, `fs`) implement traits defined at
  or above the domain boundary?
- [ ] Does `main` / the binary root wire concrete types into generic or `dyn`
  parameters?

**Anti-pattern indicators**

- [ ] Do high-level functions construct `SqliteConnection` or `reqwest::Client`
  directly?
- [ ] Does an "abstract" trait exist only to mirror one concrete type 1:1
  everywhere?
- [ ] Do tests spin up real services because nothing can be swapped?

**Testing benefits**

- [ ] Do unit tests inject fakes for I/O and time?
- [ ] Do integration tests use real adapters while unit tests stay fast and
  deterministic?

---

## DIP — when to apply vs. over-engineering

| Apply | Over-engineering |
| ----- | ---------------- |
| Multiple implementations (prod, mock, alternate vendor). | Trait for `new()` of a struct with one caller forever. |
| Need to test domain without network or DB. | `Box<dyn>` for every helper with no polymorphism. |
| Library boundary meant for third-party extensions. | "Clean architecture" folders with no change pressure. |

---

## Advanced DIP patterns

| Pattern | Rust shape | Notes |
| ------- | ---------- | ----- |
| Abstract Factory | `trait Factory { fn make(&self) -> Box<dyn Product>; }` | Families of products behind one abstraction |
| IoC wiring in `main` | `let repo = PostgresRepo::new(...); let app = App::new(repo);` | Composition root builds the graph |
| Mock or stub testing | `struct MockRepo; impl Repository for MockRepo { ... }` | Swap at test construction |
| Module boundary | `pub` facade module; `pub(crate)` internals | DIP + encapsulation: depend on a stable `pub` API |

---

## Rust-specific patterns mapped to SOLID

| Pattern | Primary principles | Notes |
| ------- | ------------------ | ----- |
| **Trait** | OCP, ISP, DIP, LSP | Contract and extension point; document behavior, not just signatures |
| **Generic `T: Trait`** | OCP, DIP, LSP | Static dispatch; compile-time substitution check |
| **`dyn Trait`** | OCP, DIP, LSP | Runtime polymorphism; LSP critical—every implementor must fit |
| **Module `pub` / `pub(crate)`** | SRP, DIP | Hide details; expose stable surface |
| **Newtype** | SRP, LSP | Separate domain wrapper from raw primitive semantics |

**Advanced (by principle)**

| Principle | Advanced Rust patterns |
| --------- | ---------------------- |
| **SRP** | `mod` per concern; RAII guards owning resources; enums instead of boolean state soup |
| **OCP** | Trait + `impl`; enum with `#[non_exhaustive]` for extensibility warnings; strategy structs |
| **LSP** | Sealed traits (`pub trait` + private supertrait pattern); careful `default` methods on traits |
| **ISP** | Role traits; `+` bounds; split `Read`/`Write` vs monolithic trait |
| **DIP** | Constructor injection (`new`, builder); `async` services taking `impl Port`; test doubles at boundary |
