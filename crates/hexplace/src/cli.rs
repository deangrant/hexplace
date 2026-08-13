//! Command-line interface for import, serve, and query.

use std::fs;
use std::io::{self, BufRead, ErrorKind, Write};
use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use hexplace_core::{BatchItem, BatchRequest, CoreError, Geocoder, ReverseQuery, SearchQuery};
use hexplace_engine::{bench_reverse, import_pbf, Engine, EngineConfig};
use tokio::runtime::Runtime;

use crate::api;

/// High-performance OpenStreetMap geocoding service.
#[derive(Debug, Parser)]
#[command(name = "hexplace", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level Hexplace subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Import an OSM PBF extract into a local data directory.
    Import(ImportArgs),
    /// Serve the HTTP API from an imported data directory.
    Serve(ServeArgs),
    /// Run a forward geocode query on the CLI.
    Search(SearchArgs),
    /// Run a reverse geocode query on the CLI.
    Reverse(ReverseArgs),
    /// Run a JSONL batch of geocode/reverse operations.
    Batch(BatchArgs),
    /// Time reverse lookups against an imported data directory.
    Bench(BenchArgs),
}

/// Arguments for `hexplace import`.
#[derive(Debug, Parser)]
pub struct ImportArgs {
    /// Path to an `.osm.pbf` file.
    #[arg(long)]
    pub pbf: PathBuf,
    /// Output data directory.
    #[arg(long, env = "HEXPLACE_DATA", default_value = "./data")]
    pub data_dir: PathBuf,
}

/// Arguments for `hexplace serve`.
#[derive(Debug, Parser)]
pub struct ServeArgs {
    /// Imported data directory.
    #[arg(long, env = "HEXPLACE_DATA", default_value = "./data")]
    pub data_dir: PathBuf,
    /// Listen address.
    #[arg(long, env = "HEXPLACE_BIND", default_value = "127.0.0.1:8080")]
    pub bind: SocketAddr,
}

/// Arguments for `hexplace search`.
#[derive(Debug, Parser)]
pub struct SearchArgs {
    /// Free-text query.
    pub query: String,
    /// Imported data directory.
    #[arg(long, env = "HEXPLACE_DATA", default_value = "./data")]
    pub data_dir: PathBuf,
    /// Maximum results.
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
}

/// Arguments for `hexplace reverse`.
#[derive(Debug, Parser)]
pub struct ReverseArgs {
    /// Latitude in degrees.
    #[arg(long)]
    pub lat: f64,
    /// Longitude in degrees.
    #[arg(long)]
    pub lon: f64,
    /// Imported data directory.
    #[arg(long, env = "HEXPLACE_DATA", default_value = "./data")]
    pub data_dir: PathBuf,
    /// Maximum results.
    #[arg(long, default_value_t = 1)]
    pub limit: usize,
}

/// Arguments for `hexplace batch`.
#[derive(Debug, Parser)]
pub struct BatchArgs {
    /// JSONL file of batch items (or `-` for stdin).
    #[arg(long)]
    pub file: PathBuf,
    /// Imported data directory.
    #[arg(long, env = "HEXPLACE_DATA", default_value = "./data")]
    pub data_dir: PathBuf,
}

/// Arguments for `hexplace bench`.
#[derive(Debug, Parser)]
pub struct BenchArgs {
    /// Imported data directory.
    #[arg(long, env = "HEXPLACE_DATA", default_value = "./data")]
    pub data_dir: PathBuf,
    /// Seed latitude for reverse samples.
    #[arg(long, default_value_t = 43.7384)]
    pub lat: f64,
    /// Seed longitude for reverse samples.
    #[arg(long, default_value_t = 7.4246)]
    pub lon: f64,
    /// Number of reverse lookups to time.
    #[arg(long, default_value_t = 10_000)]
    pub count: usize,
}

/// Runs PBF import.
#[expect(
    clippy::print_stdout,
    reason = "CLI import progress is intentionally written to stdout"
)]
pub fn run_import(args: ImportArgs) -> Result<(), CoreError> {
    let manifest = import_pbf(&args.pbf, &args.data_dir)?;
    println!(
        "imported {} places into {}",
        manifest.place_count,
        args.data_dir.display()
    );
    Ok(())
}

/// Serves the HTTP API.
pub fn run_serve(args: ServeArgs) -> Result<(), CoreError> {
    let engine = Engine::open(EngineConfig::new(&args.data_dir))?;
    let rt = Runtime::new().map_err(|e| CoreError::io(e.to_string()))?;
    rt.block_on(api::serve(engine, args.bind))
}

/// Runs a forward search and prints JSON.
pub fn run_search(args: SearchArgs) -> Result<(), CoreError> {
    let engine = Engine::open(EngineConfig::new(&args.data_dir))?;
    let query = SearchQuery::new(args.query, Some(args.limit))?;
    let hits = engine.geocode(&query)?;
    write_json(&hits)
}

/// Runs a reverse lookup and prints JSON.
pub fn run_reverse(args: ReverseArgs) -> Result<(), CoreError> {
    let engine = Engine::open(EngineConfig::new(&args.data_dir))?;
    let query = ReverseQuery::new(args.lat, args.lon, Some(args.limit))?;
    let hits = engine.reverse(&query)?;
    write_json(&hits)
}

/// Runs a JSONL batch and prints a JSON response.
pub fn run_batch(args: BatchArgs) -> Result<(), CoreError> {
    let engine = Engine::open(EngineConfig::new(&args.data_dir))?;
    let items = read_batch_items(&args.file)?;
    let response = engine.batch(&BatchRequest { items })?;
    write_json(&response)
}

/// Times reverse lookups and prints a summary.
#[expect(
    clippy::print_stdout,
    reason = "CLI bench summary is intentionally written to stdout"
)]
pub fn run_bench(args: BenchArgs) -> Result<(), CoreError> {
    let engine = Engine::open(EngineConfig::new(&args.data_dir))?;
    let report = bench_reverse(&engine, args.lat, args.lon, args.count)?;
    println!(
        "reverse_bulk count={} total_s={:.4} p50_ms={:.4} p99_ms={:.4} qps={:.1}",
        report.count, report.total_secs, report.p50_ms, report.p99_ms, report.qps
    );
    Ok(())
}

fn read_batch_items(path: &PathBuf) -> Result<Vec<BatchItem>, CoreError> {
    let reader: Box<dyn BufRead> = if path.as_os_str() == "-" {
        Box::new(io::BufReader::new(io::stdin()))
    } else {
        Box::new(io::BufReader::new(fs::File::open(path)?))
    };
    let mut items = Vec::new();
    for (lineno, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let item: BatchItem = serde_json::from_str(trimmed)
            .map_err(|e| CoreError::invalid(format!("batch line {}: {e}", lineno + 1)))?;
        items.push(item);
    }
    if items.is_empty() {
        return Err(CoreError::invalid("batch file contained no items"));
    }
    Ok(items)
}

/// Writes JSON to stdout; ignores broken pipes from downstream pagers.
fn write_json(value: &impl serde::Serialize) -> Result<(), CoreError> {
    let mut out = io::stdout().lock();
    match serde_json::to_writer_pretty(&mut out, value) {
        Ok(()) => {}
        Err(err) if err.io_error_kind() == Some(ErrorKind::BrokenPipe) => return Ok(()),
        Err(err) => return Err(CoreError::io(err.to_string())),
    }
    match writeln!(out) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(CoreError::io(err.to_string())),
    }
}
