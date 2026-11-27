#![forbid(unsafe_code)]
//! Platform outputs on the public design site: one standalone HTML page per published output,
//! plus the `outputs.js` index the Documents destination reads.
//!
//! **Static, not runtime.** The pack makes no request; a published body is rendered here, at
//! publish time, into `docs/output-<id>.html` by `conveyor_render::markdown`, the same renderer
//! the operator's preview calls. `outputs.js` is the
//! index (`window.CONVEYOR_OUTPUTS = {…}`), and `docs/design/docs.js` only lists it.
//!
//! **The filter is the publication decision.** Only `published_at IS NOT NULL AND status='active'` rows with a
//! body are rendered. A row that exists but was never published is absent from both the page set
//! and the index — there is no "all documents" fallback.
//!
//! **Author input is untrusted.** Raw HTML in a body is turned into text, and a link or image
//! whose URL names a non-http(s)/mailto scheme is made inert. The public-page test carries
//! `<script>`, an `onerror` attribute and a `javascript:` link.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use conveyor_domain::Project;
use conveyor_domain::TenantId;
pub use conveyor_render::markdown::render_markdown;
use conveyor_store as store;
use conveyor_store::document::DocumentRecord;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::snapshot::redact_text;

/// The schema name of the generated index.
pub const OUTPUTS_SCHEMA: &str = "design-outputs/v1";

/// The template every output page uses. It is the same skeleton `build-docs.py` uses, so an
/// output page and a tracked document page look alike and share the pack's CSS.
const TEMPLATE: &str = include_str!("../../../docs/design/scripts/doc-template.html");

/// The console, where an operator edits the output. The page's "source" link points here.
const CONSOLE_URL: &str = "https://conveyor.nonagentic.ai/design/";

/// Why the outputs could not be written.
#[derive(Debug, thiserror::Error)]
pub enum PublishOutputsError {
    /// The database refused a read.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// A document read failed.
    #[error("document: {0}")]
    Document(#[from] store::document::DocumentStoreError),
    /// A file could not be written.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// The index could not be serialised.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// One published attachment on an output page and in `outputs.js`.
#[derive(Clone, Debug, Serialize)]
pub struct AttachmentEntry {
    /// The attachment document id.
    pub id: String,
    /// The display file name (the attachment's title).
    pub filename: String,
    /// The validated MIME type.
    pub content_type: String,
    /// The byte length.
    pub size_bytes: i64,
    /// The live route path, relative to the pack root.
    pub path: String,
}

/// The provenance one published output carries: what the row itself records.
///
/// Every field except `status` and `version` is optional because the rows written before the
/// write path carried no provenance; the page says `unrecorded` rather than inventing a value.
/// `commit` is the commit the pack was rendered from (`CONVEYOR_GIT_SHA`), not the author's
/// commit, which the engine never sees.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Provenance {
    /// The output-standard kind the row was published under (`document.category`).
    pub kind: Option<String>,
    /// The row's optimistic-concurrency version at render time.
    pub version: u64,
    /// The sha256 of the body or the stored bytes.
    pub hash: Option<String>,
    /// The row's status (`active`, `rejected`, `failed`).
    pub status: String,
    /// The author's display name, when one is recorded.
    pub owner: Option<String>,
    /// The gate the output was published under, when the row names one.
    pub gate: Option<String>,
    /// The commit the pack was rendered from.
    pub commit: Option<String>,
}

impl Provenance {
    /// Build from a record; `owner` is the author's resolved display name.
    pub fn of(record: &DocumentRecord, owner: Option<&str>, commit: Option<&str>) -> Self {
        Self {
            kind: record.category.clone(),
            version: record.version.get(),
            hash: record.content_hash.clone(),
            status: record.status.clone(),
            owner: owner.map(str::to_owned),
            gate: record
                .meta
                .get("gate")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            commit: commit.map(str::to_owned),
        }
    }

