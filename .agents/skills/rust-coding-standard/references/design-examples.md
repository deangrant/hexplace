# SOLID (Rust) — Examples

Teaching snippets for review and design discussions. See
[SKILL.md](../SKILL.md) for principles and when to apply them. Examples
**1–5** (SRP), **6–9** (OCP), **10–12** (LSP), **13–15** (ISP), **16–19**
(DIP).

Snippets are illustrative; they use `todo!()` and `println!`, which this
repository denies in real code.

---

## Example 1: SRP — separate modules (auth vs decrypt)

Split unrelated change axes into modules so “rotate API keys” does not collide
with “swap crypto backend.”

```rust
// auth.rs — only token acquisition and validation
pub fn bearer_token(client_id: &str, secret: &str) -> Result<String, AuthError> {
    todo!()
}

// decrypt.rs — only ciphertext → plaintext
pub fn decrypt_blob(key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>, DecryptError> {
    todo!()
}
```

---

## Example 2: SRP — one struct owns resource lifecycle

The owner closes the handle; callers borrow for work. One clear responsibility
for the connection’s lifetime.

```rust
pub struct DbConnection {
    // inner handle elided
}

impl DbConnection {
    pub fn connect(url: &str) -> Result<Self, std::io::Error> {
        todo!()
    }

    /// Exclusive use; consumers cannot leak use-after-close.
    pub fn query(&mut self, sql: &str) -> Result<Vec<Row>, QueryError> {
        todo!()
    }
}

impl Drop for DbConnection {
    fn drop(&mut self) {
        // close once
    }
}
```

---

## Example 3: SRP — trait decomposition (`Notifier` / `EmailNotifier` / `SmsNotifier`)

Behavior interface separate from transport; each type changes for one reason
(email templates vs SMS length limits).

```rust
pub trait Notifier {
    fn notify(&self, user_id: &str, message: &str) -> Result<(), NotifyError>;
}

pub struct EmailNotifier {
    smtp_host: String,
}

impl Notifier for EmailNotifier {
    fn notify(&self, user_id: &str, message: &str) -> Result<(), NotifyError> {
        todo!("resolve email by user_id, send via SMTP")
    }
}

pub struct SmsNotifier {
    twilio_sid: String,
}

impl Notifier for SmsNotifier {
    fn notify(&self, user_id: &str, message: &str) -> Result<(), NotifyError> {
        todo!("resolve phone by user_id, send SMS")
    }
}
```

---

## Example 4: SRP — enums and variant-specific logic

Each variant carries its own data; `match` keeps policies from interleaving
unrelated `if` chains.

```rust
pub enum Job {
    Download { url: String },
    Transform { pipeline_id: u32 },
}

impl Job {
    pub fn run(&self) -> Result<(), JobError> {
        match self {
            Job::Download { url } => {
                todo!("fetch only")
            }
            Job::Transform { pipeline_id } => {
                todo!("transform only")
            }
        }
    }
}
```

---

## Example 5: SRP — constructor injection with trait bounds

`ReportService` is responsible for orchestration; it does not construct
low-level policy—only receives it.

```rust
pub trait Clock {
    fn now(&self) -> std::time::Instant;
}

pub struct ReportService<C: Clock> {
    clock: C,
}

impl<C: Clock> ReportService<C> {
    pub fn new(clock: C) -> Self {
        Self { clock }
    }

    pub fn stamp(&self) -> std::time::Instant {
        self.clock.now()
    }
}
```

---

## Example 6: OCP — trait extension point (`Shape` + `area`)

Add `Circle` / `Rectangle` without editing existing shapes if the trait is
stable (contrast with a single growing `match` on a string type tag).

```rust
pub trait Shape {
    fn area(&self) -> f64;
}

pub struct Rectangle {
    pub w: f64,
    pub h: f64,
}

impl Shape for Rectangle {
    fn area(&self) -> f64 {
        self.w * self.h
    }
}

pub struct Circle {
    pub r: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.r * self.r
    }
}
```

