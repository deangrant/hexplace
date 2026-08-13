//! Geofind command-line entry point.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::Parser;
use geofind::cli::{Cli, Command};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let result = match cli.command {
        Command::Import(args) => geofind::cli::run_import(args),
        Command::Serve(args) => geofind::cli::run_serve(args),
        Command::Search(args) => geofind::cli::run_search(args),
        Command::Reverse(args) => geofind::cli::run_reverse(args),
        Command::Batch(args) => geofind::cli::run_batch(args),
        Command::Bench(args) => geofind::cli::run_bench(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("{err}");
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