    /// Whether the row carried any provenance at all.
    #[must_use]
    pub fn recorded(&self) -> bool {
        self.kind.is_some() || self.hash.is_some() || self.gate.is_some()
    }
}

/// One entry of `outputs.js`.
#[derive(Clone, Debug, Serialize)]
pub struct OutputEntry {
    /// The document id.
    pub id: String,
    /// The display title.
    pub title: String,
    /// The owning project.
    pub project_id: Option<String>,
    /// The project key, for grouping and display.
    pub project_key: Option<String>,
    /// The project name, for display.
    pub project_name: Option<String>,
    /// The output-standard slot, when recorded.
    pub category: Option<String>,
    /// The programme unit, when recorded.
    pub unit: Option<String>,
    /// The author's display name, when recorded; never an email.
    pub author: Option<String>,
    /// The published attachments, in upload order.
    pub attachments: Vec<AttachmentEntry>,
    /// The page path, relative to the pack root (`docs/output-<id>.html`).
    pub path: String,
    /// When the row last became public; `None` on a legacy published row.
    pub published_at: Option<String>,
    /// When the output last changed.
    pub updated_at: String,
    /// What the row itself records about its own provenance.
    pub provenance: Provenance,
}

/// Render one output and write `output-<id>.html` files and `outputs.js` into `out`.
///
/// Returns the number of published outputs. An empty set still writes an `outputs.js` with an
/// empty array, so a removed publication removes the stale index rather than leaving the old one.
pub async fn write_outputs(
    pool: &PgPool,
    tenant: TenantId,
    generated_at: &str,
    out: &Path,
) -> Result<usize, PublishOutputsError> {
    let inputs = read_outputs_inputs(pool, tenant).await?;
    let (index, pages) = render_outputs(
        &inputs.records,
        &inputs.projects,
        &inputs.attachments,
        &inputs.names,
        generated_at,
        rendered_commit().as_deref(),
    )?;
    write_rendered(out, &index, &pages)
}

/// The rows one outputs render needs, read inside one read-only transaction.
///
/// The register (`crate::register`) is built from these same rows, so the index and the register
/// cannot describe different published sets when they are built from one read.
pub struct OutputsInputs {
    /// The published output rows.
    pub records: Vec<store::document::DocumentRecord>,
    /// Every project the rows can belong to.
    pub projects: Vec<Project>,
    /// The published attachments, in upload order.
    pub attachments: Vec<store::document::DocumentRecord>,
    /// Author display names, by user id.
    pub names: HashMap<Uuid, String>,
}

/// Read the published-output rows and their lookups in one read-only transaction.
pub async fn read_outputs_inputs(
    pool: &PgPool,
    tenant: TenantId,
) -> Result<OutputsInputs, PublishOutputsError> {
    let mut tx = store::begin_scoped_read_only(pool, tenant).await?;
    let records = store::document::list_published_outputs_tx(&mut tx, tenant).await?;
    let projects = store::project::list_projects_tx(&mut tx, tenant, None, 1000).await?;
    let attachments = store::document::list_published_attachments_tx(&mut tx, tenant).await?;
    let names = author_names(&mut tx, &records).await?;
    tx.rollback().await?;
    Ok(OutputsInputs {
        records,
        projects,
        attachments,
        names,
    })
}

/// One file [`write_rendered`] wrote: its name inside the outputs directory and the exact bytes.
pub(crate) struct WrittenOutput {
    /// File name (`output-<id>.html` or `outputs.js`).
    pub name: String,
    /// The bytes written; hashed into the snapshot manifest.
    pub bytes: Vec<u8>,
}

/// Write the rendered index and pages. No database: the caller already read the rows.
pub fn write_rendered(
    out: &Path,
    index: &str,
    pages: &HashMap<Uuid, String>,
) -> Result<usize, PublishOutputsError> {
    Ok(write_rendered_with_files(out, index, pages)?.0)
}

/// [`write_rendered`], plus the exact files it wrote and their bytes.
///
/// The snapshot's manifest hashes every published file (SPEC §10.3); the caller needs the
/// written bytes, not a directory scan — `out` may also hold the tracked document pages.
pub(crate) fn write_rendered_with_files(
    out: &Path,
    index: &str,
    pages: &HashMap<Uuid, String>,
) -> Result<(usize, Vec<WrittenOutput>), PublishOutputsError> {
    std::fs::create_dir_all(out)?;
    let mut written = Vec::with_capacity(pages.len() + 1);
    for (id, page) in pages {
        let name = format!("output-{id}.html");
        std::fs::write(out.join(&name), page)?;
        written.push(WrittenOutput {
            name,
            bytes: page.as_bytes().to_vec(),
        });
    }
    std::fs::write(out.join("outputs.js"), index)?;
    written.push(WrittenOutput {
        name: "outputs.js".to_owned(),
        bytes: index.as_bytes().to_vec(),
    });
    Ok((pages.len(), written))
}

/// Render the published set from rows the caller already read.
///
/// The caller owns the transaction: [`write_outputs`] opens one for the CLI, and
/// [`crate::live::site_view`] reads the same rows inside the transaction it already holds. One
/// renderer keeps the two byte-identical.
pub fn render_outputs(
    records: &[DocumentRecord],
    projects: &[Project],
    attachments: &[DocumentRecord],
    names: &HashMap<Uuid, String>,
    generated_at: &str,
    commit: Option<&str>,
) -> Result<(String, HashMap<Uuid, String>), PublishOutputsError> {
    // The programme boundary, by the same predicate `live::site_view` publishes: a record or
    // attachment whose project is outside the programme is a session fixture and is not public
    // (no page, no index row). A row with no project is a tenant-level note and stays.
    let bound: HashSet<Uuid> = projects
        .iter()
        .filter(|project| project.programme_id.is_some())
        .map(|project| project.id)
        .collect();
    let projects: HashMap<Uuid, (String, String)> = projects
        .iter()
        .filter(|project| bound.contains(&project.id))
        .map(|project| {
            (
                project.id,
                (redact_text(&project.key), redact_text(&project.name)),
            )
        })
        .collect();
    let mut by_parent: HashMap<Uuid, Vec<&DocumentRecord>> = HashMap::new();
    for attachment in attachments {
        if attachment.project_id.is_some_and(|id| !bound.contains(&id)) {
            continue;
        }
        if let Some(parent) = attachment.parent_document_id {
            by_parent.entry(parent).or_default().push(attachment);
        }
    }

    let mut entries = Vec::with_capacity(records.len());
    let mut pages = HashMap::with_capacity(records.len());
    for record in records {
        // ⛔ A published row outside the programme is not public: no page, no index row.
        if record.project_id.is_some_and(|id| !bound.contains(&id)) {
            continue;
        }
        let body = redact_text(record.body.as_deref().unwrap_or_default());
        let (project_key, project_name) = record
            .project_id
            .and_then(|id| projects.get(&id).cloned())
            .map_or((None, None), |(key, name)| (Some(key), Some(name)));
        let project_label = project_name.as_deref().or(project_key.as_deref());
        // The author display name is free text (an email is a common choice) and leaves the tenant
        // with the page, so it is redacted exactly like the title and the body.
        let author = record
            .created_by
            .and_then(|id| names.get(&id))
            .map(|name| redact_text(name));
        let attachment_entries: Vec<AttachmentEntry> = by_parent
            .get(&record.id)
            .map(|rows| {
                rows.iter()
                    .map(|row| AttachmentEntry {
                        id: row.id.to_string(),
                        filename: redact_text(&row.title),
                        content_type: row
                            .meta
                            .get("content_type")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("application/octet-stream")
                            .to_owned(),
                        size_bytes: row
                            .meta
                            .get("size_bytes")
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or(0),
                        path: format!("docs/attachment-{}", row.id),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let provenance = Provenance::of(record, author.as_deref(), commit);
        let title = redact_text(&record.title);
        let page = output_page(
            &title,
            &render_markdown(&body),
            generated_at,
            project_label,
            author.as_deref(),
            &attachment_entries,
            &provenance,
        );
        pages.insert(record.id, page);
        entries.push(OutputEntry {
            id: record.id.to_string(),
            title,
            project_id: record.project_id.map(|id| id.to_string()),
            project_key,
            project_name,
            category: record.category.as_deref().map(redact_text),
            unit: record.unit.as_deref().map(redact_text),
            author,
            attachments: attachment_entries,
            path: format!("docs/output-{}.html", record.id),
            published_at: record.published_at.map(|at| at.to_rfc3339()),
            updated_at: record.updated_at.to_rfc3339(),
            provenance,
        });
    }
    Ok((outputs_js(&entries, generated_at)?, pages))
}

/// The author display names for the records' `created_by` ids, in one read.
pub(crate) async fn author_names(
    conn: &mut sqlx::PgConnection,
    records: &[DocumentRecord],
) -> Result<HashMap<Uuid, String>, sqlx::Error> {
    let mut ids: Vec<Uuid> = records
        .iter()
        .filter_map(|record| record.created_by)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    store::document::author_names_tx(conn, &ids).await
}

/// The `outputs.js` assignment the Documents page reads synchronously.
pub fn outputs_js(
    entries: &[OutputEntry],
    generated_at: &str,
) -> Result<String, serde_json::Error> {
    let value = serde_json::json!({
        "schema": OUTPUTS_SCHEMA,
        "generated_at": generated_at,
        "outputs": entries,
    });
    Ok(format!(
        "window.CONVEYOR_OUTPUTS = {};\n",
        serde_json::to_string(&value)?
    ))
}

/// Fill the docs template for one output page.
pub fn output_page(
    title: &str,
    body_html: &str,
    generated_at: &str,
    project: Option<&str>,
    author: Option<&str>,
    attachments: &[AttachmentEntry],
    provenance: &Provenance,
) -> String {
    let mut source = match project {
        Some(project) => format!("{project} · conveyor output · published {generated_at}"),
        None => format!("conveyor output · published {generated_at}"),
    };
    if let Some(author) = author.filter(|author| !author.is_empty()) {
        source.push_str(&format!(" · {author}"));
    }
    let body_html = format!(
        "{}{body_html}{}{}",
        provenance_html(provenance),
        attachments_html(attachments),
        feedback_html()
    );
    // Body last: a body that happens to contain a placeholder string must not be rewritten.
    TEMPLATE
        .replace("$root$", "../")
        .replace("$title$", &escape_html(title))
        .replace("$source_url$", CONSOLE_URL)
        .replace("$source$", &escape_html(&source))
        .replace(
            "pandoc + docs/design/scripts/build-docs.py",
            "conveyor publish (apps/server/src/publish_outputs.rs)",
        )
        .replace(
            "A conveyor source document rendered from Markdown at build time.",
            "A conveyor platform output, rendered at publish time.",
        )
        .replace("at build time.</span>", "at publish time.</span>")
        .replace("$body$", &body_html)
}

/// The commit the pack is rendered from. A publish sets `CONVEYOR_GIT_SHA`; a local run has none,
/// and the page says `unrecorded` rather than naming a commit it cannot know.
#[must_use]
pub fn rendered_commit() -> Option<String> {
    std::env::var("CONVEYOR_GIT_SHA")
        .ok()
        .filter(|sha| !sha.trim().is_empty())
}

/// The provenance block every output page carries, before the body.
///
/// It is machine-readable: the `<dl>` carries one `data-provenance-<field>` attribute per field, so
/// a reader (or a test) can assert a value without parsing prose. A field the row never recorded
/// renders `unrecorded` — the block is never omitted and never guessed.
fn provenance_html(provenance: &Provenance) -> String {
    let shown = |value: Option<&str>| -> String {
        value
            .filter(|text| !text.trim().is_empty())
            .unwrap_or("unrecorded")
            .to_owned()
    };
    let status = if provenance.recorded() {
        shown(Some(provenance.status.as_str()))
    } else {
        "unrecorded".to_owned()
    };
    let kind = shown(provenance.kind.as_deref());
    let hash = shown(provenance.hash.as_deref());
    let owner = shown(provenance.owner.as_deref());
    let gate = shown(provenance.gate.as_deref());
    let commit = shown(provenance.commit.as_deref());
    let version = provenance.version.to_string();
    format!(
        "<dl class=\"output-provenance\" \
         data-provenance-kind=\"{kind_attr}\" data-provenance-version=\"{version_attr}\" \
         data-provenance-hash=\"{hash_attr}\" data-provenance-status=\"{status_attr}\" \
         data-provenance-owner=\"{owner_attr}\" data-provenance-gate=\"{gate_attr}\" \
         data-provenance-commit=\"{commit_attr}\">\
         <div><dt>Kind</dt><dd>{kind}</dd></div>\
         <div><dt>Version</dt><dd>{version}</dd></div>\
         <div><dt>SHA-256</dt><dd>{hash}</dd></div>\
         <div><dt>Status</dt><dd>{status}</dd></div>\
         <div><dt>Owner</dt><dd>{owner}</dd></div>\
         <div><dt>Gate</dt><dd>{gate}</dd></div>\
         <div><dt>Rendered from</dt><dd>{commit}</dd></div>\
         </dl>",
        kind_attr = escape_html(&kind),
        version_attr = escape_html(&version),
        hash_attr = escape_html(&hash),
        status_attr = escape_html(&status),
        owner_attr = escape_html(&owner),
        gate_attr = escape_html(&gate),
        commit_attr = escape_html(&commit),
    )
}

/// The attachments block appended to a rendered body. Names are escaped and every link is a
/// same-origin live route; the bytes are served by `/design/docs/attachment-<id>` with a
/// non-inline disposition for anything but a PNG or JPEG.
fn attachments_html(attachments: &[AttachmentEntry]) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let mut out = String::from("<h2>Attachments</h2><ul class=\"attachments\">");
    for attachment in attachments {
        // The page lives at /design/docs/output-<id>.html, so a sibling attachment is
        // `attachment-<id>`; the `path` in `outputs.js` is relative to /design/ instead.
        out.push_str(&format!(
            "<li><a href=\"attachment-{}\" rel=\"noopener\">{}</a> <span class=\"muted\">{}</span></li>",
            escape_html(&attachment.id),
            escape_html(&attachment.filename),
            escape_html(&attachment.content_type),
        ));
    }
    out.push_str("</ul>");
    out
}

/// The public feedback affordance appended to every output page: one Discussions anchor. It is
/// a constant link, never user content and never a script, so the page stays readable without
/// JavaScript and the sanitised body contract is unchanged.
fn feedback_html() -> &'static str {
    "<p class=\"output-feedback\"><a href=\"https://github.com/nonagenticai/conveyor/discussions\">Discuss this output ↗</a></p>"
}

/// Escape a value for HTML text or an attribute.
fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_html_is_text_not_markup() {
        let html =
            render_markdown("before <script>alert(1)</script> <img src=x onerror=alert(1)> after");
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("&lt;img"), "{html}");
        assert!(html.contains("alert(1)"), "{html}");
    }

    #[test]
    fn every_output_page_carries_the_discussion_link_and_no_dead_data_link() {
        let html = output_page(
            "A title",
            "<p>body</p>",
            "2026-11-16",
            Some("T1"),
            Some("Alice"),
            &[],
            &Provenance::default(),
        );
        assert!(
            html.contains("https://github.com/nonagenticai/conveyor/discussions"),
            "{html}"
        );
        assert!(html.contains("Discuss this output"), "{html}");
        assert!(!html.contains("data/programme.json"), "{html}");
        assert!(!html.contains("data/manifest.json"), "{html}");
    }

    #[test]
    fn output_page_renders_provenance() {
        let provenance = Provenance {
            kind: Some("project-brief".to_owned()),
            version: 3,
            hash: Some("abc123".to_owned()),
            status: "active".to_owned(),
            owner: Some("Alice".to_owned()),
            gate: Some("pm.charter".to_owned()),
            commit: Some("ba56759a".to_owned()),
        };
        let html = output_page(
            "T",
            "<p>b</p>",
            "2026-11-26",
            Some("P0"),
            Some("Alice"),
            &[],
            &provenance,
        );
        assert!(
            html.contains("data-provenance-kind=\"project-brief\""),
            "{html}"
        );
        assert!(html.contains("data-provenance-version=\"3\""), "{html}");
        assert!(html.contains("data-provenance-hash=\"abc123\""), "{html}");
        assert!(html.contains("data-provenance-status=\"active\""), "{html}");
        assert!(html.contains("data-provenance-owner=\"Alice\""), "{html}");
        assert!(
            html.contains("data-provenance-gate=\"pm.charter\""),
            "{html}"
        );
        assert!(
            html.contains("data-provenance-commit=\"ba56759a\""),
            "{html}"
        );
    }

    #[test]
    fn output_page_marks_unrecorded_provenance() {
        let provenance = Provenance {
            status: "active".to_owned(),
            version: 1,
            ..Provenance::default()
        };
        let html = output_page("T", "<p>b</p>", "2026-11-26", None, None, &[], &provenance);
        assert!(
            html.contains("data-provenance-kind=\"unrecorded\""),
            "a legacy row must not invent a kind: {html}"
        );
        assert!(
            html.contains("data-provenance-status=\"unrecorded\""),
            "a row with no kind, hash or gate has no recorded status: {html}"
        );
        assert!(
            html.contains("data-provenance-gate=\"unrecorded\""),
            "{html}"
        );
        assert!(
            html.contains("data-provenance-commit=\"unrecorded\""),
            "{html}"
        );
    }

    #[test]
    fn unsafe_link_schemes_are_inert() {
        let html = render_markdown("[x](javascript:alert(1)) [y](JaVaScRiPt:alert(1))");
        assert!(!html.to_ascii_lowercase().contains("javascript:"), "{html}");
        assert!(html.contains("href=\"#\""), "{html}");
        assert!(html.contains(">x</a>"), "{html}");
    }

    #[test]
    fn safe_links_survive() {
        let html =
            render_markdown("[docs](https://example.com/a) [mail](mailto:a@b.c) [rel](./a.md)");
        assert!(html.contains("href=\"https://example.com/a\""), "{html}");
        assert!(html.contains("href=\"mailto:a@b.c\""), "{html}");
        assert!(html.contains("href=\"./a.md\""), "{html}");
    }

    #[test]
    fn the_page_uses_the_pack_assets_and_escapes_the_title() {
        let attachments = [AttachmentEntry {
            id: "a-1".to_owned(),
            filename: "evidence.png".to_owned(),
            content_type: "image/png".to_owned(),
            size_bytes: 8,
            path: "docs/attachment-a-1".to_owned(),
        }];
        let page = output_page(
            "<T>",
            "<p>x</p>",
            "2026-11-13T00:00:00Z",
            Some("S02"),
            Some("A Person"),
            &attachments,
            &Provenance::default(),
        );
        assert!(
            page.contains("href=\"attachment-a-1\""),
            "an attachment is a sibling of the output page"
        );
        assert!(
            !page.contains("href=\"docs/attachment-a-1\""),
            "the outputs.js path is not the page link"
        );
        assert!(page.contains("A Person"), "the author is credited");
        assert!(
            page.contains("../docs.css"),
            "the page must share the pack CSS"
        );
        assert!(page.contains("&lt;T&gt;"), "the title is escaped");
        assert!(page.contains("<p>x</p>"), "the rendered body is inserted");
        assert!(
            page.contains("S02 · conveyor output"),
            "the project is shown"
        );
    }

    #[test]
    fn the_index_is_a_js_assignment_with_the_schema() {
        let entries = vec![OutputEntry {
            id: "id-1".to_owned(),
            title: "A note".to_owned(),
            project_id: Some("p-1".to_owned()),
            project_key: Some("S02".to_owned()),
            project_name: Some("Skeleton".to_owned()),
            category: Some("evidence".to_owned()),
            unit: Some("S02".to_owned()),
            author: Some("A Person".to_owned()),
            attachments: Vec::new(),
            path: "docs/output-id-1.html".to_owned(),
            published_at: Some("2026-11-13T00:00:00Z".to_owned()),
            updated_at: "2026-11-13T00:00:00Z".to_owned(),
            provenance: Provenance::default(),
        }];
        let js = outputs_js(&entries, "2026-11-13T00:00:00Z").expect("index");
        assert!(js.starts_with("window.CONVEYOR_OUTPUTS = {"), "{js}");
        assert!(js.contains("\"schema\":\"design-outputs/v1\""), "{js}");
        assert!(
            js.contains("\"published_at\":\"2026-11-13T00:00:00Z\""),
            "{js}"
        );
        assert!(js.contains("A note"), "{js}");
    }
}
