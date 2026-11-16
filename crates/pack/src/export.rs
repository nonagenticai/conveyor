#![forbid(unsafe_code)]
//! The one-way programme export: published documents → `programme/` Markdown + manifest.
//!
//! The database is the store; this is the public copy. The renderer is deterministic — sorted
//! paths, `generated_at` is the newest published row's `updated_at`, never the wall clock — so
//! two exports of an unchanged database are byte-identical and `--check` is meaningful.
//!
//! The reader is the live public reader's: the same `publishable AND status='active' AND body IS
//! NOT NULL` query and the same `redact_text` safety net ([`crate::live`]). A private row cannot
//! reach the export because it cannot reach the query.
//!
//! The export never calls GitHub and never merges anything. A session runs it and commits the
//! result; a merged design pull request is applied to the database by a maintainer, and the next
//! export supersedes the file.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use conveyor_domain::{Project, TenantId};
use conveyor_store as store;
use conveyor_store::document::{DocumentRecord, DocumentStoreError};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::live::EMPTY_GENERATED_AT;
use crate::snapshot::redact_text;

/// The directory the export writes, relative to the output root.
pub const PROGRAMME_DIR: &str = "programme";

/// Why an export could not be built or checked.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// `--database-url` and `DATABASE_URL` are both absent.
    #[error("DATABASE_URL is not set")]
    MissingDatabaseUrl,
    /// The slug names no tenant.
    #[error("unknown tenant {0}")]
    UnknownTenant(String),
    /// The document query failed.
    #[error("document store: {0}")]
    DocumentStore(#[from] DocumentStoreError),
    /// The project or tenant query failed.
    #[error("store: {0}")]
    Store(#[from] sqlx::Error),
    /// A file could not be written or read.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// `--check` found a mismatch.
    #[error("{0}")]
    Check(String),
    /// The caller supplied neither `--out` nor `--check`.
    #[error("supply --out <dir|-|json> or --check <dir>")]
    NoDestination,
}

/// One exported file.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExportedFile {
    /// Path relative to the output root, starting with `programme/`.
    pub path: String,
    /// The file's bytes, UTF-8.
    pub content: String,
    /// The document row, when the file is a document (absent for the README and manifest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// The document's version, when the file is a document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

/// The CLI arguments.
pub struct ExportArgs {
    /// The tenant slug.
    pub tenant: String,
    /// A directory, `-` for one JSON document on stdout, or `json` for the same.
    pub out: Option<String>,
    /// Compare the tree under this root instead of writing.
    pub check: Option<PathBuf>,
    /// A read-only database URL; defaults to `DATABASE_URL`.
    pub database_url: Option<String>,
}

/// What an export did.
#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    /// The tenant slug.
    pub tenant: String,
    /// The newest published row's `updated_at`, or the empty constant.
    pub generated_at: String,
    /// Every file in the tree, sorted by path.
    pub files: Vec<ExportedFile>,
}

impl ExportReport {
    /// Files as a path → content map (the JSON form and the writer).
    pub fn map(&self) -> BTreeMap<String, String> {
        self.files
            .iter()
            .map(|file| (file.path.clone(), file.content.clone()))
            .collect()
    }
}

