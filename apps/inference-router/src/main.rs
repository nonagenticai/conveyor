#![forbid(unsafe_code)]
//! Binary entry point: `serve`, plus `--print-openapi` / `--check-lock`.
//!
//! The document flags run BEFORE configuration is read and never touch a database: `--check-lock`
//! must be runnable in a bare workspace (and in CI) with no environment at all.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use conveyor_inference_router::SERVICE_NAME;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::Config;
use conveyor_inference_router::contract;
use conveyor_inference_router::db;
use conveyor_inference_router::router::{self, ROUTES};
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::{AppDeps, AppState};

#[derive(Debug, Parser)]
#[command(
    name = "conveyor-inference-router",
    version,
    about = "OpenAI-compatible inference router: the captured contract, provider routing and the inference ledger."
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
    /// Serve HTTP. Never migrates implicitly.
    Serve,
    /// Apply the embedded migrations to a fresh database.
    Migrate {
        /// Stamp `_sqlx_migrations` on a database already at the Python service's alembic head;
        /// runs no DDL, and refuses a database that is not at that head.
        #[arg(long)]
        baseline: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.print_openapi || cli.check_lock {
        return document(&cli);
    }
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(),
        Command::Migrate { baseline } => migrate(baseline),
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
    // The service cannot serve without its own database. Refuse at startup, not at the first
    // request; the pool this guard protects is opened immediately below.
    let database_url = config.database_url()?.to_owned();
    conveyor_platform::telemetry::init(SERVICE_NAME);

    // The catalog is deployment config and its refusals are part of the contract: an ambiguous
    // alias or a missing owner-hardware credential fails the boot, exactly as it does live.
    let catalog = Catalog::load(&config.providers_config_path)?;
    let slot_wait = std::time::Duration::from_secs_f64(config.provider_slot_wait_seconds);
    let router_service = RouterService::new(catalog, slot_wait)?;
    let pool = db::connect(&database_url, 5).await?;
    tracing::info!(
        environment = %config.environment,
        providers = %config.providers_config_path.display(),
        docs = config.docs_enabled,
        "serving"
    );
    // The budget gate and the cost sink are wired to their real implementations by the slices
    // that add them; until then the defaults are explicit rather than absent.
    let deps = AppDeps {
        ledger: Some(pool),
        ..AppDeps::default()
    };
    let state = AppState::with_deps(config, router_service, deps);
    let listener = tokio::net::TcpListener::bind(state.config().bind).await?;
    axum::serve(listener, router::router(state)).await?;
    Ok(())
}

fn migrate(baseline: bool) -> ExitCode {
    match migrate_inner(baseline) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[tokio::main]
async fn migrate_inner(baseline: bool) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let url = config.database_url()?.to_owned();
    let pool = db::connect(&url, 5).await?;
    if baseline {
        let stamped = db::baseline(&pool).await?;
        eprintln!(
            "baseline OK: {stamped} migration(s) stamped at {}",
            db::ALEMBIC_HEAD
        );
    } else {
        db::migrate(&pool).await?;
        eprintln!(
            "migrate OK: {} migration(s) embedded",
            db::MIGRATOR.iter().count()
        );
    }
    Ok(())
}
