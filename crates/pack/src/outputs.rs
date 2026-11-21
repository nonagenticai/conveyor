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
//! body are rendered, and only when they belong to the programme: a project outside the programme
//! is a session fixture, exactly as in the snapshot's `artefacts` section. A row that exists but
//! was never published is absent from both the page set and the index — there is no "all
//! documents" fallback.
//!
//! **The same redaction as the JSON.** A title, body, attachment name, project name, author or
//! output-standard slot passes [`crate::snapshot::redact_text`] before it is rendered, so a token
//! pasted into a published output cannot reach a page while the JSON copy is masked.
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
    Ok(write_outputs_with_files(pool, tenant, generated_at, out)
        .await?
        .0)
}

/// One file [`write_outputs`] wrote: its name inside the outputs directory and the exact bytes.
pub(crate) struct WrittenOutput {
    /// File name (`output-<id>.html` or `outputs.js`).
    pub name: String,
    /// The bytes written; hashed into the snapshot manifest.
    pub bytes: Vec<u8>,
}

/// [`write_outputs`], plus the exact files it wrote and their bytes.
///
/// The snapshot's manifest hashes every published file (SPEC §10.3); the caller needs the
/// written bytes, not a directory scan — `out` may also hold the tracked document pages.
pub(crate) async fn write_outputs_with_files(
    pool: &PgPool,
    tenant: TenantId,
    generated_at: &str,
    out: &Path,
) -> Result<(usize, Vec<WrittenOutput>), PublishOutputsError> {
    let mut tx = store::begin_scoped_read_only(pool, tenant).await?;
    let records = store::document::list_published_outputs_tx(&mut tx, tenant).await?;
    let projects = store::project::list_projects_tx(&mut tx, tenant, None, 1000).await?;
    let attachments = store::document::list_published_attachments_tx(&mut tx, tenant).await?;
    let names = author_names(&mut tx, &records).await?;
    tx.rollback().await?;

    let (index, pages) = render_outputs(&records, &projects, &attachments, &names, generated_at)?;

    std::fs::create_dir_all(out)?;
    let mut written = Vec::with_capacity(pages.len() + 1);
    for (id, page) in &pages {
        let name = format!("output-{id}.html");
        std::fs::write(out.join(&name), page)?;
        written.push(WrittenOutput {
            name,
            bytes: page.as_bytes().to_vec(),
        });
    }
    std::fs::write(out.join("outputs.js"), &index)?;
    written.push(WrittenOutput {
        name: "outputs.js".to_owned(),
        bytes: index.into_bytes(),
    });
    // A HashMap's order is not stable; the manifest needs the same list on every run.
    written.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((pages.len(), written))
}

