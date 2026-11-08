# SOLID Design Principles (Rust)

Narrative for Part 2 of [SKILL.md](../SKILL.md). Tables and when-to-apply
notes live in [design-reference.md](design-reference.md); code snippets in
[design-examples.md](design-examples.md).

## 1. Single Responsibility Principle (SRP)

A module, struct, or function should have one reason to change—one axis of
change driven by business or technical requirements.

Elements that change together belong together. High cohesion keeps edits
localized. Low cohesion mixes unrelated concerns in the same place.

Ask: if product requirements shift on axis X, how many unrelated places must
you edit? If the answer is "many," responsibilities may be entangled.

SRP is not "one function per file" or "never put two fields in a struct."
Over-fragmentation creates noise, harder navigation, and artificial
boundaries. Split when a responsibility is clearly separable and has its own
change pressure. Keep items together when splitting would only duplicate
context.

Rust-specific mechanisms:

- **Ownership and borrowing:** A type that owns a socket or file should be the
  only place that closes it. Callers borrow for shorter operations.
- **Modules (`mod`, `pub`, `pub(crate)`):** Isolate auth, parsing, I/O, and
  policy into separate modules with explicit APIs.
- **Traits:** Separate behavior interfaces (for example `Notifier`) from
  concrete transports (`EmailNotifier`, `SmsNotifier`).
- **Enums and pattern matching:** Push variant-specific logic into `match`
  arms or methods on the enum. Avoid boolean flags that mix unrelated states.
- **Dependency injection:** Pass dependencies through constructors with trait
  bounds (`fn new(n: impl Notifier)`) so the type's responsibility stays clear.

## 2. Open/Closed Principle (OCP)

Designs should be open for extension (add new behavior) and closed for
modification (avoid churn in stable, well-tested core code).

Prefer adding new types (`impl Trait` for a new variant, a new enum arm with
delegated logic, a new strategy struct) over repeatedly editing a central
`match` that knows every detail—when that core is truly stable. The goal is to
minimize edits to the part everyone depends on, not to freeze the codebase.

OCP does not mean "never change any file." Early code changes freely; mature
boundaries deserve stability. OCP is not an excuse for layers of indirection
on every feature. Simple code that changes in one place is often correct.

Introduce extension points (traits, enums) when multiple variants or
plugin-style growth is real. A single implementation does not need a trait
hierarchy "for future use."

Rust-specific mechanisms:

- **Traits as extension points:** Add behavior by implementing the trait for a
  new type without editing existing implementations (when the trait is
  stable).
- **Generics:** `fn process<T: Format>(t: T)` — static dispatch,
  monomorphization, usually fastest.
- **Trait objects:** `&dyn Trait`, `Box<dyn Trait>` — runtime polymorphism, one
  binary, dynamic dispatch cost.
- **Enums and delegated arms:** A new variant extends behavior; each arm can
  call shared helpers or inner types.

Common Gang of Four (GoF) patterns in Rust:

- **Strategy:** Swappable algorithms via a trait (for example
  `PaymentStrategy`).
- **Decorator:** A wrapper type also implements the same trait, delegating with
  added behavior.
- **Abstract Factory:** A factory trait returning `Box<dyn Product>` (or
  generic associated types) to construct families of objects.

Prefer **generics** when the implementing type is known at compile time and
performance matters. Use **trait objects** when you need heterogeneous
collections or runtime selection.

## 3. Liskov Substitution Principle (LSP)

Any implementation usable where a trait or abstraction is expected must
preserve correctness for callers. Substitution must not break assumed
behavior.

Behavioral contracts:

- **Preconditions:** What must hold before a method runs (for example "buffer
  non-empty").
- **Postconditions:** What the method guarantees after (for example "returns
  parsed value or error").
- **Invariants:** What stays true for the object across operations.

Contract rules (informal): subtypes should not strengthen preconditions (do
not require more from callers) or weaken postconditions or invariants (do not
deliver less than promised).

LSP is not "all structs must look the same." It is about preserving
assumptions—including error behavior and side effects—that generic code relies
on.

Rust-specific mechanisms:

- **Traits** document the contract; document semantics in rustdoc, not only
  signatures.
- **Enums:** Each variant's payload and handling should respect the same
  logical expectations where code treats the enum uniformly.
- **Generics and associated types:** Associated types are part of the
  contract; changing their meaning can break substitutability.
- **Trait objects:** If `dyn Trait` is used, every implementation must be a
  valid substitute. LSP is the pressure test for whether `dyn` is safe.

Avoid needlessly complex trait bounds. Avoid `downcast`-style escape hatches
that defeat abstraction. Prefer correct contracts over clever
inheritance-style hierarchies.

## 4. Interface Segregation Principle (ISP)

Clients should not depend on methods they do not use. Prefer small, focused
traits over a single fat trait that forces every implementor to care about
unrelated operations.

ISP is not "exactly one method per trait." It is client-driven decomposition:
group methods by who needs them (role interfaces).

Name traits by capability or role—for example `Switchable`, `Adjustable`,
`Lockable`—rather than `EverythingDeviceDoes`.

Rust-specific mechanisms:

- **Fine-grained traits** and multiple `impl` blocks on one type for different
  traits.
- **`+` bounds:** `fn f<T: Read + Write>(t: T)` — require only the combination
  this API needs.
- **Supertraits:** `trait Serial: Read + Write` when the combined role is
  stable.
- **Trait objects:** Smaller traits mean smaller vtables and clearer `dyn`
  boundaries.
- **Precise generic bounds:** Avoid `T: MegaTrait` when only `T: Serialize` is
  needed.

Like SRP, avoid over-engineering. Ten one-method traits can be as hard to
follow as one god trait. Segregate where different clients have different
needs.

## 5. Dependency Inversion Principle (DIP)

High-level policy should not depend on low-level details. Both should depend on
abstractions. Abstractions (traits) should not depend on concrete types;
details implement abstractions.

DIP is not the same as dependency injection (DI). DIP is about dependency
direction and ownership of abstractions. DI (constructor injection, factories)
is a technique to achieve inverted dependencies.

Inversion of Control (IoC): high-level modules define the trait they need;
low-level modules implement it. Wiring happens at the root (`main`, composition
root, tests).

Rust-specific mechanisms:

- **Traits** as the stable abstraction layer between layers.
- **Generics with trait bounds** — static dispatch, compile-time checking, no
  heap indirection for the trait itself.
- **`&dyn Trait` / `Box<dyn Trait>`** — dynamic dispatch when details vary at
  runtime.
- **Module visibility** — `pub` / `pub(crate)` to expose interfaces while
  hiding struct fields and submodules.
- **Constructor injection** — `new(repo: impl Repository)` or
  `new(logger: Box<dyn Logger>)`; builders for complex graphs.

GoF patterns aligned with DIP:

- **Abstract Factory** — factory trait producing trait objects or generic
  products.
- **Strategy** — algorithm trait injected into a high-level type.
- **Template Method** — trait with default methods calling hooks implemented by
  subtypes.

Not every dependency deserves a trait. Direct use of a concrete type is fine
when there is one implementation and no test or extension pressure.

Traits and injection enable mocks and stubs so high-level logic can be
unit-tested without databases or the network.
