#![forbid(unsafe_code)]
//! `conveyor snapshot` — write the public design-site snapshot for one tenant.
//!
//! The design site (`nonagentic.ai/design`) must render the real platform, not a fixture. This
//! module turns one tenant's rows into a small set of deterministic JSON files plus one
//! `snapshot.js` wrapper the studio can load synchronously:
//!
//! ```text
//! docs/design/data/manifest.json      schema, git sha, migration version, per-file sha256
//! docs/design/data/programme.json     the buildout programme, its projects and issue links
//! docs/design/data/organisation.json  industry → department → job → release → binding
//! docs/design/data/config.json        catalogue, capabilities, agents, procedures, prompts
//! docs/design/data/work.json          boards, lists, cards, runs, approvals, comments, people,
//!                                     and each project's observed activity window
//! docs/design/data/msp.json           benefits, realization, stakeholders, tranches, baselines,
//!                                     cadence, WBS, registers and typed dependencies
//! docs/design/data/artefacts.json     governed assets and document metadata
//! docs/design/data/measure.json       inference aggregates (tokens, cache classes) and the
//!                                     per-project/per-card spend the delivery view reads
//! docs/design/data/secrets.json       secret METADATA only — never a value
//! docs/design/data/evidence.json      the evidence/plan documents and their hashes
//! docs/design/data/feedback.json      the external feedback issues
//! docs/design/data/rework.json        the rework register, only with `--conformance`
//! docs/design/data/snapshot.js        window.CONVEYOR_SNAPSHOT = {…}
//! ```
//!
//! **Determinism.** The same database produces byte-identical files. `--generated-at` overrides
//! the clock; without it the clock is derived from the snapshot (the newest published document),
//! exactly as the live view derives it. Every array is ordered by the store query; every object
//! is built in a fixed order.
//!
//! **Manifest.** `manifest.json` hashes every published file: the data files above, the output
//! pages and `outputs.js` when `--outputs-dir` is set, and the issue registry the run read.
//!
//! **Finalise.** `--manifest-only` rehashes a pack directory that later generators have already
//! added to and rewrites its `manifest.json`; it reads no database and regenerates nothing.
//!
//! **Redaction.** The store queries select explicit columns (see `conveyor_store::snapshot`):
//! no secret value, no encryption key id, no email, no OIDC subject, no session or licence
//! token. A redaction test generates a snapshot over a tenant seeded with a known secret and a
//! token-shaped string and asserts neither reaches a file.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use conveyor_store as store;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The snapshot format this binary writes.
pub const SCHEMA: &str = "design-snapshot/v1";

/// The only schema `--conformance` accepts.
pub const REWORK_SCHEMA: &str = "conveyor-rework/v1";

/// The public feedback surface. Overridable per run; the repo may become public, in which case
/// this points at its own Discussions.
pub const DEFAULT_FEEDBACK_URL: &str = "https://github.com/nonagenticai/conveyor/discussions";

/// One `conveyor snapshot` invocation.
#[derive(Debug, Clone)]
pub struct SnapshotArgs {
    /// The tenant slug to snapshot.
    pub tenant: String,
    /// The output directory (`docs/design/data` by default).
    pub out: PathBuf,
    /// When set, also render every published platform output into this directory
    /// (`docs/design/docs`). Absent means the data snapshot only.
    pub outputs_dir: Option<PathBuf>,
    /// A fixed RFC 3339 timestamp, for a byte-identical re-run. Absent means the newest
    /// published document's `updated_at` — the same clock the live view uses — so the same
    /// database still produces byte-identical files.
    pub generated_at: Option<String>,
    /// The committed `gh issue list` registry (`issues.json`).
    pub issues: PathBuf,
    /// The committed rework register (`schema: "conveyor-rework/v1"`). Absent means the pack
    /// publishes no `rework` section and writes no `rework.json`.
    pub conformance: Option<PathBuf>,
    /// Linked from every studio page.
    pub feedback_url: String,
    /// The read-only database URL. Absent means `DATABASE_URL`; the admin command does not
    /// need the serving auth configuration.
    pub database_url: Option<String>,
    /// The commit the published pack describes. Absent means `git rev-parse HEAD` in the
    /// working directory; a pod has no checkout, so a publish passes it explicitly.
    pub git_sha: Option<String>,
    /// Rehash an already-written pack directory and rewrite its `manifest.json`; nothing else
    /// is regenerated and no database is read.
    pub manifest_only: bool,
}

