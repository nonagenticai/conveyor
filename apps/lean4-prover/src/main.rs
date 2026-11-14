//! Process entry point: `serve`, plus `--print-openapi` / `--check-lock`.
//!
//! All behaviour lives in the library so the HTTP contract can be exercised by
//! `tests/` without booting a process. The document flags run BEFORE the
//! environment is read and never touch the network or a Lean subprocess, so
//! `--check-lock` runs in a bare workspace with no configuration at all.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use conveyor_lean4_prover::admission::Gate;
use conveyor_lean4_prover::auth::LicenceClient;
use conveyor_lean4_prover::config::Config;
use conveyor_lean4_prover::contract;
use conveyor_lean4_prover::metrics::Metrics;
use conveyor_lean4_prover::routes::{self, ROUTES};
use conveyor_lean4_prover::runner::LeanRunner;
use conveyor_lean4_prover::state::AppState;

#[derive(Debug, Parser)]
#[command(
    name = "conveyor-lean4-prover",
    version,
    about = "Lean 4 formal verification as a REST API."
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
            // Also to stderr, not only tracing: a startup refusal must be visible even when the
            // log subscriber is not configured.
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn document(cli: &Cli) -> ExitCode {
    let value = routes::openapi();
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

    let cfg = match Config::from_env() {
        Ok(cfg) => Arc::new(cfg),
        Err(err) => {
            // Refuse to boot on a bad configuration, like the pydantic model
            // did. A lane limit or timeout nobody set is worse than a crashloop.
            eprintln!("lean4-prover: configuration error: {err}");
            std::process::exit(1);
        }
    };

    let metrics = Arc::new(Metrics::default());
    let gate = Arc::new(Gate::new(
        cfg.lane_max_active,
        cfg.lane_max_waiting,
        cfg.lane_max_wait_s,
        Arc::clone(&metrics),
    ));

    let licence = if cfg.enable_auth {
        let client = LicenceClient::new(
            cfg.license_introspect_url.clone(),
            Duration::from_secs(cfg.license_introspect_timeout_s),
        )?;
        tracing::info!(url = %cfg.license_introspect_url, "licence authentication enabled");
        Some(Arc::new(client))
    } else {
        tracing::warn!("Authentication middleware DISABLED (ENABLE_AUTH=false)");
        None
    };

    let addr = format!("{}:{}", cfg.bind_host, cfg.bind_port);
    let state = AppState {
        runner: LeanRunner::new(Arc::clone(&cfg), Arc::clone(&metrics)),
        cfg,
        gate,
        metrics,
        licence,
    };
    let app = conveyor_lean4_prover::build_app(state.clone());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(
        %addr,
        version = %state.cfg.version,
        max_active = state.cfg.lane_max_active,
        max_waiting = state.cfg.lane_max_waiting,
        "lean4-prover ready"
    );

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