---

## Example 7: OCP context — `Command` enum + `execute()` via `match` (closed set)

This pattern is **not** the usual “open for extension, closed for modification”
story: the set of commands is **fixed in one place**. Adding a new command means
editing the `Command` enum **and** the `execute` method’s `match` arms (and
often `Engine`). That is a deliberate **trade-off**: routing stays simple,
exhaustive, and compiler-checked, but you **cannot** plug in new commands from
outside the crate without changing these definitions. For plugin-style
extension without editing this match, you would typically use a trait object
(for example `dyn CommandHandler`) or a registry.

```rust
pub enum Command {
    Start,
    Stop,
    Pause,
}

impl Command {
    pub fn execute(&self, engine: &mut Engine) {
        match self {
            Command::Start => engine.start(),
            Command::Stop => engine.stop(),
            Command::Pause => engine.pause(),
        }
    }
}

pub struct Engine;

impl Engine {
    pub fn start(&mut self) {}
    pub fn stop(&mut self) {}
    pub fn pause(&mut self) {}
}
```

---


## Example 8: OCP — generic vs trait object (trade-offs)

Generics: **static dispatch**, monomorphization, inlined calls — best when the
concrete type is fixed per call site. Trait objects: **one** type-erased path,
heap/indirection for `Box`, dynamic dispatch — best for heterogeneous
collections or runtime plugin choice.

```rust
pub trait Sink {
    fn write(&mut self, line: &str);
}

/// Static dispatch: each `W` gets its own machine code — fast, compile-time resolved.
pub fn log_static<W: Sink>(sink: &mut W, msg: &str) {
    sink.write(msg);
}

/// Dynamic dispatch: same function for all implementors — flexible, vtable call.
pub fn log_dynamic(sink: &mut dyn Sink, msg: &str) {
    sink.write(msg);
}
```

---

## Example 9: OCP — Strategy pattern (`PaymentStrategy` / `CreditCard` / `PayPal`)

Swap payment algorithms without editing checkout orchestration if `Checkout`
depends only on the trait.

```rust
pub trait PaymentStrategy {
    fn charge(&self, amount_cents: u64) -> Result<PaymentId, PaymentError>;
}

pub struct CreditCard {
    pub last4: String,
}

impl PaymentStrategy for CreditCard {
    fn charge(&self, amount_cents: u64) -> Result<PaymentId, PaymentError> {
        todo!()
    }
}

pub struct PayPal {
    pub email: String,
}

impl PaymentStrategy for PayPal {
    fn charge(&self, amount_cents: u64) -> Result<PaymentId, PaymentError> {
        todo!()
    }
}

pub struct Checkout<'a> {
    pub payment: &'a dyn PaymentStrategy,
}

#[derive(Clone, Copy)]
pub struct PaymentId(pub u64);

pub enum PaymentError {
    Declined,
}
```

---

## Example 10: LSP — `ChunkBuffer` and `run_double_buffered_export` (this codebase)

The real definitions live in
[`crates/export/src/historic_export/chunk_buffer.rs`](../../../../crates/export/src/historic_export/chunk_buffer.rs).
The **contract**: any `B: ChunkBuffer` must implement push/length/flush/final
consistently so the generic export loop stays correct. `PreviewChunkBuffer`,
`LocationEventsChunkBuffer`, `IpAddressChunkBuffer`, and `DeviceChunkBuffer`
are substitutable implementations.