/// One written file.
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotFile {
    /// File name inside the output directory.
    pub path: String,
    /// SHA-256 of the written bytes.
    pub sha256: String,
    /// Byte length.
    pub bytes: usize,
    /// Top-level array records (the row count of the file).
    pub records: usize,
}

/// What the command wrote.
#[derive(Debug, Clone)]
pub struct SnapshotReport {
    /// The tenant slug.
    pub tenant: String,
    /// The output directory.
    pub out: PathBuf,
    /// Every file, sorted by name.
    pub files: Vec<SnapshotFile>,
    /// The SHA-256 of the bytes the file hashes were computed over.
    pub generated_at: String,
    /// The `manifest.json` sha256, for the caller's log line.
    pub manifest_sha256: String,
    /// Platform output pages written beside the data, when `outputs_dir` was set.
    pub outputs: usize,
}

/// Why a snapshot could not be written.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// No database URL was supplied and `DATABASE_URL` is unset.
    #[error("no database URL: pass --database-url or set DATABASE_URL")]
    MissingDatabaseUrl,
    /// The database refused a read.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// The slug names no tenant. Naming it is the point: a typo must not snapshot every tenant.
    #[error("no tenant with slug {0:?}")]
    UnknownTenant(String),
    /// The output directory or a file could not be written.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// A value could not be serialised.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// A published output page or index could not be written.
    #[error(transparent)]
    PublishOutputs(#[from] crate::outputs::PublishOutputsError),
    /// The rework register `--conformance` names is unreadable or is not `conveyor-rework/v1`.
    #[error("rework register {path}: {detail}")]
    Rework {
        /// The path the flag named.
        path: String,
        /// Why it was refused.
        detail: String,
    },
    /// `--manifest-only` names a directory with no readable, snapshot-shaped `manifest.json`.
    #[error("manifest-only {path}: {detail}")]
    ManifestOnly {
        /// The `manifest.json` path that was read.
        path: String,
        /// Why it was refused.
        detail: String,
    },
}

/// Read one tenant and write every snapshot file.
pub async fn run(args: SnapshotArgs) -> Result<SnapshotReport, SnapshotError> {
    if args.manifest_only {
        return manifest_only(&args);
    }
    let database_url = args
        .database_url
        .clone()
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .filter(|url| !url.trim().is_empty())
        .ok_or(SnapshotError::MissingDatabaseUrl)?;
    // The runtime URL reads through the same role the design site's data will eventually
    // describe; the transaction is READ ONLY, so no statement can write.
    let pool = store::connect(&database_url).await?;
    run_with_pool(&pool, args).await
}

