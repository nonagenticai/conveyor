#![forbid(unsafe_code)]
//! Binary entry point: `serve`, `migrate`, plus `--print-openapi` / `--check-lock`.
//!
//! The document flags run BEFORE configuration is read and never touch a database: `--check-lock`
//! must be runnable in a bare workspace (and in CI) with no environment at all.

use std::ffi::OsString;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use conveyor_licenses::SERVICE_NAME;
use conveyor_licenses::auth::JwtVerifier;
use conveyor_licenses::commands::{
    expire_stale, reap_expired, reconcile_metering, revoke_expired_tokens,
};
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
    /// Revoke time-expired-but-still-unrevoked tokens across the estate: env-configured, one JSON
    /// line on stdout, exit 0 converged / 1 not converged / 2 config-or-DB failure.
    RevokeExpiredTokens,
    /// Expire time-expired-but-still-active licences across the estate: env-configured, one JSON
    /// line on stdout, exit 0 converged / 1 not converged / 2 config-or-DB failure.
    ExpireStale,
    /// Archive terminally-expired licences to `revoked` and tear down their billing mirrors
    /// across the estate: env-configured, one JSON line on stdout, exit 0 converged / 1 not
    /// converged / 2 config-or-DB failure.
    ReapExpired,
    /// Measure each live licence's metering coverage against billing and repair it: env-configured,
    /// one JSON line on stdout, exit 0 converged / 1 not converged / 2 config-or-transport failure.
    ReconcileMetering,
}

/// The four CronJob commands and the argv contract of the reference scripts.
///
/// Those scripts are invoked by their CronJobs with NO arguments, and their contract is "exactly
/// one JSON line on stdout, on EVERY path": three of the four `main_cli`s inspect `sys.argv` and
/// refuse ANY argument — a usage line on stderr plus `_fail(None, …)` on stdout, exit 2 — while
/// `revoke_expired_tokens.py` never looks at `sys.argv` at all, so extra arguments are IGNORED and
/// the sweep runs. clap, by contrast, answers an extra argument with a usage error on stderr and
/// NO stdout line, leaving the audit trail (`kubectl logs`) empty. This table is therefore
/// applied BEFORE clap.
struct JobArgv {
    /// The clap subcommand spelling.
    command: &'static str,
    /// `usage: <program> (configured via <env> env)`. The reference prints the script's basename,
    /// which is its own invocation; in this binary that is the program and the subcommand.
    program: &'static str,
    env: &'static str,
    event: &'static str,
    error: &'static str,
    /// `revoke_expired_tokens.py:main_cli` never inspects `argv`.
    ignores_arguments: bool,
    /// `reconcile_metering.py::_fail` carries a `license_id` key; the other two `_fail`s do not.
    license_id: bool,
}

const JOB_ARGV: [JobArgv; 4] = [
    JobArgv {
        command: "expire-stale",
        program: "conveyor-licenses expire-stale",
        env: "EXPIRE_*",
        event: "expire_stale_licenses",
        error: "this job takes no arguments; configure it via EXPIRE_* env",
        ignores_arguments: false,
        license_id: false,
    },
    JobArgv {
        command: "reap-expired",
        program: "conveyor-licenses reap-expired",
        env: "REAP_*",
        event: "reap_expired_licenses",
        error: "this job takes no arguments; configure it via REAP_* env",
        ignores_arguments: false,
        license_id: false,
    },
    JobArgv {
        command: "reconcile-metering",
        program: "conveyor-licenses reconcile-metering",
        env: "RECONCILE_*",
        event: "metering_reconcile",
        error: "this job takes no arguments; configure it via RECONCILE_* env",
        ignores_arguments: false,
        license_id: true,
    },
    JobArgv {
        command: "revoke-expired-tokens",
        program: "conveyor-licenses revoke-expired-tokens",
        env: "REVOKE_*",
        event: "revoke_expired_tokens",
        error: "this job takes no arguments; configure it via REVOKE_* env",
        ignores_arguments: true,
        license_id: false,
    },
];

/// Parse the process's own argv: the reference scripts' argv contract first, then clap.
///
/// A no-argument invocation — the CronJob path — and every non-CronJob command (`serve`,
/// `migrate`, `--print-openapi`, `--check-lock`) reach clap with the process's own argv,
/// byte-identical to before. Only a CronJob command followed by anything else is intercepted.
fn parse_cli() -> Result<Cli, ExitCode> {
    let args: Vec<OsString> = std::env::args_os().collect();
    let mut index = 1; // argv[0] is the program
    while index < args.len() {
        let arg = args[index].as_os_str();
        // The document flags are global and may precede the subcommand; they keep their meaning.
        if arg == "--print-openapi" || arg == "--check-lock" {
            index += 1;
            continue;
        }
        let Some(job) = JOB_ARGV.iter().find(|job| arg == job.command) else {
            return Ok(Cli::parse());
        };
        if index + 1 == args.len() {
            // The CronJob path: NOT ONE argument after the subcommand.
            return Ok(Cli::parse());
        }
        if !job.ignores_arguments {
            eprintln!("usage: {} (configured via {} env)", job.program, job.env);
            return Err(arguments_refused(job));
        }
        // `revoke_expired_tokens.py` never inspects `sys.argv`, so extra tokens are dropped and
        // the job runs. A flag is the one thing that still means something to THIS binary, so it
        // is handed to clap rather than silently swallowed.
        let kept: Vec<OsString> = args[..=index]
            .iter()
            .chain(
                args[index + 1..]
                    .iter()
                    .filter(|arg| arg.as_os_str().to_string_lossy().starts_with('-')),
            )
            .cloned()
            .collect();
        return Ok(Cli::parse_from(kept));
    }
    Ok(Cli::parse())
}

