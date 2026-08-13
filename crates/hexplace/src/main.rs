//! Hexplace command-line entry point.

#![forbid(unsafe_code)]

use std::process::ExitCode;

use clap::Parser;
use hexplace::cli::{Cli, Command};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let result = match cli.command {
        Command::Import(args) => hexplace::cli::run_import(args),
        Command::Serve(args) => hexplace::cli::run_serve(args),
        Command::Search(args) => hexplace::cli::run_search(args),
        Command::Reverse(args) => hexplace::cli::run_reverse(args),
        Command::Batch(args) => hexplace::cli::run_batch(args),
        Command::Bench(args) => hexplace::cli::run_bench(args),
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