/// Run an export end to end.
pub async fn run(args: ExportArgs) -> Result<ExportReport, ExportError> {
    let database_url = args
        .database_url
        .clone()
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .filter(|url| !url.trim().is_empty())
        .ok_or(ExportError::MissingDatabaseUrl)?;
    let pool = store::connect(&database_url).await?;
    let tenant = store::tenants::id_by_slug(&pool, &args.tenant)
        .await?
        .ok_or_else(|| ExportError::UnknownTenant(args.tenant.clone()))?;
    let mut report = build(&pool, tenant).await?;
    report.tenant = args.tenant;

    if let Some(root) = args.check {
        check(&root, &report)?;
        return Ok(report);
    }
    let out = args.out.ok_or(ExportError::NoDestination)?;
    if out == "-" || out == "json" {
        let json = serde_json::json!({
            "generated_at": report.generated_at,
            "files": report.map(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&json).map_err(|error| {
                ExportError::Check(format!("serialize the export: {error}"))
            })?
        );
    } else {
        write_tree(Path::new(&out), &report)?;
    }
    Ok(report)
}

/// Build the tree for one tenant from the published set.
pub async fn build(pool: &PgPool, tenant: TenantId) -> Result<ExportReport, ExportError> {
    let mut tx = store::begin_scoped_read_only(pool, tenant).await?;
    let documents = store::document::list_published_outputs_tx(&mut tx, tenant).await?;
    let projects = store::project::list_projects_tx(&mut tx, tenant, None, 1000).await?;
    tx.rollback().await?;
    Ok(render(&documents, &projects))
}

/// Render one published set. Pure: no I/O, no clock.
pub fn render(documents: &[DocumentRecord], projects: &[Project]) -> ExportReport {
    let generated_at = documents
        .iter()
        .map(|record| record.updated_at)
        .max()
        .map(|at| at.to_rfc3339())
        .unwrap_or_else(|| EMPTY_GENERATED_AT.to_owned());

    let by_id: BTreeMap<Uuid, &Project> = projects.iter().map(|p| (p.id, p)).collect();
    let mut paths: BTreeMap<String, ()> = BTreeMap::new();
    let mut docs: Vec<ExportedFile> = Vec::new();

    for record in documents {
        let body = record.body.as_deref().unwrap_or("").trim_end();
        if body.is_empty() {
            // The query already excludes a null body; an all-whitespace body is not a document.
            continue;
        }
        let project = record.project_id.and_then(|id| by_id.get(&id).copied());
        let project_key = project.map(|p| p.key.as_str()).unwrap_or("_unassigned");
        let category = record
            .meta
            .get("category")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("uncategorised");
        let unit = record
            .meta
            .get("unit")
            .and_then(Value::as_str)
            .unwrap_or("");
        let title = redact_text(&record.title);
        let body = redact_text(body);
        let stem = format!("{}-{}", slug(title.as_str()), id8(record.id));
        let mut path = format!(
            "{PROGRAMME_DIR}/{}/{}/{stem}.md",
            segment(project_key),
            segment(category)
        );
        if paths.insert(path.clone(), ()).is_some() {
            // Two rows with the same title under one project+category: fall back to the full id,
            // which is unique; a counter keeps the render total even under a pathological clash.
            path = format!(
                "{PROGRAMME_DIR}/{}/{}/{}-{}.md",
                segment(project_key),
                segment(category),
                slug(title.as_str()),
                record.id.simple()
            );
            let mut counter = 2;
            while paths.insert(path.clone(), ()).is_some() {
                path = format!(
                    "{PROGRAMME_DIR}/{}/{}/{}-{}-{counter}.md",
                    segment(project_key),
                    segment(category),
                    slug(title.as_str()),
                    record.id.simple()
                );
                counter += 1;
            }
        }
        let frontmatter = format!(
            "---\ndocument_id: {}\nversion: {}\ntitle: {}\nproject: {}\nunit: {}\ncategory: {}\npublished_at: {}\n---\n",
            json_string(&record.id.to_string()),
            record.version.get(),
            json_string(title.as_str()),
            json_string(project_key),
            json_string(unit),
            json_string(category),
            json_string(&record.updated_at.to_rfc3339()),
        );
        let content = format!("{frontmatter}\n{body}\n");
        docs.push(ExportedFile {
            path,
            content,
            document_id: Some(record.id.to_string()),
            version: Some(i64::try_from(record.version.get()).unwrap_or(i64::MAX)),
        });
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));

    let readme = render_readme(&generated_at, &docs, projects);
    let manifest = render_manifest(&generated_at, &docs);
    let mut files = docs.clone();
    files.push(ExportedFile {
        path: format!("{PROGRAMME_DIR}/README.md"),
        content: readme,
        document_id: None,
        version: None,
    });
    files.push(ExportedFile {
        path: format!("{PROGRAMME_DIR}/manifest.json"),
        content: manifest,
        document_id: None,
        version: None,
    });
    files.sort_by(|a, b| a.path.cmp(&b.path));
    ExportReport {
        tenant: String::new(),
        generated_at,
        files,
    }
}