/// The reference's `_fail(None, …)` for a command invoked WITH arguments: exactly one JSON line
/// on stdout, keys sorted as `json.dumps(..., sort_keys=True)` sorts them, exit 2.
fn arguments_refused(job: &JobArgv) -> ExitCode {
    let mut payload = serde_json::json!({
        "event": job.event,
        "actor": serde_json::Value::Null,
        "error": job.error,
        "exit_code": 2,
    });
    if job.license_id {
        payload["license_id"] = serde_json::Value::Null;
    }
    match serde_json::to_string(&payload) {
        Ok(json) => {
            println!("{json}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("error: cannot serialise the outcome: {error}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let cli = match parse_cli() {
        Ok(cli) => cli,
        Err(code) => return code,
    };
    if cli.print_openapi || cli.check_lock {
        return document(&cli);
    }
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(),
        Command::Migrate { baseline } => migrate(baseline),
        Command::RevokeExpiredTokens => revoke_expired_tokens_cli(),
        Command::ExpireStale => expire_stale_cli(),
        Command::ReapExpired => reap_expired_cli(),
        Command::ReconcileMetering => reconcile_metering_cli(),
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
    let mut state = AppState::new(pool, config, verifier);
    if let Some(billing) = conveyor_licenses::billing::BillingTeardown::from_env() {
        state = state.with_billing(std::sync::Arc::new(billing));
    } else {
        tracing::warn!(
            "METERING_SERVICE_URL unset or unusable: revoked licences will not tear down their \
             billing mirrors; the sync-from-licenses sweep remains the backstop"
        );
    }
    // One metering client, two readers: the user-licence and bundle-ensure routes read it from
    // the state; `POST /licenses/admin/metering/reconcile` (which runs the same sweep as the
    // CronJob command) reads it from an extension and answers 500 — never a silent no-op —
    // when billing is not configured.
    let metering = conveyor_licenses::billing::metering::MeteringProvisioner::from_env()
        .map(std::sync::Arc::new);
    match &metering {
        Some(provisioner) => state = state.with_metering(provisioner.clone()),
        None => tracing::warn!(
            "METERING_SERVICE_URL unset or unusable: created licences will not be metered, \
             bundle-ensure inserts are recorded metering-pending, and \
             POST /licenses/admin/metering/reconcile answers 500; POST {} remains the repair path",
            conveyor_licenses::billing::metering::METERING_RECONCILE_PATH
        ),
    }
    let mut app = router::router(state);
    if let Some(provisioner) = metering {
        app = app.layer(axum::Extension(provisioner));
    }
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!(port, %environment, %version, "serving");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
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

/// `revoke-expired-tokens`: the CronJob's contract is one JSON line on stdout on EVERY path
/// (config failures included), with the verdict carried by the exit code.
fn revoke_expired_tokens_cli() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: cannot start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = runtime.block_on(revoke_expired_tokens::run(
        std::env::var("DATABASE_URL").ok(),
    ));
    // serde_json's object map is a BTreeMap here (no `preserve_order`), so keys come out sorted,
    // exactly like the script's `json.dumps(..., sort_keys=True)`.
    match serde_json::to_string(&outcome.payload) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("error: cannot serialise the outcome: {error}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::from(u8::try_from(outcome.code).unwrap_or(1))
}

/// `expire-stale`: the same CronJob contract as `revoke-expired-tokens` — one JSON line on
/// stdout on EVERY path (config failures included), with the verdict carried by the exit code.
fn expire_stale_cli() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: cannot start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = runtime.block_on(expire_stale::run(std::env::var("DATABASE_URL").ok()));
    // serde_json's object map is a BTreeMap here (no `preserve_order`), so keys come out sorted,
    // exactly like the script's `json.dumps(..., sort_keys=True)`.
    match serde_json::to_string(&outcome.payload) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("error: cannot serialise the outcome: {error}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::from(u8::try_from(outcome.code).unwrap_or(1))
}

/// `reconcile-metering`: the same CronJob contract as `reap-expired` — one JSON line on stdout
/// on EVERY path (config failures included), with the verdict carried by the exit code.
fn reconcile_metering_cli() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: cannot start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = runtime.block_on(reconcile_metering::run(std::env::var("DATABASE_URL").ok()));
    // serde_json's object map is a BTreeMap here (no `preserve_order`), so keys come out sorted,
    // exactly like the script's `json.dumps(..., sort_keys=True)`.
    match serde_json::to_string(&outcome.payload) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("error: cannot serialise the outcome: {error}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::from(u8::try_from(outcome.code).unwrap_or(1))
}

/// `reap-expired`: the same CronJob contract as `expire-stale` — one JSON line on stdout on
/// EVERY path (config failures included), with the verdict carried by the exit code.
fn reap_expired_cli() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: cannot start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let outcome = runtime.block_on(reap_expired::run(std::env::var("DATABASE_URL").ok()));
    // serde_json's object map is a BTreeMap here (no `preserve_order`), so keys come out sorted,
    // exactly like the script's `json.dumps(..., sort_keys=True)`.
    match serde_json::to_string(&outcome.payload) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("error: cannot serialise the outcome: {error}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::from(u8::try_from(outcome.code).unwrap_or(1))
}