```rust
// Abbreviated; full definitions in
// crates/export/src/historic_export/chunk_buffer.rs

pub trait ChunkBuffer {
    fn push_row(&mut self, record: &csv::StringRecord);
    fn len(&self) -> usize;
    fn flush_if_full(&mut self) -> Option<ChunkWriteData>;
    fn final_chunk(&mut self) -> Option<ChunkWriteData>;
}

pub fn run_double_buffered_export<R, B>(
    mut csv_reader: csv::Reader<R>,
    mut buffer_a: B,
    mut buffer_b: B,
    // ... output paths, channels, etc.
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: std::io::Read,
    B: ChunkBuffer,
{
    todo!("double-buffered loop uses only ChunkBuffer methods")
}

pub struct ChunkWriteData; // placeholder; real type in crate
```

---

## Example 11: LSP — `ExportRequest` (this codebase)

`ExportRequest` is a minimal teaching trait: every export response type
provides `request_id()` for polling. The snippet below is illustrative; it is
not a live type in this repository.

```rust
pub trait ExportRequest {
    fn request_id(&self) -> &str;
}

pub struct LocationDataSearchResponse {
    pub request_id: String,
}

impl ExportRequest for LocationDataSearchResponse {
    fn request_id(&self) -> &str {
        &self.request_id
    }
}

fn poll_status<R: ExportRequest>(req: &R) -> String {
    format!("polling {}", req.request_id())
}
```

---

## Example 12: LSP — `Shape` violation, then correction

**Violation:** `Square` implements `Shape` but `area()` returns the wrong
quantity (e.g. perimeter or a bogus formula). Generic code that relies on
`area()` meaning “surface area” becomes wrong—substitutability fails.

```rust
pub trait Shape {
    fn area(&self) -> f64;
}

pub struct Square {
    pub side: f64,
}

/// LSP break: callers expect `area` ≈ side² for a square.
impl Shape for Square {
    fn area(&self) -> f64 {
        4.0 * self.side // wrong — this is perimeter, not area
    }
}
```

**Correction:** Honor the **semantic contract** of the trait—same meaning for
`area` everywhere.

```rust
pub trait Shape {
    fn area(&self) -> f64;
}

pub struct Square {
    side: f64,
}

impl Square {
    pub fn new(side: f64) -> Self {
        Self { side }
    }
    pub fn set_side(&mut self, s: f64) {
        self.side = s;
    }
}

impl Shape for Square {
    fn area(&self) -> f64 {
        self.side * self.side
    }
}
```

---

## Example 13: ISP — fat `Device` vs narrow traits

Fat trait forces implementors to care about heating, locks, and switches even
when the client only toggles power.

```rust
pub trait Device {
    fn toggle_power(&mut self);
    fn set_temperature(&mut self, c: f32);
    fn lock(&mut self);
}

pub struct SmartPlug;
impl Device for SmartPlug {
    fn toggle_power(&mut self) {}
    fn set_temperature(&mut self, _: f32) {
        unimplemented!() // ISP smell: not a thermostat
    }
    fn lock(&mut self) {
        unimplemented!() // ISP smell: not a lock
    }
}
```

**Narrow traits** — clients depend on what they need:

```rust
pub trait Switchable {
    fn toggle_power(&mut self);
}

pub trait Thermostat {
    fn set_temperature(&mut self, c: f32);
}

pub trait DoorLock {
    fn lock(&mut self);
}

impl Switchable for SmartPlug {
    fn toggle_power(&mut self) {}
}
```

---

## Example 14: ISP — `ExportRequest` as a focused trait (this codebase)

One method, one role: “has export request id.” Clients that poll do not depend
on HTTP payload details.

```rust
pub trait ExportRequest {
    fn request_id(&self) -> &str;
}
```

---

## Example 15: ISP — trait composition with `+` bounds

Require exactly the capabilities the function uses—no fat custom trait.

```rust
pub trait Validate {
    fn validate(&self) -> Result<(), &'static str>;
}

pub fn save_json<T>(value: &T) -> Result<Vec<u8>, String>
where
    T: serde::Serialize + Validate,
{
    value.validate().map_err(String::from)?;
    serde_json::to_vec(value).map_err(|e| e.to_string())
}

pub fn copy_streams<R, W>(mut r: R, mut w: W) -> std::io::Result<u64>
where
    R: std::io::Read,
    W: std::io::Write,
{
    std::io::copy(&mut r, &mut w)
}
```

