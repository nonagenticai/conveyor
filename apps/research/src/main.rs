//! Process entry point: `serve`, plus `--print-openapi` / `--check-lock`.
//!
//! The document flags run before the environment is read and never touch the
//! network, so `--check-lock` works in a bare workspace with no configuration.

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use conveyor_research::contract;
use conveyor_research::router::{self, AppState, Config, ROUTES};

#[derive(Debug, Parser)]
#[command(
    name = "conveyor-research",
    version,
    about = "The captured webscraper contract as a static-page scraping service."
)]
struct Cli {
    /// Print the served OpenAPI document as JSON to stdout and exit.
    #[arg(long, global = true)]
    print_openapi: bool,

    /// Exit non-zero when the served document drifts from the captured contract.
    #[arg(long, global = true)]
    check_lock: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.print_openapi || cli.check_lock {
        return document(&cli);
    }
    match serve() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Also to stderr, not only tracing: a startup refusal must be
            // visible even when the log subscriber is not configured.
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn document(cli: &Cli) -> ExitCode {
    let value = router::openapi();
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

#[tokio::main]
async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    let config = Config::from_env();
    let address = config.address();
    let app = router::build_app(AppState::new(config));

    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!(%address, "conveyor-research ready");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