/// Write the tree under `root`, creating directories.
pub fn write_tree(root: &Path, report: &ExportReport) -> Result<(), ExportError> {
    for file in &report.files {
        let path = root.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &file.content)?;
    }
    Ok(())
}

/// Compare the tree under `root` with the report; any missing, extra or different file refuses.
pub fn check(root: &Path, report: &ExportReport) -> Result<(), ExportError> {
    let expected: BTreeMap<&str, &str> = report
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.content.as_str()))
        .collect();
    let mut problems: Vec<String> = Vec::new();
    for file in &report.files {
        let path = root.join(&file.path);
        match std::fs::read_to_string(&path) {
            Ok(actual) if actual == file.content => {}
            Ok(_) => problems.push(format!("differs: {}", file.path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                problems.push(format!("missing: {}", file.path));
            }
            Err(error) => problems.push(format!("unreadable: {} ({error})", file.path)),
        }
    }
    let mut actual = BTreeSet::new();
    collect_files(&root.join(PROGRAMME_DIR), &mut actual)?;
    for path in actual {
        if !expected.contains_key(path.as_str()) {
            problems.push(format!("extra: {path}"));
        }
    }
    if problems.is_empty() {
        println!(
            "programme export: {} files current (generated_at {})",
            report.files.len(),
            report.generated_at
        );
        return Ok(());
    }
    problems.sort();
    Err(ExportError::Check(format!(
        "the {PROGRAMME_DIR}/ tree is stale:\n  {}",
        problems.join("\n  ")
    )))
}

fn collect_files(dir: &Path, out: &mut BTreeSet<String>) -> Result<(), ExportError> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if let Ok(relative) = path.strip_prefix(dir.parent().unwrap_or(dir)) {
            out.insert(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn render_manifest(generated_at: &str, docs: &[ExportedFile]) -> String {
    let files: Vec<Value> = docs
        .iter()
        .map(|file| {
            serde_json::json!({
                "path": file.path,
                "document_id": file.document_id,
                "version": file.version,
                "sha256": sha256(&file.content),
            })
        })
        .collect();
    let manifest = serde_json::json!({
        "schema": "conveyor-programme-export/v1",
        "generated_at": generated_at,
        "files": files,
    });
    let mut text = serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_owned());
    text.push('\n');
    text
}

fn render_readme(generated_at: &str, docs: &[ExportedFile], projects: &[Project]) -> String {
    let mut by_project: BTreeMap<String, Vec<&ExportedFile>> = BTreeMap::new();
    for doc in docs {
        let key = doc
            .path
            .strip_prefix(&format!("{PROGRAMME_DIR}/"))
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("_unassigned")
            .to_owned();
        by_project.entry(key).or_default().push(doc);
    }
    let mut ordered: Vec<&Project> = projects.iter().collect();
    ordered.sort_by_key(|project| {
        (
            project
                .meta
                .get("tranche")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            project.key.clone(),
        )
    });
    let mut out = String::new();
    out.push_str("# The conveyor programme\n\n");
    writeln!(
        out,
        "This directory is the public copy of the programme's intermediate outputs. The database \
         is the store; this tree is exported from it. A design change is a pull request against \
         this directory; a maintainer applies it to the database and the next export supersedes \
         the file.\n"
    )
    .ok();
    writeln!(out, "- **Exported:** {generated_at}").ok();
    writeln!(out, "- **Documents:** {}", docs.len()).ok();
    writeln!(
        out,
        "- **Index:** every document carries frontmatter (id, version, project, unit, category, \
         published_at); `manifest.json` carries the hashes.\n"
    )
    .ok();
    writeln!(out, "| Tranche | Project | Name | Outputs |").ok();
    writeln!(out, "|---:|---|---|---:|").ok();
    for project in &ordered {
        let tranche = project
            .meta
            .get("tranche")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let count = by_project
            .get(&segment(&project.key))
            .map(Vec::len)
            .unwrap_or(0);
        let link = format!("[{}](./{}/)", project.key, segment(&project.key));
        writeln!(
            out,
            "| {tranche} | {link} | {} | {count} |",
            project.name.replace('|', "\\|")
        )
        .ok();
    }
    writeln!(out).ok();
    for (key, files) in &by_project {
        writeln!(out, "## {key}\n").ok();
        for file in files {
            let relative = file.path.trim_start_matches(&format!("{PROGRAMME_DIR}/"));
            let title = file
                .content
                .split("\n---\n")
                .nth(1)
                .and_then(|body| body.lines().find(|line| line.starts_with("# ")))
                .map(|line| line.trim_start_matches("# ").to_owned())
                .unwrap_or_else(|| relative.to_owned());
            writeln!(out, "- [{title}](./{relative})").ok();
        }
        writeln!(out).ok();
    }
    out
}

fn sha256(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        write!(out, "{byte:02x}").ok();
    }
    out
}

