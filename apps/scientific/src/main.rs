#![forbid(unsafe_code)]
//! Binary entry point: `serve`, plus `--print-openapi` / `--check-lock`.
//!
//! The document flags run BEFORE configuration is read and never touch the network or a database:
//! `--check-lock` must be runnable in a bare workspace (and in CI) with no environment at all.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use conveyor_scientific::SERVICE_NAME;
use conveyor_scientific::config::Config;
use conveyor_scientific::contract;
use conveyor_scientific::router::{self, ROUTES};
use conveyor_scientific::state::{AppState, Shared};

#[derive(Debug, Parser)]
#[command(
    name = "conveyor-scientific",
    version,
    about = "The evidence tools: the captured scientific-processor contract, statistics and cache."
)]
struct Cli {
    /// Print the served OpenAPI document as JSON to stdout and exit.
    #[arg(long, global = true)]
    print_openapi: bool,

    /// Exit non-zero when the served routes or document drift from the captured contract.
    #[arg(long, global = true)]
    check_lock: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve HTTP.
    Serve,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.print_openapi || cli.check_lock {
        return document(&cli);
    }
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(),
    }
}

fn document(cli: &Cli) -> ExitCode {
    let value = match serde_json::to_value(router::openapi()) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("error: cannot serialise OpenAPI: {error}");
            return ExitCode::FAILURE;
        }
    };
    if cli.check_lock {
        match contract::check(&value, ROUTES) {
            Ok(()) => eprintln!("lock OK: {} operations", ROUTES.len()),
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    if cli.print_openapi {
        match serde_json::to_string_pretty(&value) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("error: cannot serialise OpenAPI: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn serve() -> ExitCode {
    match serve_inner() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Also to stderr, not only tracing: a startup refusal must be visible even when the
            // log subscriber is not configured.
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[tokio::main]
async fn serve_inner() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    conveyor_platform::telemetry::init(SERVICE_NAME);
    let bind = config.bind;
    let state: Shared = std::sync::Arc::new(AppState::new(config));
    let app = router::router(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(service = SERVICE_NAME, %bind, "serving");
    axum::serve(listener, app).await?;
    Ok(())
}
