#![forbid(unsafe_code)]
//! Binary entry point: `serve | migrate | worker | admin`, plus `--print-openapi`.
//!
//! `--print-openapi` writes the merged document and exits BEFORE reading configuration or
//! touching a database (CONTRACT §16), which is why the parity tooling can run it in a bare
//! workspace. `--write-lock` / `--check-lock` wrap it with the served-spec lock.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use conveyor_actions::registry::{assert_registry_consistent, registry};
use conveyor_actions::spec::HandlerRef;
use conveyor_api::{AppState, Config, ConfigError, app, openapi};
use conveyor_store as store;
use sqlx::PgPool;
use tokio::net::TcpListener;
use uuid::Uuid;

use conveyor_server::{admin, bus_relay, export, seed, shutdown, snapshot, telemetry, worker};

#[derive(Debug, Parser)]
#[command(
    name = "conveyor",
    version,
    about = "conveyor — tenant-isolated board, governance and processes service"
)]
struct Cli {
    /// Print the merged OpenAPI document as JSON to stdout and exit.
    #[arg(long, global = true)]
    print_openapi: bool,

    /// Regenerate served-spec.lock.json from the current document (with --print-openapi).
    #[arg(long, global = true)]
    write_lock: bool,

    /// Exit 1 when the served document drifts from served-spec.lock.json.
    #[arg(long, global = true)]
    check_lock: bool,

