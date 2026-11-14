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
//! docs/design/data/work.json          boards, lists, cards, runs, approvals, comments, people
//! docs/design/data/artefacts.json     governed assets and document metadata
//! docs/design/data/measure.json       inference aggregates (tokens, cache classes)
//! docs/design/data/secrets.json       secret METADATA only — never a value
//! docs/design/data/evidence.json      the evidence/plan documents and their hashes
//! docs/design/data/feedback.json      the external feedback issues
//! docs/design/data/snapshot.js        window.CONVEYOR_SNAPSHOT = {…}
//! ```
//!
//! **Determinism.** The same database and the same `--generated-at` produce byte-identical
//! files. Every array is ordered by the store query; every object is built in a fixed order.
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
    /// When set, also render every publishable platform output into this directory
    /// (`docs/design/docs`). Absent means the data snapshot only.
    pub outputs_dir: Option<PathBuf>,
    /// A fixed RFC 3339 timestamp, for a byte-identical re-run. Defaults to now.
    pub generated_at: Option<String>,
    /// The committed `gh issue list` registry (`issues.json`).
    pub issues: PathBuf,
    /// Linked from every studio page.
    pub feedback_url: String,
    /// The read-only database URL. Absent means `DATABASE_URL`; the admin command does not
    /// need the serving auth configuration.
    pub database_url: Option<String>,
    /// The commit the published pack describes. Absent means `git rev-parse HEAD` in the
    /// working directory; a pod has no checkout, so a publish passes it explicitly.
    pub git_sha: Option<String>,
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
    /// A publishable output page or index could not be written.
    #[error(transparent)]
    PublishOutputs(#[from] crate::publish_outputs::PublishOutputsError),
}

/// Read one tenant and write every snapshot file.
pub async fn run(args: SnapshotArgs) -> Result<SnapshotReport, SnapshotError> {
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
    let tenant = store::tenants::id_by_slug(pool, &args.tenant)
        .await?
        .ok_or_else(|| SnapshotError::UnknownTenant(args.tenant.clone()))?;

    let generated_at = args
        .generated_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
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

    let issues = read_issues(&args.issues)?;
    let feedback = feedback_issues(&issues);

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
        "artefacts.json",
        with_header(data.artefacts, SCHEMA, &generated_at),
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

    // The combined wrapper the studio loads with one <script> tag. It carries the files the
    // pages render; the raw JSON files stay beside it for sharing and diffing.
    write_bytes(&args.out, "snapshot.js", snapshot_js.as_bytes(), &mut files)?;

    // Platform outputs are a separate, publishable-only projection. They land beside the tracked
    // documents (`docs/design/docs`), not in the data directory, and the pack mirrors both.
    let mut outputs = 0usize;
    if let Some(dir) = args.outputs_dir.as_deref() {
        outputs = crate::publish_outputs::write_outputs(pool, tenant, &generated_at, dir).await?;
        tracing::info!(outputs, dir = %dir.display(), "platform outputs written");
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

fn snapshot_js(
    data: &store::snapshot::SnapshotData,
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
        "artefacts": data.artefacts,
        "measure": data.measure,
    });
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
fn redact(value: &mut Value) {
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

/// The issue registry written by `scripts/fetch-issues.sh`. Absent is not fatal: the app does
/// not call GitHub at runtime, and a snapshot without the registry still renders every row.
fn read_issues(path: &Path) -> Result<Value, SnapshotError> {
    if !path.exists() {
        tracing::warn!(path = %path.display(), "issue registry absent; publishing no issue links");
        return Ok(json!({"issues": []}));
    }
    let text = std::fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)?;
    Ok(if value.get("issues").is_some() {
        value
    } else {
        json!({"issues": value})
    })
}

fn feedback_issues(issues: &Value) -> Vec<Value> {
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