fn id8(id: Uuid) -> String {
    id.simple().to_string().chars().take(8).collect()
}

/// A path segment safe for a public tree: lowercase alphanumerics and single dashes.
pub fn segment(input: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "output".to_owned()
    } else {
        out.chars()
            .take(64)
            .collect::<String>()
            .trim_end_matches('-')
            .to_owned()
    }
}

fn slug(title: &str) -> String {
    let out = segment(title);
    if out == "output" {
        "output".to_owned()
    } else {
        out
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn project(key: &str, name: &str, tranche: i64) -> Project {
        Project {
            id: Uuid::now_v7(),
            key: key.to_owned(),
            name: name.to_owned(),
            description: String::new(),
            status: "active".to_owned(),
            programme_id: None,
            project_type_code: None,
            phase_code: None,
            owner_user_id: None,
            repo_url: None,
            meta: serde_json::json!({"tranche": tranche, "track": "v2"}),
            version: conveyor_domain::Version::new(1).expect("version"),
        }
    }

    fn document(project_id: Uuid, title: &str, category: &str, body: &str) -> DocumentRecord {
        DocumentRecord {
            id: Uuid::now_v7(),
            project_id: Some(project_id),
            card_id: None,
            parent_document_id: None,
            kind: "document".to_owned(),
            title: title.to_owned(),
            status: "active".to_owned(),
            object_key: None,
            content_hash: None,
            body: Some(body.to_owned()),
            external_ref: None,
            meta: serde_json::json!({"category": category, "unit": "T1"}),
            category: Some(category.to_owned()),
            unit: Some("T1".to_owned()),
            created_by: None,
            updated_by: None,
            published_at: None,
            published_by: None,
            publish_requested_at: None,
            publishable: true,
            version: conveyor_domain::Version::new(1).expect("version"),
            created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            updated_at: Utc.timestamp_opt(1_700_000_100, 0).unwrap(),
        }
    }

    #[test]
    fn render_is_deterministic_and_sorted() {
        let p = project("T1", "Plan spine", 1);
        let docs = vec![
            document(p.id, "Second", "analysis", "# Second\n"),
            document(p.id, "First / unsafe", "project-brief", "# First\n"),
        ];
        let a = render(&docs, std::slice::from_ref(&p));
        let b = render(&docs, std::slice::from_ref(&p));
        assert_eq!(a.generated_at, "2023-11-14T22:15:00+00:00");
        assert_eq!(a.files, b.files, "two renders are byte-identical");
        let paths: Vec<&str> = a.files.iter().map(|f| f.path.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted);
        assert!(
            paths
                .iter()
                .all(|path| !path.contains("..") && !path.contains(" / "))
        );
        assert!(paths.iter().any(|path| path.ends_with("README.md")));
        assert!(paths.iter().any(|path| path.ends_with("manifest.json")));
        let manifest = a
            .files
            .iter()
            .find(|f| f.path.ends_with("manifest.json"))
            .unwrap();
        assert!(manifest.content.contains("conveyor-programme-export/v1"));
        assert_eq!(manifest.content.matches("\"sha256\"").count(), 2);
    }

    #[test]
    fn a_document_with_no_body_is_not_exported() {
        let p = project("S00", "Inventory", 0);
        let mut blank = document(p.id, "Blank", "analysis", "");
        blank.body = Some("   \n".to_owned());
        let out = render(&[blank], &[p]);
        assert_eq!(
            out.files
                .iter()
                .filter(|f| f.path.ends_with(".md") && !f.path.ends_with("README.md"))
                .count(),
            0
        );
    }
}