    /// The lock file to write or check.
    #[arg(long, global = true, default_value = "served-spec.lock.json")]
    lock_path: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve HTTP. Never migrates implicitly (CONTRACT §10).
    Serve,
    /// Apply migrations as the schema owner, then re-apply the runtime grants.
    Migrate {
        /// Insert the dev tenants `dev-a`/`dev-b` with one user each, and their active
        /// app_licence rows. Refused unless APP_ENV is an explicit development/test value
        /// (`development`, `dev`, `test`, `local`); anything else, including `Production`, is
        /// treated as production.
        #[arg(long)]
        dev_seed: bool,
        /// Import the checked-in catalogue export (`seed/catalogue.json`) and the embedded
        /// benchmark suite (`seed/benchmark/verified50-v1.json`) into the target tenants:
        /// card types first, then the entries, then the suite, one transaction per tenant.
        #[arg(long)]
        seed: bool,
        /// The tenant to seed. Without it `--seed` imports into every active tenant.
        /// `--process-catalogue` requires it explicitly: there is no universal default catalogue.
        #[arg(long)]
        tenant: Option<String>,
        /// Install the default process catalogue (`seed/processes/**`) into ONE tenant.
        ///
        /// ⛔ There is NO universal default. A tenant may run accounting, marketing, software
        /// development or project management with human members, so the catalogue is content a
        /// tenant CHOOSES — never something a new tenant is born with. It therefore requires an
        /// explicit `--tenant` and never rides `--seed`.
        #[arg(long, requires = "tenant", conflicts_with = "seed")]
        process_catalogue: bool,
    },
    /// Run the job poller: schedules, claims, fenced finishes (S02b).
    Worker,
    /// Write the public design-site snapshot for one tenant (read-only).
    ///
    /// Deterministic JSON + `snapshot.js` under `--out`; no secret value, email or key id is
    /// ever selected. `--generated-at` pins the timestamp so a re-run is byte-identical.
    Snapshot {
        /// The tenant slug to snapshot.
        #[arg(long)]
        tenant: String,
        /// The output directory.
        #[arg(long, default_value = "docs/design/data")]
        out: PathBuf,
        /// A fixed RFC 3339 timestamp (byte-identical re-runs). Defaults to now.
        #[arg(long)]
        generated_at: Option<String>,
        /// The committed `gh issue list` registry.
        #[arg(long, default_value = "docs/design/data/issues.json")]
        issues: PathBuf,
        /// The feedback surface linked from every studio page.
        #[arg(long, default_value = conveyor_server::snapshot::DEFAULT_FEEDBACK_URL)]
        feedback_url: String,
        /// The read-only database URL. Defaults to `DATABASE_URL`.
        #[arg(long)]
        database_url: Option<String>,
        /// The commit the published pack describes. Defaults to `git rev-parse HEAD` in the
        /// working directory; a pod has no checkout, so a publish passes it explicitly.
        #[arg(long)]
        git_sha: Option<String>,
    },
    /// Administrative operations (S14; S04 owns the action-registry commands).
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AdminCommand {
    /// Action-registry operations.
    Actions {
        #[command(subcommand)]
        command: ActionsCommand,
    },
    /// Write a board-export/v1 fixture from a seed. Touches no database.
    Fixture {
        #[command(subcommand)]
        command: FixtureCommand,
    },
    /// Replay a board-export/v1 document through the real actions.
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
    /// Re-read a board as board-export/v1.
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
    /// Compare two board-export/v1 documents; non-zero exit on any content delta.
    Diff {
        #[command(subcommand)]
        command: DiffCommand,
    },
    /// Compare two JSONL traces: exit 0 identical, 1 delta, 2 unparseable. Touches no database.
    TraceDiff {
        /// The old (recorded) trace file. Required unless `--expect` is given.
        #[arg(long, required_unless_present = "expect", conflicts_with = "expect")]
        old: Option<PathBuf>,
        /// The new (exported) trace file.
        #[arg(long)]
        new: PathBuf,
        /// A reviewed expectation (`fixtures/<x>.expected.json`) — the ordered effect path a human
        /// read out of the recording. Replaces `--old`; shares the migration comparison.
        #[arg(long)]
        expect: Option<PathBuf>,
        /// The recorded n8n workflow snapshot (`workflow_entity` id/name/versionId/nodes).
        ///
        /// With this flag `--old` is a raw recorded n8n execution (`execution_data.data`) and is
        /// projected into a trace first; without it `--old` is JSONL, like `--new`.
        #[arg(long, requires = "old")]
        old_workflow: Option<PathBuf>,
    },
    /// Export a PROCESS run's `run_step` trace, or diff it against a reviewed expectation.
    /// Exit 0 recorded/identical, 1 delta, 2 unreadable. S13's trace gate.
    ProcessTraceDiff {
        /// The process run id.
        #[arg(long)]
        run: Uuid,
        /// The tenant slug the run belongs to.
        #[arg(long)]
        tenant: String,
        /// The reviewed expectation file (`--write` records one).
        #[arg(long, conflicts_with = "write")]
        expect: Option<PathBuf>,
        /// Write the exported trace here (recording mode).
        #[arg(long, conflicts_with = "expect")]
        write: Option<PathBuf>,
    },
    /// Seal a tenant's BYO token for an upstream service (`laya`, `inference`). Never prints it.
    UpstreamToken {
        /// The tenant slug.
        #[arg(long)]
        tenant: String,
        /// The service whose bearer this token is.
        #[arg(long, value_enum)]
        service: admin::upstream_tokens::UpstreamService,
        /// Mint the token through the licences API (the tenant's per-app licence).
        #[arg(
            long,
            conflicts_with = "token_stdin",
            required_unless_present = "token_stdin"
        )]
        mint: bool,
        /// Read the tenant's own token (BYO) from stdin.
        #[arg(long, conflicts_with = "mint", required_unless_present = "mint")]
        token_stdin: bool,
    },
    /// Seal every pre-`0014` plaintext webhook signing secret under the tenant KEK. Idempotent.
    ResealWebhooks,
}