---

## Example 16: DIP — `ReportGenerator` depends on `DataSource`, not `PostgresDatabase`

High-level reporting depends on an abstraction; the database adapter implements
it at the edge.

```rust
pub trait DataSource {
    fn fetch_rows(&self, query: &str) -> Result<Vec<Row>, DbError>;
}

pub struct ReportGenerator<T: DataSource> {
    db: T,
}

impl<T: DataSource> ReportGenerator<T> {
    pub fn new(db: T) -> Self {
        Self { db }
    }
    pub fn monthly_summary(&self) -> Result<String, DbError> {
        let rows = self.db.fetch_rows("SELECT ...")?;
        Ok(format!("{} rows", rows.len()))
    }
}

pub struct Row;
pub enum DbError {
    Failed,
}
```

---

## Example 17: DIP — `Application` + `Box<dyn Logger>` constructor injection

Swap `ConsoleLogger` / `FileLogger` without changing `Application`’s
orchestration code.

```rust
pub trait Logger {
    fn log(&self, msg: &str);
}

pub struct ConsoleLogger;
impl Logger for ConsoleLogger {
    fn log(&self, msg: &str) {
        println!("{msg}");
    }
}

pub struct FileLogger {
    pub path: std::path::PathBuf,
}

impl Logger for FileLogger {
    fn log(&self, msg: &str) {
        todo!("append to file")
    }
}

pub struct Application {
    logger: Box<dyn Logger>,
}

impl Application {
    pub fn new(logger: Box<dyn Logger>) -> Self {
        Self { logger }
    }
    pub fn run(&self) {
        self.logger.log("started");
    }
}
```

---

## Example 18: DIP — generic struct vs trait object (side by side)

```rust
pub trait Repository {
    fn get(&self, id: u64) -> Option<String>;
}

/// Static dispatch: concrete repo type chosen at compile time — zero-cost, fixed at build.
pub struct ServiceStatic<R: Repository> {
    repo: R,
}

impl<R: Repository> ServiceStatic<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
    pub fn load(&self, id: u64) -> Option<String> {
        self.repo.get(id)
    }
}

/// Dynamic dispatch: repo type can vary at runtime (e.g. mock vs prod) — flexibility, vtable/indirection cost.
pub struct ServiceDynamic {
    repo: Box<dyn Repository>,
}

impl ServiceDynamic {
    pub fn new(repo: Box<dyn Repository>) -> Self {
        Self { repo }
    }
    pub fn load(&self, id: u64) -> Option<String> {
        self.repo.get(id)
    }
}
```

---

## Example 19: DIP — `run_double_buffered_export` depends on `ChunkBuffer`

High-level export loop is written against `ChunkBuffer`; it does not encode
preview vs location-events vs device details. Same pattern as Example 10
([`crates/export/src/historic_export/chunk_buffer.rs`](../../../../crates/export/src/historic_export/chunk_buffer.rs)).

```rust
// Abbreviated from crates/export: real `ChunkWriteData` lives in
// crates/export/src/historic_export/types.rs.
pub struct ChunkWriteData; // placeholder

pub trait ChunkBuffer {
    fn push_row(&mut self, record: &csv::StringRecord);
    fn len(&self) -> usize;
    fn flush_if_full(&mut self) -> Option<ChunkWriteData>;
    fn final_chunk(&mut self) -> Option<ChunkWriteData>;
}

pub fn run_double_buffered_export<R, B>(
    mut csv_reader: csv::Reader<R>,
    mut buffer_a: B,
    mut buffer_b: B,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: std::io::Read,
    B: ChunkBuffer,
{
    todo!("policy depends on ChunkBuffer, not concrete buffer struct")
}
```
