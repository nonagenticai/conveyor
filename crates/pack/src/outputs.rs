#![forbid(unsafe_code)]
//! Platform outputs on the public design site: one standalone HTML page per published output,
//! plus the `outputs.js` index the Documents destination reads.
//!
//! **Static, not runtime.** The pack has no Markdown renderer and makes no request; a published
//! body is rendered here, at publish time, into `docs/output-<id>.html`. `outputs.js` is the
//! index (`window.CONVEYOR_OUTPUTS = {…}`), and `docs/design/docs.js` only lists it.
//!
//! **The filter is the publication decision.** Only `publishable AND status='active'` rows with a
//! body are rendered. A row that exists but was never published is absent from both the page set
//! and the index — there is no "all documents" fallback.
//!
//! **Author input is untrusted.** Raw HTML in a body is turned into text, and a link or image
//! whose URL names a non-http(s)/mailto scheme is made inert. The public-page test carries
//! `<script>`, an `onerror` attribute and a `javascript:` link.

use std::collections::HashMap;
use std::path::Path;

use conveyor_domain::Project;
use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::document::DocumentRecord;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

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
    let mut tx = store::begin_scoped_read_only(pool, tenant).await?;
    let records = store::document::list_published_outputs_tx(&mut tx, tenant).await?;
    let projects = store::project::list_projects_tx(&mut tx, tenant, None, 1000).await?;
    let attachments = store::document::list_published_attachments_tx(&mut tx, tenant).await?;
    let names = author_names(&mut tx, &records).await?;
    tx.rollback().await?;

    let (index, pages) = render_outputs(&records, &projects, &attachments, &names, generated_at)?;

    std::fs::create_dir_all(out)?;
    for (id, page) in &pages {
        std::fs::write(out.join(format!("output-{id}.html")), page)?;
    }
    std::fs::write(out.join("outputs.js"), index)?;
    Ok(pages.len())
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
) -> Result<(String, HashMap<Uuid, String>), PublishOutputsError> {
    let projects: HashMap<Uuid, (String, String)> = projects
        .iter()
        .map(|project| (project.id, (project.key.clone(), project.name.clone())))
        .collect();
    let mut by_parent: HashMap<Uuid, Vec<&DocumentRecord>> = HashMap::new();
    for attachment in attachments {
        if let Some(parent) = attachment.parent_document_id {
            by_parent.entry(parent).or_default().push(attachment);
        }
    }

    let mut entries = Vec::with_capacity(records.len());
    let mut pages = HashMap::with_capacity(records.len());
    for record in records {
        let body = record.body.as_deref().unwrap_or_default();
        let (project_key, project_name) = record
            .project_id
            .and_then(|id| projects.get(&id).cloned())
            .map_or((None, None), |(key, name)| (Some(key), Some(name)));
        let project_label = project_name.as_deref().or(project_key.as_deref());
        let author = record.created_by.and_then(|id| names.get(&id).cloned());
        let attachment_entries: Vec<AttachmentEntry> = by_parent
            .get(&record.id)
            .map(|rows| {
                rows.iter()
                    .map(|row| AttachmentEntry {
                        id: row.id.to_string(),
                        filename: row.title.clone(),
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
            &record.title,
            &render_markdown(body),
            generated_at,
            project_label,
            author.as_deref(),
            &attachment_entries,
        );
        pages.insert(record.id, page);
        entries.push(OutputEntry {
            id: record.id.to_string(),
            title: record.title.clone(),
            project_id: record.project_id.map(|id| id.to_string()),
            project_key,
            project_name,
            category: record.category.clone(),
            unit: record.unit.clone(),
            author,
            attachments: attachment_entries,
            path: format!("docs/output-{}.html", record.id),
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
    let mut ids: Vec<Uuid> = records.iter().filter_map(|record| record.created_by).collect();
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

/// Render a Markdown body to HTML with raw HTML neutralised and unsafe URL schemes refused.
pub fn render_markdown(markdown: &str) -> String {
    let parser = Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    );
    let events = parser.map(|event| match event {
        // An author's raw HTML is a design note, not markup for this site. Display it as text;
        // the HTML writer escapes it. Dropping it would silently lose the author's words.
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) if !safe_href(&dest_url) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from("#"),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) if !safe_href(&dest_url) => Event::Start(Tag::Image {
            link_type,
            dest_url: CowStr::from("#"),
            title,
            id,
        }),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// A URL is safe when it is relative, a fragment, or an `http`, `https` or `mailto` URL.
///
/// The scheme is read before the first `/`, `?` or `#`; anything else is refused, including
/// `javascript:`, `data:`, `vbscript:` and `file:` with any leading whitespace or mixed case.
fn safe_href(raw: &str) -> bool {
    let value = raw.trim();
    if value.is_empty() || value.starts_with('#') || value.starts_with('/') {
        return true;
    }
    if value.starts_with("./") || value.starts_with("../") {
        return true;
    }
    let scheme_end = value.find(['/', '?', '#']).unwrap_or(value.len());
    let head = &value[..scheme_end];
    let Some(colon) = head.find(':') else {
        return true; // a relative URL with no scheme
    };
    let scheme = head[..colon].to_ascii_lowercase();
    matches!(scheme.as_str(), "http" | "https" | "mailto")
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
    let body_html = format!("{body_html}{}", attachments_html(attachments));
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
        out.push_str(&format!(
            "<li><a href=\"{}\" rel=\"noopener\">{}</a> <span class=\"muted\">{}</span></li>",
            escape_html(&attachment.path),
            escape_html(&attachment.filename),
            escape_html(&attachment.content_type),
        ));
    }
    out.push_str("</ul>");
    out
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
        let page = output_page("<T>", "<p>x</p>", "2026-11-13T00:00:00Z", Some("S02"), None, &[]);
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
            updated_at: "2026-11-13T00:00:00Z".to_owned(),
        }];
        let js = outputs_js(&entries, "2026-11-13T00:00:00Z").expect("index");
        assert!(js.starts_with("window.CONVEYOR_OUTPUTS = {"), "{js}");
        assert!(js.contains("\"schema\":\"design-outputs/v1\""), "{js}");
        assert!(js.contains("A note"), "{js}");
    }
}