/// Render the published set from rows the caller already read.
///
/// The caller owns the transaction: [`write_outputs`] opens one for the CLI, and
/// [`crate::live::site_view`] reads the same rows inside the transaction it already holds. One
/// renderer keeps the two byte-identical — and it is the ONE place the publication filter and the
/// snapshot's redaction are applied, so a CLI page and a live page cannot disagree about either.
///
/// A record or attachment whose project is outside the programme is a session fixture and is not
/// rendered (the rule `conveyor_store::snapshot` applies to `artefacts.documents`; a row with no
/// project is a tenant-level note and stays). Free text is redacted before it reaches the page,
/// the index or the CSV-equivalent view of the same rows.
pub fn render_outputs(
    records: &[DocumentRecord],
    projects: &[Project],
    attachments: &[DocumentRecord],
    names: &HashMap<Uuid, String>,
    generated_at: &str,
) -> Result<(String, HashMap<Uuid, String>), PublishOutputsError> {
    // The programme boundary, by the same predicate `live::site_view` publishes.
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
        let title = redact_text(&record.title);
        let body = redact_text(record.body.as_deref().unwrap_or_default());
        let (project_key, project_name) = record
            .project_id
            .and_then(|id| projects.get(&id))
            .map_or((None, None), |(key, name)| {
                (Some(key.clone()), Some(name.clone()))
            });
        let project_label = project_name.as_deref().or(project_key.as_deref());
        let author = record
            .created_by
            .and_then(|id| names.get(&id))
            .map(|name| redact_text(name.as_str()));
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
        let page = output_page(
            &title,
            &render_markdown(&body),
            generated_at,
            project_label,
            author.as_deref(),
            &attachment_entries,
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
) -> String {
    let mut source = match project {
        Some(project) => format!("{project} · conveyor output · published {generated_at}"),
        None => format!("conveyor output · published {generated_at}"),
    };
    if let Some(author) = author.filter(|author| !author.is_empty()) {
        source.push_str(&format!(" · {author}"));
    }
    let body_html = format!(
        "{body_html}{}{}",
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
    use chrono::{TimeZone, Utc};

    fn project(id: Uuid, key: &str, name: &str, programme_id: Option<Uuid>) -> Project {
        Project {
            id,
            key: key.to_owned(),
            name: name.to_owned(),
            description: String::new(),
            status: "active".to_owned(),
            programme_id,
            project_type_code: None,
            phase_code: None,
            owner_user_id: None,
            repo_url: None,
            meta: serde_json::json!({}),
            version: conveyor_domain::Version::new(1).expect("version"),
        }
    }

    fn document(id: Uuid, project_id: Option<Uuid>, title: &str, body: &str) -> DocumentRecord {
        DocumentRecord {
            id,
            project_id,
            card_id: None,
            parent_document_id: None,
            kind: "document".to_owned(),
            title: title.to_owned(),
            status: "active".to_owned(),
            object_key: None,
            content_hash: None,
            body: Some(body.to_owned()),
            external_ref: None,
            meta: serde_json::json!({}),
            publishable: true,
            category: Some("evidence".to_owned()),
            unit: Some("T1".to_owned()),
            created_by: None,
            updated_by: None,
            published_at: Some(Utc.timestamp_opt(1_700_000_000, 0).unwrap()),
            published_by: None,
            publish_requested_at: None,
            version: conveyor_domain::Version::new(1).expect("version"),
            created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            updated_at: Utc.timestamp_opt(1_700_000_100, 0).unwrap(),
        }
    }

    fn attachment(id: Uuid, parent: Uuid, project_id: Option<Uuid>, name: &str) -> DocumentRecord {
        let mut row = document(id, project_id, name, "");
        row.kind = "attachment".to_owned();
        row.parent_document_id = Some(parent);
        row.object_key = Some(format!("attachments/{id}"));
        row.body = None;
        row.meta = serde_json::json!({"content_type": "text/plain", "size_bytes": 4});
        row
    }

    /// v1-pack-6: the page and the index apply the JSON record's redaction — a pasted token or
    /// email in a title, body, attachment name, project name, author or slot never publishes.
    #[test]
    fn a_secret_in_an_output_never_reaches_the_page_or_the_index() {
        let token = format!("ghp_{}", "A".repeat(30));
        let programme = Uuid::now_v7();
        let project_id = Uuid::now_v7();
        let row_id = Uuid::now_v7();
        let author_id = Uuid::now_v7();
        let projects = [project(
            project_id,
            "T1",
            &format!("Project {token}"),
            Some(programme),
        )];
        let mut row = document(
            row_id,
            Some(project_id),
            "Contact alice@example.com",
            &format!("# Note\n\npasted {token} here\n"),
        );
        row.created_by = Some(author_id);
        row.category = Some(format!("evidence {token}"));
        row.unit = Some("alice@example.com".to_owned());
        let attachments = [attachment(
            Uuid::now_v7(),
            row_id,
            Some(project_id),
            &format!("{token}.txt"),
        )];
        let names: HashMap<Uuid, String> = [(author_id, "alice@example.com".to_owned())]
            .into_iter()
            .collect();

        let (index, pages) = render_outputs(
            &[row],
            &projects,
            &attachments,
            &names,
            "2026-11-13T00:00:00Z",
        )
        .expect("render");
        let page = pages.get(&row_id).expect("the page");

        for surface in [page.as_str(), index.as_str()] {
            assert!(
                !surface.contains(&token),
                "the token is redacted: {surface}"
            );
            assert!(
                !surface.contains("alice@example.com"),
                "the email is redacted: {surface}"
            );
            assert!(
                surface.contains("[redacted-token]"),
                "the token is visibly masked: {surface}"
            );
            assert!(
                surface.contains("***@redacted"),
                "the email is visibly masked: {surface}"
            );
        }
        assert!(
            page.contains("[redacted-token].txt"),
            "an attachment file name is redacted: {page}"
        );
    }

    /// v1-pack-6: an output whose project is outside the programme is a session fixture — it has
    /// no page and no index row, matching the snapshot's `artefacts.documents` boundary.
    #[test]
    fn an_output_outside_the_programme_is_not_rendered() {
        let programme = Uuid::now_v7();
        let bound = Uuid::now_v7();
        let fixture = Uuid::now_v7();
        let projects = [
            project(bound, "BOUND", "In the programme", Some(programme)),
            project(fixture, "FIX", "A scratch fixture", None),
        ];
        let shown = document(Uuid::now_v7(), Some(bound), "Bound note", "public");
        let hidden = document(Uuid::now_v7(), Some(fixture), "Fixture note", "not public");
        let hidden_id = hidden.id;
        let orphan = document(Uuid::now_v7(), None, "Tenant note", "stays");
        let orphan_id = orphan.id;

        let (index, pages) = render_outputs(
            &[shown, hidden, orphan],
            &projects,
            &[],
            &HashMap::new(),
            "2026-11-13T00:00:00Z",
        )
        .expect("render");
        assert!(pages.values().any(|page| page.contains("Bound note")));
        assert!(pages.values().any(|page| page.contains("Tenant note")));
        assert!(
            !pages.contains_key(&hidden_id),
            "a fixture project's output has no page"
        );
        assert!(
            pages.contains_key(&orphan_id),
            "a tenant-level note stays public"
        );
        assert!(!index.contains("Fixture note"), "{index}");
    }

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
