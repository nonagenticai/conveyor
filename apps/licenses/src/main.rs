#![forbid(unsafe_code)]
//! Binary entry point: `serve`, `migrate`, plus `--print-openapi` / `--check-lock`.
//!
//! The document flags run BEFORE configuration is read and never touch a database: `--check-lock`
//! must be runnable in a bare workspace (and in CI) with no environment at all.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use conveyor_licenses::SERVICE_NAME;
use conveyor_licenses::auth::JwtVerifier;
use conveyor_licenses::config::Config;
use conveyor_licenses::contract;
use conveyor_licenses::db;
use conveyor_licenses::router::{self, ROUTES};
use conveyor_licenses::state::AppState;

#[derive(Debug, Parser)]
#[command(
    name = "conveyor-licenses",
    version,
    about = "The licensing service: tenants, licences, apps and tokens."
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
        /// Stamp `_sqlx_migrations` on a database already at the Python service's head; runs no
        /// DDL, and refuses a database that is not at that head.
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
    let port = config.port;
    let environment = config.environment.clone();
    let version = config.version.clone();
    let database_url = config.database_url()?.to_owned();
    conveyor_platform::telemetry::init(SERVICE_NAME);
    let pool = db::connect(&database_url, 5).await?;
    if config.trusted_issuers.is_empty() {
        // Fail closed and say why: with no allowlist every JWT is refused, which is the
        // reference's behaviour but must not be mistaken for "auth is off".
        tracing::warn!(
            "no trusted JWT issuer configured (KEYCLOAK_TRUSTED_ISSUERS / KEYCLOAK_URL unset): every JWT will be refused"
        );
    }
    let verifier = JwtVerifier::new(
        config.trusted_issuers.clone(),
        conveyor_licenses::auth::production_source(),
    );
    let state = AppState::new(pool, config, verifier);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!(port, %environment, %version, "serving");
    axum::serve(
        listener,
        router::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
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
    let url = config.database_url()?;
    let pool = db::connect(&url, 5).await?;
    if baseline {
        let stamped = db::baseline(&pool).await?;
        eprintln!(
            "baseline OK: {stamped} migration(s) stamped at {}",
            db::PYTHON_HEAD
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