/// The body of [`run`], against a pool the caller owns (the integration tests use this).
pub async fn run_with_pool(
    pool: &sqlx::PgPool,
    args: SnapshotArgs,
) -> Result<SnapshotReport, SnapshotError> {
    if args.manifest_only {
        return manifest_only(&args);
    }
    let tenant = store::tenants::id_by_slug(pool, &args.tenant)
        .await?
        .ok_or_else(|| SnapshotError::UnknownTenant(args.tenant.clone()))?;

    let git_sha = args.git_sha.clone().unwrap_or_else(git_sha);
    let migration_version = store::MIGRATOR
        .iter()
        .last()
        .map(|migration| migration.version)
        .unwrap_or(0);
    let migration_description = store::MIGRATOR
        .iter()
        .last()
        .map(|migration| migration.description.to_string())
        .unwrap_or_default();

    let mut tx = store::begin_scoped_read_only(pool, tenant).await?;
    let mut data = store::snapshot::snapshot_tx(&mut tx, tenant).await?;
    tx.rollback().await?;

    // One clock for both producers (v1-pack-2): the live view derives the same value from the
    // same `SnapshotData`, so a download served from memory is the file this run writes.
    let generated_at = args
        .generated_at
        .clone()
        .unwrap_or_else(|| snapshot_generated_at(&data));

    let (issues, issues_bytes) = read_issues(&args.issues)?;
    let feedback = feedback_issues(&issues);
    // The rework register is a file input like the issue registry, but the flag is explicit:
    // an unreadable register or a wrong schema is refused, never silently omitted.
    let rework = args.conformance.as_deref().map(read_rework).transpose()?;

    std::fs::create_dir_all(&args.out)?;

    // The wrapper carries the same registry the per-file documents do: insert it before the
    // wrapper is built, or the data panel and feedback list read a programme without it.
    if let Some(object) = data.programme.as_object_mut() {
        object.insert("issue_registry".to_owned(), issues.clone());
        object.insert("feedback".to_owned(), Value::Array(feedback.clone()));
    }

    // Build the combined wrapper before the per-file writes consume `data`'s fields.
    let snapshot_js = snapshot_js(
        &data,
        rework.as_ref(),
        &args.tenant,
        &args.feedback_url,
        &generated_at,
        &git_sha,
    )?;

    let mut files = Vec::new();
    let programme = data.programme;
    write_file(
        &args.out,
        "programme.json",
        with_header(programme, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "organisation.json",
        with_header(data.organisation, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "config.json",
        with_header(data.config, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "work.json",
        with_header(data.work, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "msp.json",
        with_header(data.msp, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "artefacts.json",
        with_header(data.artefacts, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "architecture.json",
        with_header(data.architecture, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "ops.json",
        with_header(data.ops, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "measure.json",
        with_header(data.measure, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "secrets.json",
        with_header(data.secrets, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "evidence.json",
        with_header(evidence_index()?, SCHEMA, &generated_at),
        &mut files,
    )?;
    write_file(
        &args.out,
        "feedback.json",
        with_header(json!({"issues": feedback}), SCHEMA, &generated_at),
        &mut files,
    )?;

    // The register keeps its own `conveyor-rework/v1` schema: it is a self-describing document,
    // and the pack manifest already carries the snapshot's schema and timestamp for the file.
    if let Some(rework) = rework {
        write_file(&args.out, "rework.json", rework, &mut files)?;
    }

    // The combined wrapper the studio loads with one <script> tag. It carries the files the
    // pages render; the raw JSON files stay beside it for sharing and diffing.
    write_bytes(&args.out, "snapshot.js", snapshot_js.as_bytes(), &mut files)?;

    // Platform outputs are a separate, published-only projection. They land beside the tracked
    // documents (`docs/design/docs`), not in the data directory, and the pack mirrors both.
    let mut outputs = 0usize;
    if let Some(dir) = args.outputs_dir.as_deref() {
        let (count, written) =
            crate::outputs::write_outputs_with_files(pool, tenant, &generated_at, dir).await?;
        outputs = count;
        // ⛔ Every published page is in the manifest (v1-pack-3): the path is relative to the
        // data directory, so `check-data.cjs` hashes a page exactly as it hashes `programme.json`.
        for output in written {
            files.push(SnapshotFile {
                path: relative_manifest_path(&args.out, dir, &output.name),
                sha256: sha256_hex(&output.bytes),
                bytes: output.bytes.len(),
                records: 0,
            });
        }
        tracing::info!(outputs, dir = %dir.display(), "platform outputs written");
    }

    // The issue registry is an input, but it publishes beside the data (`data/issues.json`). Its
    // hash in the manifest is the registry's provenance (v1-pack-3/5): an app publish that carries
    // another image's registry names a hash the target branch does not have, and the public gate
    // rejects it instead of silently relabelling producer B's registry as tenant A's.
    if let Some(bytes) = issues_bytes
        && args.issues.file_name() == Some(std::ffi::OsStr::new("issues.json"))
    {
        files.push(SnapshotFile {
            path: "issues.json".to_owned(),
            sha256: sha256_hex(&bytes),
            bytes: bytes.len(),
            records: issues
                .get("issues")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    let manifest = json!({
        "schema": SCHEMA,
        "generated_at": &generated_at,
        "tenant": &args.tenant,
        "git_sha": git_sha,
        "migration": {"version": migration_version, "description": migration_description},
        "feedback_url": args.feedback_url,
        "redaction": "no secret values, no encryption key ids, no emails, no OIDC subjects, \
                      no session or licence tokens",
        "files": &files,
    });
    let manifest_bytes = to_pretty_bytes(&manifest)?;
    std::fs::write(args.out.join("manifest.json"), &manifest_bytes)?;
    let manifest_sha256 = sha256_hex(&manifest_bytes);

    Ok(SnapshotReport {
        tenant: args.tenant,
        out: args.out,
        files,
        generated_at,
        manifest_sha256,
        outputs,
    })
}

/// `--manifest-only` (v1-pack-3): rehash every file already in the pack directory and rewrite
/// `manifest.json` from those bytes.
///
/// The snapshot writes its manifest before the publish's later generators (conformance,
/// standards, documents), so those files would publish unhashed. This pass runs after the last
/// generator: it keeps the old manifest's header, takes each file's `records` from the old
/// manifest when it listed that path (never inventing one), and hashes the bytes on disk now.
fn manifest_only(args: &SnapshotArgs) -> Result<SnapshotReport, SnapshotError> {
    let out = &args.out;
    let manifest_path = out.join("manifest.json");
    let path_label = manifest_path.display().to_string();

    let read =
        std::fs::read_to_string(&manifest_path).map_err(|error| SnapshotError::ManifestOnly {
            path: path_label.clone(),
            detail: if error.kind() == std::io::ErrorKind::NotFound {
                "no manifest.json; run a normal snapshot first".to_owned()
            } else {
                error.to_string()
            },
        })?;
    let old: Value = serde_json::from_str(&read).map_err(|error| SnapshotError::ManifestOnly {
        path: path_label.clone(),
        detail: format!("does not parse: {error}"),
    })?;
    if old.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        return Err(SnapshotError::ManifestOnly {
            path: path_label.clone(),
            detail: format!(
                "schema is {}, expected {SCHEMA:?}",
                old.get("schema")
                    .map_or_else(|| "absent".to_owned(), |schema| schema.to_string())
            ),
        });
    }
    let old_files =
        old.get("files")
            .and_then(Value::as_array)
            .ok_or_else(|| SnapshotError::ManifestOnly {
                path: path_label.clone(),
                detail: "carries no files array".to_owned(),
            })?;

    let mut old_records = std::collections::HashMap::new();
    for file in old_files {
        if let (Some(path), Some(records)) = (
            file.get("path").and_then(Value::as_str),
            file.get("records").and_then(Value::as_u64),
        ) {
            old_records.insert(path.to_owned(), records as usize);
        }
    }

    let mut paths = Vec::new();
    collect_files(out, out, &mut paths)?;
    paths.sort();

    let mut files = Vec::with_capacity(paths.len() + old_files.len());
    let mut walked = std::collections::HashSet::new();
    for path in paths {
        let bytes = std::fs::read(out.join(&path))?;
        walked.insert(path.clone());
        files.push(SnapshotFile {
            path: path.clone(),
            sha256: sha256_hex(&bytes),
            bytes: bytes.len(),
            records: old_records.get(&path).copied().unwrap_or(0),
        });
    }

    // The snapshot's platform pages live outside the data directory (`../docs/…`). They are
    // published pack files too, so keep an old entry whose file still exists; an entry whose
    // file is gone is dropped, because the manifest must name only files that are there.
    for file in old_files {
        let Some(path) = file.get("path").and_then(Value::as_str) else {
            continue;
        };
        if walked.contains(path) || path == "manifest.json" {
            continue;
        }
        if !out.join(path).is_file() {
            continue;
        }
        let bytes = std::fs::read(out.join(path))?;
        files.push(SnapshotFile {
            path: path.to_owned(),
            sha256: sha256_hex(&bytes),
            bytes: bytes.len(),
            records: old_records.get(path).copied().unwrap_or(0),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let header = |key: &str| old.get(key).cloned().unwrap_or(Value::Null);
    let manifest = json!({
        "schema": SCHEMA,
        "generated_at": header("generated_at"),
        "tenant": header("tenant"),
        "git_sha": header("git_sha"),
        "migration": header("migration"),
        "feedback_url": header("feedback_url"),
        "redaction": header("redaction"),
        "files": &files,
    });
    let manifest_bytes = to_pretty_bytes(&manifest)?;
    std::fs::write(&manifest_path, &manifest_bytes)?;
    let manifest_sha256 = sha256_hex(&manifest_bytes);

    Ok(SnapshotReport {
        tenant: old
            .get("tenant")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        out: out.clone(),
        files,
        generated_at: old
            .get("generated_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        manifest_sha256,
        outputs: 0,
    })
}

/// Every file under `dir`, as a forward-slash path relative to `root`, excluding the root
/// `manifest.json` (the file this pass rewrites).
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if relative != "manifest.json" {
            out.push(relative);
        }
    }
    Ok(())
}

/// One snapshot section as the standalone JSON file the CLI writes.
///
/// The live `/design/data/programme.json` route serves this from memory, so the download and the
/// file `conveyor snapshot` writes are the same bytes: one header, one redaction pass, one
/// ordering.
pub(crate) fn section_json(section: &Value, generated_at: &str) -> Result<String, SnapshotError> {
    let mut value = section.clone();
    redact(&mut value);
    let value = with_header(value, SCHEMA, generated_at);
    let mut text = serde_json::to_string_pretty(&value)?;
    text.push('\n');
    Ok(text)
}

/// The clock the CLI and the live view share: the newest published document in the programme
/// snapshot, or [`crate::live::EMPTY_GENERATED_AT`] when there is none.
///
/// v1-pack-2: both producers compute this from the same `SnapshotData`, so a CLI run without
/// `--generated-at` and the live route cannot disagree about `generated_at` — and therefore about
/// the manifest timestamp or `snapshot.js.payload_sha256`.
pub(crate) fn snapshot_generated_at(data: &store::snapshot::SnapshotData) -> String {
    data.artefacts
        .get("documents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|document| document.get("updated_at").and_then(Value::as_str))
        .filter_map(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .max()
        .map(|at| at.to_rfc3339())
        .unwrap_or_else(|| crate::live::EMPTY_GENERATED_AT.to_owned())
}

/// The manifest path of one file in the outputs directory, relative to the data directory.
///
/// The pack's two writers lay the directories out as siblings (`data/` and `docs/`), so
/// `output-<id>.html` is `../docs/output-<id>.html` in `data/manifest.json` — the form
/// `check-data.cjs` resolves against its data directory.
fn relative_manifest_path(out: &Path, outputs_dir: &Path, name: &str) -> String {
    let mut base = out;
    let mut ups = 0usize;
    loop {
        if let Ok(rest) = outputs_dir.strip_prefix(base) {
            if rest.as_os_str().is_empty() {
                return name.to_owned();
            }
            let mut path = PathBuf::new();
            for _ in 0..ups {
                path.push("..");
            }
            path.push(rest);
            path.push(name);
            return path.to_string_lossy().replace('\\', "/");
        }
        match base.parent() {
            Some(parent) => {
                base = parent;
                ups += 1;
            }
            None => break,
        }
    }
    outputs_dir.join(name).to_string_lossy().replace('\\', "/")
}

/// Add the schema and timestamp so every file is self-describing.
fn with_header(mut value: Value, schema: &str, generated_at: &str) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert("schema".to_owned(), Value::String(schema.to_owned()));
        object.insert(
            "generated_at".to_owned(),
            Value::String(generated_at.to_owned()),
        );
    }
    value
}

pub(crate) fn snapshot_js(
    data: &store::snapshot::SnapshotData,
    rework: Option<&Value>,
    tenant: &str,
    feedback_url: &str,
    generated_at: &str,
    git_sha: &str,
) -> Result<String, SnapshotError> {
    let mut value = json!({
        "schema": SCHEMA,
        "tenant": tenant,
        "generated_at": generated_at,
        "git_sha": git_sha,
        "feedback_url": feedback_url,
        "programme": data.programme,
        "organisation": data.organisation,
        "config": data.config,
        "work": data.work,
        "msp": data.msp,
        "artefacts": data.artefacts,
        "architecture": data.architecture,
        "ops": data.ops,
        "measure": data.measure,
    });
    // The register is not a store section (`SnapshotData` is owned by `conveyor_store`), so it
    // is attached here, before redaction — present only when the caller supplied one, absent
    // otherwise, so a snapshot without `--conformance` keeps its exact bytes.
    if let Some(rework) = rework {
        value["rework"] = rework.clone();
    }
    redact(&mut value);
    // The hash covers the payload BEFORE this field is added; it is what the LIVE ribbon shows
    // and what a reader cites when reporting on a screenshot.
    let payload = serde_json::to_string(&value)?;
    value["payload_sha256"] = json!(sha256_hex(payload.as_bytes()));
    Ok(format!(
        "window.CONVEYOR_SNAPSHOT = {};\n",
        serde_json::to_string(&value)?
    ))
}

fn write_file(
    out: &Path,
    name: &str,
    mut value: Value,
    files: &mut Vec<SnapshotFile>,
) -> Result<(), SnapshotError> {
    redact(&mut value);
    let bytes = to_pretty_bytes(&value)?;
    write_bytes(out, name, &bytes, files)?;
    if let Some(file) = files.last_mut() {
        file.records = records(&value);
    }
    Ok(())
}

fn write_bytes(
    out: &Path,
    name: &str,
    bytes: &[u8],
    files: &mut Vec<SnapshotFile>,
) -> Result<(), SnapshotError> {
    std::fs::write(out.join(name), bytes)?;
    files.push(SnapshotFile {
        path: name.to_owned(),
        sha256: sha256_hex(bytes),
        bytes: bytes.len(),
        records: 0,
    });
    Ok(())
}

fn to_pretty_bytes(value: &Value) -> Result<Vec<u8>, SnapshotError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn records(value: &Value) -> usize {
    value
        .as_object()
        .map(|object| {
            object
                .values()
                .filter_map(Value::as_array)
                .map(Vec::len)
                .sum()
        })
        .unwrap_or(0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The safety net for free text. The store queries already exclude every secret column, but a
/// card description or a feedback issue can carry a credential a person pasted into it. A
/// public build must not publish that by accident.
static EMAIL_PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}")
        .expect("the email pattern is a constant")
});
static TOKEN_PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(
        r"(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9\-]{10,}|eyJ[A-Za-z0-9_\-]{20,}\.[A-Za-z0-9_\-]{20,}\.[A-Za-z0-9_\-]{20,})",
    )
    .expect("the token pattern is a constant")
});
static PRIVATE_KEY_PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")
        .expect("the private-key pattern is a constant")
});
static BEARER_PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{20,}")
        .expect("the bearer pattern is a constant")
});
static DATABASE_URL_PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r#"(?i)\b(postgres(?:ql)?://)[^\s"']+"#)
        .expect("the database-url pattern is a constant")
});
static IP_PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("the IP pattern is a constant")
});
static HOME_PATH_PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r#"(?:/Users/[A-Za-z0-9._-]+|/home/[A-Za-z0-9._-]+|~/)[^\s"']*"#)
        .expect("the home-path pattern is a constant")
});

/// Mask emails, credential-shaped strings, IPs and home paths anywhere in a value.
///
/// The structural queries already exclude every free-text field that is likely to carry
/// private material (card descriptions, comments, run payloads, notes). This pass is the
/// safety net for the fields that remain public (titles, charters, configuration text).
pub(crate) fn redact(value: &mut Value) {
    match value {
        Value::String(text) => {
            let masked = DATABASE_URL_PATTERN.replace_all(text, "$1[redacted]");
            let masked = BEARER_PATTERN.replace_all(&masked, "Bearer [redacted-token]");
            let masked = EMAIL_PATTERN.replace_all(&masked, "***@redacted");
            let masked = TOKEN_PATTERN.replace_all(&masked, "[redacted-token]");
            let masked = PRIVATE_KEY_PATTERN.replace_all(&masked, "[redacted-key]");
            let masked = IP_PATTERN.replace_all(&masked, "[redacted-ip]");
            let masked = HOME_PATH_PATTERN.replace_all(&masked, "[redacted-path]");
            if masked.as_ref() != text.as_str() {
                *text = masked.into_owned();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        Value::Object(map) => map.values_mut().for_each(redact),
        _ => {}
    }
}

/// Redact one free-text value (the same safety net the snapshot applies to its JSON).
pub(crate) fn redact_text(text: &str) -> String {
    let mut value = Value::String(text.to_owned());
    redact(&mut value);
    match value {
        Value::String(redacted) => redacted,
        _ => String::new(),
    }
}

/// The issue registry written by `scripts/fetch-issues.sh`. Absent is not fatal: the app does
/// not call GitHub at runtime, and a snapshot without the registry still renders every row.
///
/// The bytes come back so the manifest can name the registry it published — its provenance.
fn read_issues(path: &Path) -> Result<(Value, Option<Vec<u8>>), SnapshotError> {
    if !path.exists() {
        tracing::warn!(path = %path.display(), "issue registry absent; publishing no issue links");
        return Ok((json!({"issues": []}), None));
    }
    let text = std::fs::read_to_string(path)?;
    let bytes = text.clone().into_bytes();
    let value: Value = serde_json::from_str(&text)?;
    Ok((
        if value.get("issues").is_some() {
            value
        } else {
            json!({"issues": value})
        },
        Some(bytes),
    ))
}

/// The rework register `--conformance` names.
///
/// Unlike the issue registry, absence here is a failure: the flag is an explicit request to
/// publish a register, and claiming success while publishing none would misreport the pack.
fn read_rework(path: &Path) -> Result<Value, SnapshotError> {
    let read = std::fs::read_to_string(path).map_err(|error| SnapshotError::Rework {
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    let value: Value = serde_json::from_str(&read).map_err(|error| SnapshotError::Rework {
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    let schema = value.get("schema").and_then(Value::as_str);
    if schema != Some(REWORK_SCHEMA) {
        return Err(SnapshotError::Rework {
            path: path.display().to_string(),
            detail: format!(
                "expected schema {REWORK_SCHEMA:?}, found {}",
                schema.map_or_else(|| "no schema".to_owned(), |found| format!("{found:?}"))
            ),
        });
    }
    Ok(value)
}

pub(crate) fn feedback_issues(issues: &Value) -> Vec<Value> {
    issues
        .get("issues")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter(|row| {
                    row.get("labels")
                        .and_then(Value::as_array)
                        .map(|labels| {
                            labels.iter().any(|label| {
                                label.get("name").and_then(Value::as_str) == Some("feedback")
                            })
                        })
                        .unwrap_or(false)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// The evidence and plan documents, by path and hash, so a design page can cite what it shows.
fn evidence_index() -> Result<Value, SnapshotError> {
    let root = std::env::current_dir()?;
    let mut paths = Vec::new();
    for dir in [
        "docs/evidence",
        "docs/plans",
        "docs/decisions",
        "docs/design",
    ] {
        collect_markdown(&root.join(dir), &mut paths)?;
    }
    for file in [
        "docs/SPEC.md",
        "docs/SPEC_V2.md",
        "docs/KNOWLEDGE-MEMORY.md",
    ] {
        let path = root.join(file);
        if path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    let mut rows = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = std::fs::read(&path)?;
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        rows.push(json!({
            "path": relative,
            "sha256": sha256_hex(&bytes),
            "bytes": bytes.len(),
        }));
    }
    Ok(json!({"files": rows}))
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}

fn git_sha() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|sha| sha.trim().to_owned())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// One line for the CLI log.
impl SnapshotReport {
    /// `tenant · N files · manifest <sha>`.
    pub fn summary(&self) -> String {
        let mut line = String::new();
        let _ = write!(
            line,
            "tenant={} files={} manifest_sha256={}",
            self.tenant,
            self.files.len(),
            self.manifest_sha256
        );
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v1-pack-3: an output page in the sibling `docs/` directory is named from the data
    /// directory the manifest lives in, so the public gate can hash it like any data file.
    #[test]
    fn an_output_page_is_named_relative_to_the_data_directory() {
        assert_eq!(
            relative_manifest_path(
                Path::new("/pack/data"),
                Path::new("/pack/docs"),
                "output-1.html"
            ),
            "../docs/output-1.html"
        );
        assert_eq!(
            relative_manifest_path(
                Path::new("/pack/data"),
                Path::new("/pack/docs"),
                "outputs.js"
            ),
            "../docs/outputs.js"
        );
        assert_eq!(
            relative_manifest_path(
                Path::new("/pack/data"),
                Path::new("/pack/data"),
                "outputs.js"
            ),
            "outputs.js"
        );
        assert_eq!(
            relative_manifest_path(
                Path::new("/pack/data"),
                Path::new("/pack/data/pages"),
                "output-1.html"
            ),
            "pages/output-1.html"
        );
    }

    /// v1-pack-3/v1-pack-4: the finalise pass rehashes the finished pack, keeps the record
    /// count the old manifest gave a file, gives a file the old manifest never saw `0`, and
    /// never lists `manifest.json` itself.
    #[test]
    fn manifest_only_rehashes_the_finished_pack() -> Result<(), SnapshotError> {
        let dir =
            std::env::temp_dir().join(format!("conveyor-pack-manifest-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir)?;
        let old_manifest = json!({
            "schema": SCHEMA,
            "generated_at": "2026-11-12T00:00:00Z",
            "tenant": "snap",
            "git_sha": "0123456789abcdef0123456789abcdef01234567",
            "migration": {"version": 42, "description": "the snapshot migration"},
            "feedback_url": DEFAULT_FEEDBACK_URL,
            "redaction": "no secret values, no encryption key ids, no emails",
            "files": [
                {"path": "notes.txt", "sha256": "stale", "bytes": 1, "records": 5}
            ],
        });
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&old_manifest)?,
        )?;
        let notes = b"a note\n";
        std::fs::write(dir.join("notes.txt"), notes)?;
        let added = b"[1,2,3]\n";
        std::fs::write(dir.join("added.json"), added)?;

        let report = manifest_only(&SnapshotArgs {
            tenant: "snap".to_owned(),
            out: dir.clone(),
            outputs_dir: None,
            generated_at: None,
            issues: dir.join("issues.json"),
            conformance: None,
            feedback_url: DEFAULT_FEEDBACK_URL.to_owned(),
            database_url: None,
            git_sha: None,
            manifest_only: true,
        })?;

        let paths: Vec<&str> = report.files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, ["added.json", "notes.txt"]);
        assert!(
            !paths.contains(&"manifest.json"),
            "the manifest never lists itself"
        );
        let added_file = &report.files[0];
        assert_eq!(added_file.sha256, sha256_hex(added));
        assert_eq!(added_file.bytes, added.len());
        assert_eq!(
            added_file.records, 0,
            "a file the old manifest never listed has no record count"
        );
        let notes_file = &report.files[1];
        assert_eq!(notes_file.sha256, sha256_hex(notes));
        assert_eq!(
            notes_file.records, 5,
            "the old manifest's record count survives"
        );

        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json"))?)?;
        assert_eq!(written["schema"], SCHEMA);
        assert_eq!(written["tenant"], "snap");
        assert_eq!(written["generated_at"], "2026-11-12T00:00:00Z");
        assert_eq!(
            written["migration"],
            json!({"version": 42, "description": "the snapshot migration"})
        );
        assert_eq!(written["files"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            report.manifest_sha256,
            sha256_hex(&std::fs::read(dir.join("manifest.json"))?)
        );

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
