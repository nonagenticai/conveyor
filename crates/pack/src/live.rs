#![forbid(unsafe_code)]
//! The live public view: the snapshot and the published outputs, built in memory.
//!
//! The unauthenticated `/design` routes call [`site_view`]. It reads **everything in one
//! read-only transaction** — the snapshot data, the publishable documents and the projects —
//! so the two projections cannot disagree about a row that changed between two reads.
//!
//! **Deterministic.** `generated_at` is the newest published document's `updated_at`, never the
//! request clock; an empty tenant gets [`EMPTY_GENERATED_AT`]. Two reads of an unchanged database
//! are byte-identical, which is what makes the HTTP `ETag` stable.
//!
//! **Redacted and safe.** Titles and bodies pass the snapshot's `redact` safety net before they
//! are rendered. The Markdown renderer (in [`crate::outputs`]) neutralises raw HTML and refuses
//! non-http(s)/mailto URLs. Only `publishable AND status='active'` rows with a body are visible —
//! the same filter the CLI writer uses.

use std::collections::HashMap;

use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::document::DocumentRecord;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::outputs::{PublishOutputsError, render_outputs};
use crate::snapshot::{SnapshotError, feedback_issues, redact_text, snapshot_js};

/// The clock an empty tenant reports: a constant, so the bytes never depend on the request time.
pub const EMPTY_GENERATED_AT: &str = "1970-01-01T00:00:00Z";

/// Everything the public routes serve for one tenant.
#[derive(Debug, Clone)]
pub struct SiteView {
    /// `window.CONVEYOR_SNAPSHOT = {…};` — the programme, organisation, config, work, artefacts
    /// and measure sections, plus the issue registry and feedback list.
    pub snapshot_js: String,
    /// `window.CONVEYOR_OUTPUTS = {…};` — the published-output index.
    pub outputs_js: String,
    /// One rendered page per published output, by document id.
    pub pages: HashMap<Uuid, String>,
    /// The newest published document's `updated_at`, or [`EMPTY_GENERATED_AT`].
    pub generated_at: String,
}

/// What the caller supplies beyond the database.
pub struct SiteArgs<'a> {
    /// The tenant whose published rows are public.
    pub tenant: TenantId,
    /// The tenant slug, as the snapshot carries it.
    pub tenant_slug: &'a str,
    /// The committed issue registry (`{"issues": [...]}` or a bare array).
    pub issues: Value,
    /// Linked from every page.
    pub feedback_url: &'a str,
    /// The commit the served image was built from.
    pub git_sha: &'a str,
}

/// Build the live view for one tenant in exactly one read-only transaction.
pub async fn site_view(pool: &PgPool, args: SiteArgs<'_>) -> Result<SiteView, SnapshotError> {
    let mut tx = store::begin_scoped_read_only(pool, args.tenant).await?;
    let mut data = store::snapshot::snapshot_tx(&mut tx, args.tenant).await?;
    let records = store::document::list_published_outputs_tx(&mut tx, args.tenant)
        .await
        .map_err(PublishOutputsError::from)?;
    let projects = store::project::list_projects_tx(&mut tx, args.tenant, None, 1000).await?;
    let attachments = store::document::list_published_attachments_tx(&mut tx, args.tenant)
        .await
        .map_err(PublishOutputsError::from)?;
    let names = crate::outputs::author_names(&mut tx, &records).await?;
    tx.rollback().await?;

    let generated_at = generated_at(&records);

    // The issue registry is a commit-time artefact, not a database row: inject it before the
    // wrapper is built, exactly as `snapshot::run_with_pool` does.
    let feedback = feedback_issues(&args.issues);
    if let Some(object) = data.programme.as_object_mut() {
        object.insert("issue_registry".to_owned(), args.issues.clone());
        object.insert("feedback".to_owned(), Value::Array(feedback));
    }
    let snapshot_js = snapshot_js(
        &data,
        args.tenant_slug,
        args.feedback_url,
        &generated_at,
        args.git_sha,
    )?;

    // Apply the snapshot's safety net to the one free text the public site renders from a
    // document row. The rows themselves come from a query that selects explicit columns.
    let mut redacted = records;
    for record in &mut redacted {
        record.title = redact_text(&record.title);
        if let Some(body) = record.body.as_deref() {
            record.body = Some(redact_text(body));
        }
    }
    // An attachment's title is the file name the public page shows; it is free text too.
    let mut redacted_attachments = attachments;
    for attachment in &mut redacted_attachments {
        attachment.title = redact_text(&attachment.title);
    }
    let (outputs_js, pages) = render_outputs(
        &redacted,
        &projects,
        &redacted_attachments,
        &names,
        &generated_at,
    )?;

    Ok(SiteView {
        snapshot_js,
        outputs_js,
        pages,
        generated_at,
    })
}

/// The newest published document's timestamp, or the empty-tenant constant.
fn generated_at(records: &[DocumentRecord]) -> String {
    records
        .iter()
        .map(|record| record.updated_at)
        .max()
        .map(|at| at.to_rfc3339())
        .unwrap_or_else(|| EMPTY_GENERATED_AT.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_tenant_has_a_constant_clock() {
        assert_eq!(generated_at(&[]), EMPTY_GENERATED_AT);
    }
}
