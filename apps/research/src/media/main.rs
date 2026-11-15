//! `conveyor-research-media`: the `media-discovery` half of `apps/research`.
//!
//! `serve` runs the service; `--check-lock` compares the routed operations with the captured
//! contract and exits non-zero while any captured operation has no route.

use clap::Parser;
use conveyor_research::media::{self, Config};

#[derive(Parser, Debug)]
#[command(name = "conveyor-research-media", version, about)]
struct Cli {
    /// Bind address (default 0.0.0.0).
    #[arg(long)]
    host: Option<String>,

    /// Bind port (default 8809).
    #[arg(long)]
    port: Option<u16>,

    /// Compare the routed operations with the captured contract and exit.
    #[arg(long)]
    check_lock: bool,

    /// Print the served OpenAPI document and exit.
    #[arg(long)]
    print_openapi: bool,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    let mut config = Config::from_env();
    if let Some(host) = cli.host {
        config.bind_host = host;
    }
    if let Some(port) = cli.port {
        config.bind_port = port;
    }

    if cli.check_lock {
        return match media::contract::check(media::SERVED) {
            Ok(()) => {
                println!(
                    "lock: OK ({} operations served, the capture has {})",
                    media::SERVED.len(),
                    media::contract::operations(&media::openapi()).len()
                );
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("lock: DRIFTED — {error}");
                std::process::ExitCode::FAILURE
            }
        };
    }

    if cli.print_openapi {
        println!(
            "{}",
            serde_json::to_string_pretty(&media::openapi()).unwrap_or_default()
        );
        return std::process::ExitCode::SUCCESS;
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("cannot start the runtime: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };

    runtime.block_on(async move {
        // A boot with no database refuses to start rather than serving 500s: every one of the 16
        // data-backed operations would otherwise be a route that can only fail.
        let Some(database_url) = config.database_url.clone() else {
            eprintln!("DATABASE_URL is not set: media-discovery owns its own database and cannot serve without it");
            return std::process::ExitCode::FAILURE;
        };
        let pool = match media::db::connect(&database_url, config.max_connections).await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("cannot connect to the database: {error}");
                return std::process::ExitCode::FAILURE;
            }
        };
        if let Err(error) = media::db::migrate(&pool).await {
            eprintln!("cannot apply the migrations: {error}");
            return std::process::ExitCode::FAILURE;
        }

        let app = media::build_app(media::AppState::with_pool(config.clone(), pool));
        let listener = match tokio::net::TcpListener::bind(config.address()).await {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("cannot bind {}: {error}", config.address());
                return std::process::ExitCode::FAILURE;
            }
        };
        println!(
            "{} {} on {}",
            config.service_name,
            config.version,
            config.address()
        );
        if let Err(error) = axum::serve(listener, app).await {
            eprintln!("serve failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
        std::process::ExitCode::SUCCESS
    })
}