#[derive(Debug, Subcommand)]
enum FixtureCommand {
    /// `conveyor admin fixture board --cards 20 --lists 5 --seed 7 --out …`
    Board {
        /// Live cards to emit.
        #[arg(long, default_value_t = 20)]
        cards: usize,
        /// Lanes to emit; at least the template's five.
        #[arg(long, default_value_t = 5)]
        lists: usize,
        /// Comments distributed over the cards.
        #[arg(long, default_value_t = 0)]
        comments: usize,
        /// Checklist items distributed over the cards.
        #[arg(long, default_value_t = 0)]
        tasks: usize,
        /// Board fields; every card gets a value for each.
        #[arg(long, default_value_t = 0)]
        fields: usize,
        /// The deterministic seed.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Also write the document here (the document is always printed to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum ImportCommand {
    /// `conveyor admin import board --file F --tenant <tenant uuid>`
    Board {
        /// The board-export/v1 document.
        #[arg(long)]
        file: PathBuf,
        /// The tenant UUID (the tenant table has no RLS).
        #[arg(long)]
        tenant: uuid::Uuid,
    },
}

#[derive(Debug, Subcommand)]
enum ExportCommand {
    /// `conveyor admin export board --board <board uuid>`
    Board {
        /// The board id.
        #[arg(long)]
        board: uuid::Uuid,
    },
}

#[derive(Debug, Subcommand)]
enum DiffCommand {
    /// `conveyor admin diff board a.json b.json`
    Board {
        /// The left document (the fixture).
        left: PathBuf,
        /// The right document (the re-export).
        right: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ActionsCommand {
    /// Ingest a service's openapi.json. Exit 2 when a refresh is breaking.
    Ingest {
        /// The service document.
        #[arg(long)]
        file: PathBuf,
        /// The service key (`fixtures`).
        #[arg(long)]
        service: String,
        /// The tenant slug whose registry is written.
        #[arg(long)]
        tenant: String,
        /// Approve a breaking refresh instead of writing it unapproved.
        #[arg(long)]
        approve: bool,
    },
    /// Approve one ingested spec version.
    Approve {
        /// `<resource>.<verb>`.
        name: String,
        /// The spec version to enable.
        #[arg(long)]
        version: i32,
        /// The tenant slug.
        #[arg(long)]
        tenant: String,
    },
}

#[derive(Debug, thiserror::Error)]
enum RunError {
    #[error("config: {0}")]
    Config(#[from] ConfigError),
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migrate: {0}")]
    Migrate(#[from] store::StoreMigrateError),
    #[error("seed: {0}")]
    Seed(#[from] seed::SeedError),
    #[error("state: {0}")]
    State(#[from] conveyor_api::state::AppStateError),
    #[error("worker: {0}")]
    Worker(#[from] worker::WorkerError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("registry: {0}")]
    Registry(#[from] conveyor_actions::RegistryError),
    #[error("ingest: {0}")]
    Ingest(#[from] conveyor_actions::ingest::IngestError),
    #[error("{0}")]
    Refused(String),
    #[error("export: {0}")]
    Export(#[from] export::ExportError),
    #[error("snapshot: {0}")]
    Snapshot(#[from] snapshot::SnapshotError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// A documented non-zero exit that is not a failure of the process (ingest: 2).
    #[error("{1}")]
    Exit(u8, String),
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.print_openapi {
        return print_openapi_document(&cli);
    }
    telemetry::init_tracing();
    match run(cli.command.unwrap_or(Command::Serve)).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(RunError::Exit(code, message)) => {
            eprintln!("error: {message}");
            ExitCode::from(code)
        }
        Err(error) => {
            // Also to stderr, not only tracing: a startup refusal must be visible even when
            // the log subscriber is not configured.
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_openapi_document(cli: &Cli) -> ExitCode {
    let value = openapi();
    if cli.write_lock {
        let lock = conveyor_actions::lock::build(&value);
        if let Err(error) = conveyor_actions::lock::write(&cli.lock_path, &lock) {
            eprintln!("error: cannot write {}: {error}", cli.lock_path.display());
            return ExitCode::FAILURE;
        }
        eprintln!(
            "wrote {} ({} operations)",
            cli.lock_path.display(),
            lock.operations.len()
        );
    }
    if cli.check_lock {
        match conveyor_actions::lock::check(&value, &cli.lock_path) {
            Ok(_) => eprintln!("lock OK: {}", cli.lock_path.display()),
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    match serde_json::to_string_pretty(&value) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: cannot serialise OpenAPI: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(command: Command) -> Result<(), RunError> {
    match command {
        Command::Serve => serve().await,
        Command::Migrate {
            dev_seed,
            seed: seed_catalogue,
            tenant,
            process_catalogue,
        } => migrate(dev_seed, seed_catalogue, tenant, process_catalogue).await,
        Command::Worker => worker().await,
        Command::Snapshot {
            tenant,
            out,
            generated_at,
            issues,
            feedback_url,
            database_url,
            git_sha,
        } => {
            let report = snapshot::run(snapshot::SnapshotArgs {
                tenant,
                out,
                generated_at,
                issues,
                feedback_url,
                database_url,
                git_sha,
            })
            .await?;
            tracing::info!(
                tenant = %report.tenant,
                out = %report.out.display(),
                files = report.files.len(),
                generated_at = %report.generated_at,
                manifest_sha256 = %report.manifest_sha256,
                "snapshot written"
            );
            Ok(())
        }
        Command::Admin { command } => admin(command).await,
    }
}

/// Serve. The registry is asserted BEFORE any connection is opened, so a duplicate, a
/// collision or a handler-less action exits 1 without a database and **before bind**.
async fn serve() -> Result<(), RunError> {
    assert_registry_consistent()?;
    let config = Config::from_env()?;
    // BEFORE opening a connection: a non-development APP_ENV with CONVEYOR_DEV_AUTH=1 must
    // exit non-zero without depending on a reachable database. `Config` normalises APP_ENV
    // (trim + ASCII lowercase) and `is_production` is a fail-closed allowlist.
    config.validate_auth()?;
    let bind_addr = config.bind_addr;
    let dev_auth = config.auth.dev_auth;

    let state = AppState::connect(config).await?;
    // A database that names an action this binary cannot execute is a boot failure, not a
    // runtime 500 on the first call. Still before bind.
    assert_db_registry(&state.pool).await?;
    // The outbox relay is SPAWNED, never awaited: a Redis that is down is logged and retried in
    // the background and must not delay the listener below. It shares the pool and the SSE
    // doorbell (`state.listen`), so there is one LISTEN connection and one set of cursors.
    bus_relay::spawn(state.pool.clone(), state.listen.subscribe());
    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(addr = %listener.local_addr()?, dev_auth, "listening");
    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown::signal())
        .await?;
    Ok(())
}

/// Every approved `action_registry` row must name a compiled action, and an external row's
/// service must have a configured base URL (S04 §5.2).
async fn assert_db_registry(pool: &PgPool) -> Result<(), RunError> {
    for tenant in store::tenants::active_ids(pool).await? {
        let mut tx = store::begin_scoped(pool, tenant).await?;
        let rows = store::actions::approved_registry(&mut tx).await?;
        tx.commit().await?;
        for row in rows {
            let Some(spec) = registry().get(&row.name) else {
                return Err(RunError::Refused(format!(
                    "the database names action {:?}, which this binary does not compile",
                    row.name
                )));
            };
            if let HandlerRef::External { service, .. } = spec.handler
                && conveyor_actions::invoke::service_url(service).is_none()
            {
                return Err(RunError::Refused(format!(
                    "action {:?} names service {:?}, which has no configured base URL",
                    row.name, row.service
                )));
            }
            if row.kind == "workflow" && !matches!(spec.handler, HandlerRef::Workflow) {
                return Err(RunError::Refused(format!(
                    "action {:?} is stored as a workflow but compiled without one",
                    row.name
                )));
            }
        }
    }
    Ok(())
}

async fn worker() -> Result<(), RunError> {
    assert_registry_consistent()?;
    let config = Config::from_env()?;
    // The worker authenticates nobody, so it does not apply the serving auth rules; it does
    // need the database and (for licence.sync) LICENCES_BASE_URL.
    let runtime = worker::Worker::connect(config).await?;
    // The bus consumer is the fast path for a `process.step`; the job arm inside `run` is the
    // fallback that a Redis loss cannot stop (CONTRACT §7: the bus is a nudge, never the record).
    // Both limbs call the same idempotent `execute_step`, and the consumer shares this pool.
    worker::spawn_bus_consumer(runtime.pool().clone(), runtime.app_key().to_owned());
    runtime.run().await?;
    Ok(())
}

async fn migrate(
    dev_seed: bool,
    seed_catalogue: bool,
    tenant: Option<String>,
    process_catalogue: bool,
) -> Result<(), RunError> {
    // A lone `--tenant` is a usage error: it names a target for one of the two imports.
    if tenant.is_some() && !seed_catalogue && !process_catalogue {
        return Err(RunError::Refused(
            "--tenant requires --seed or --process-catalogue".into(),
        ));
    }
    let config = Config::from_env()?;
    // Fail closed: only an explicit development/test APP_ENV may seed. The helper is the
    // same allowlist the serving auth check uses, applied to the already-normalised value.
    if dev_seed && !conveyor_auth::is_development_env(&config.app_env) {
        return Err(RunError::Refused(
            "migrate --dev-seed is refused unless APP_ENV is a development/test value \
             (development, dev, test, local)"
                .into(),
        ));
    }
    // The migrator takes a SESSION advisory lock: it must bypass pgbouncer.
    let pool = store::connect(&config.database_migrate_url).await?;
    store::migrate(&pool).await?;
    tracing::info!(
        migrations = store::MIGRATOR.iter().count(),
        "migrations applied and runtime grants re-applied"
    );
    if dev_seed {
        seed::dev(&pool).await?;
        tracing::info!("dev tenants dev-a and dev-b seeded");
    }
    // Every active tenant gets its maintenance schedule rows (idempotent). This runs after
    // the dev seed so a freshly seeded tenant is covered in the same command.
    let schedules = store::seed_defaults(&pool).await?;
    tracing::info!(inserted = schedules, "maintenance schedules ensured");
    // The full catalogue import runs after every other seed, so a tenant created by
    // `--dev-seed` in this same command is a valid target. Unlike `--dev-seed` it is not
    // environment-gated: the export is the production vocabulary.
    if seed_catalogue {
        let counts = seed::seed_catalogue(&pool, tenant.as_deref()).await?;
        for (slug, counts) in counts {
            tracing::info!(
                tenant = %slug,
                created = counts.created,
                updated = counts.updated,
                skipped = counts.skipped,
                "catalogue seeded"
            );
        }
        // S17's benchmark suite is part of the production vocabulary too: the hashed artifact
        // is embedded, and the import is idempotent by `seed_sha256` (CONTRACT §19).
        let benchmark = seed::seed_benchmark(&pool, tenant.as_deref()).await?;
        for (slug, created) in benchmark {
            tracing::info!(
                tenant = %slug,
                suites_created = created,
                "benchmark suite seeded"
            );
        }
        // The buildout programme: one programme and one project per current plan, with the
        // GitHub issue numbers linked from `project.meta.issues` (SPEC_V2 §5.1).
        let programme = seed::seed_programme(&pool, tenant.as_deref()).await?;
        for (slug, counts) in programme {
            tracing::info!(
                tenant = %slug,
                created = counts.created,
                updated = counts.updated,
                skipped = counts.skipped,
                "programme seeded"
            );
        }
        // The plan spine: one board per project and one card per plan unit, each card naming
        // its process. Runs after the programme (the projects exist) and inside the catalogue
        // block (the board template and card types exist).
        let plans = seed::seed_plans(&pool, tenant.as_deref()).await?;
        for (slug, counts) in plans {
            tracing::info!(
                tenant = %slug,
                created = counts.created,
                updated = counts.updated,
                skipped = counts.skipped,
                "plan cards seeded"
            );
        }
    }
    // ⛔ NEVER part of `--seed`: a tenant's processes are content it chooses (accounting,
    // marketing, software development, …), not a universal default. This installs the shipped
    // software-development catalogue into ONE named tenant that asked for it.
    if process_catalogue {
        let Some(slug) = tenant.as_deref() else {
            return Err(RunError::Refused(
                "--process-catalogue requires --tenant <slug>".into(),
            ));
        };
        let counts = seed::seed_process_catalogue(&pool, Some(slug)).await?;
        for (slug, counts) in counts {
            tracing::info!(
                tenant = %slug,
                agents = counts.agents,
                sops = counts.sops,
                processes = counts.processes,
                "process catalogue installed for this tenant"
            );
        }
    }
    pool.close().await;
    Ok(())
}

async fn admin(command: AdminCommand) -> Result<(), RunError> {
    // `fixture` and `diff` are pure file operations: they must run in a bare workspace with
    // no database, exactly like `--print-openapi` (CONTRACT §16).
    match command {
        AdminCommand::Fixture {
            command:
                FixtureCommand::Board {
                    cards,
                    lists,
                    comments,
                    tasks,
                    fields,
                    seed,
                    out,
                },
        } => {
            let document = export::fixture_document(&export::FixtureOptions {
                cards,
                lists,
                comments,
                tasks,
                fields,
                seed,
            })?;
            match out {
                Some(path) => {
                    if let Some(parent) = path.parent()
                        && !parent.as_os_str().is_empty()
                    {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&path, serde_json::to_vec_pretty(&document)?)?;
                    let cards = document
                        .get("boards")
                        .and_then(serde_json::Value::as_array)
                        .and_then(|boards| boards.first())
                        .and_then(|board| board.get("cards"))
                        .and_then(serde_json::Value::as_array)
                        .map_or(0, Vec::len);
                    println!(
                        "{}",
                        serde_json::json!({ "path": path, "cards": cards, "format": export::FORMAT })
                    );
                }
                None => println!("{}", serde_json::to_string_pretty(&document)?),
            }
            return Ok(());
        }
        AdminCommand::Diff {
            command: DiffCommand::Board { left, right },
        } => {
            let left: serde_json::Value = serde_json::from_slice(&std::fs::read(&left)?)?;
            let right: serde_json::Value = serde_json::from_slice(&std::fs::read(&right)?)?;
            let (deltas, references) = export::diff_documents(&left, &right);
            for reference in &references {
                eprintln!("reference: {reference}");
            }
            for delta in &deltas {
                println!("delta: {delta}");
            }
            if !deltas.is_empty() {
                return Err(RunError::Exit(
                    1,
                    format!("{} board-export content delta(s)", deltas.len()),
                ));
            }
            println!("diff: zero deltas");
            return Ok(());
        }
        AdminCommand::TraceDiff {
            old,
            new,
            expect,
            old_workflow,
        } => {
            return match admin::trace_diff::trace_diff(
                old.as_deref(),
                &new,
                old_workflow.as_deref(),
                expect.as_deref(),
            ) {
                Ok(admin::trace_diff::TraceDiffOutcome::Identical) => {
                    println!("diff: zero deltas");
                    Ok(())
                }
                Ok(admin::trace_diff::TraceDiffOutcome::Delta(count)) => {
                    Err(RunError::Exit(1, format!("{count} trace delta(s)")))
                }
                Err(message) => Err(RunError::Exit(2, message)),
            };
        }
        _ => {}
    }

    let config = Config::from_env()?;
    let pool = store::connect(&config.database_migrate_url).await?;
    match command {
        AdminCommand::Actions { command } => match command {
            ActionsCommand::Ingest {
                file,
                service,
                tenant,
                approve,
            } => {
                let bytes = std::fs::read(&file)?;
                let tenant_id = store::tenants::id_by_slug(&pool, &tenant)
                    .await?
                    .ok_or_else(|| RunError::Refused(format!("no tenant {tenant:?}")))?;
                let outcome = conveyor_actions::ingest::ingest_production(
                    &pool, tenant_id, &service, &bytes, approve,
                )
                .await?;
                for (name, version) in &outcome.written {
                    println!("wrote {name} v{version}");
                }
                for (name, version) in &outcome.approved {
                    println!("approved {name} v{version}");
                }
                for (name, version, changes) in &outcome.breaking {
                    eprintln!(
                        "breaking refresh of {name} v{version} written UNAPPROVED: {}",
                        changes.join("; ")
                    );
                }
                if let Some(code) = outcome.exit_code() {
                    return Err(RunError::Exit(
                        code,
                        "a breaking refresh needs `conveyor admin actions approve`".into(),
                    ));
                }
            }
            ActionsCommand::Approve {
                name,
                version,
                tenant,
            } => {
                let tenant_id = store::tenants::id_by_slug(&pool, &tenant)
                    .await?
                    .ok_or_else(|| RunError::Refused(format!("no tenant {tenant:?}")))?;
                let mut tx = store::begin_scoped(&pool, tenant_id).await?;
                let row = store::actions::registry_approve(&mut tx, &name, version, None)
                    .await?
                    .ok_or_else(|| RunError::Refused(format!("no {name} v{version} to approve")))?;
                store::actions::registry_revoke_others(&mut tx, &name, version).await?;
                tx.commit().await?;
                println!("approved {} v{}", row.name, row.spec_version);
            }
        },
        AdminCommand::Import {
            command: ImportCommand::Board { file, tenant },
        } => {
            let document: serde_json::Value = serde_json::from_slice(&std::fs::read(&file)?)?;
            let tenant = conveyor_domain::TenantId::from_uuid(tenant)
                .map_err(|error| RunError::Refused(format!("unusable tenant id: {error}")))?;
            let report = export::import_board(&pool, tenant, &document, &config.app_key).await?;
            println!("{}", serde_json::to_string(&report)?);
        }
        AdminCommand::Export {
            command: ExportCommand::Board { board },
        } => {
            let document = export::export_board(&pool, board).await?;
            println!("{}", serde_json::to_string_pretty(&document)?);
        }
        AdminCommand::ProcessTraceDiff {
            run,
            tenant,
            expect,
            write,
        } => {
            let tenant_id = store::tenants::id_by_slug(&pool, &tenant)
                .await?
                .ok_or_else(|| RunError::Refused(format!("no tenant {tenant:?}")))?;
            let mut tx = store::begin_scoped(&pool, tenant_id).await?;
            let outcome = admin::trace_diff::process_trace_diff(
                &mut tx,
                tenant_id,
                run,
                expect.as_deref(),
                write.as_deref(),
            )
            .await
            .map_err(|message| RunError::Exit(2, message))?;
            tx.commit().await?;
            match &outcome {
                admin::trace_diff::ProcessTraceOutcome::Recorded { path, records } => {
                    println!("recorded {records} record(s) to {}", path.display());
                }
                admin::trace_diff::ProcessTraceOutcome::Identical => {
                    println!("diff: zero deltas");
                }
                admin::trace_diff::ProcessTraceOutcome::Delta(count) => {
                    return Err(RunError::Exit(1, format!("{count} trace delta(s)")));
                }
            }
            return Ok(());
        }
        AdminCommand::UpstreamToken {
            tenant,
            service,
            mint,
            token_stdin: _,
        } => {
            let licences = match config.licences_base_url.as_deref() {
                Some(base) => Some(match config.service_credentials() {
                    Ok(creds) => conveyor_auth::LicenceSyncClient::with_credentials(base, creds)
                        .map_err(|error| RunError::Refused(format!("licences client: {error}")))?,
                    Err(error) => {
                        return Err(RunError::Refused(format!(
                            "licences S2S credential: {error}"
                        )));
                    }
                }),
                None => None,
            };
            let source = if mint {
                admin::upstream_tokens::TokenSource::Mint
            } else {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).map_err(|error| {
                    RunError::Refused(format!("read the token from stdin: {error}"))
                })?;
                admin::upstream_tokens::TokenSource::Byo(line.trim().as_bytes().to_vec())
            };
            let filled =
                admin::upstream_tokens::fill(&pool, licences.as_ref(), &tenant, service, source)
                    .await
                    .map_err(RunError::Refused)?;
            println!(
                "{}",
                serde_json::json!({
                    "secret": filled.secret,
                    "source": filled.source,
                    "expires_at": filled.expires_at,
                })
            );
        }
        AdminCommand::Fixture { .. }
        | AdminCommand::Diff { .. }
        | AdminCommand::TraceDiff { .. } => unreachable!("handled above"),
        AdminCommand::ResealWebhooks => {
            let report = store::webhook::reseal_legacy(&pool)
                .await
                .map_err(|error| RunError::Refused(format!("reseal-webhooks: {error}")))?;
            println!(
                "resealed {} webhook secret(s) across {} tenant(s)",
                report.resealed, report.tenants
            );
        }
    }
    pool.close().await;
    Ok(())
}
