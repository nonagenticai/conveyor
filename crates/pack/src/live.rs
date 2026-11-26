#![forbid(unsafe_code)]
//! The live public view: the snapshot and the published outputs, built in memory.
//!
//! The unauthenticated `/design` routes call [`site_view`]. It reads **everything in one
//! read-only transaction** — the snapshot data, the published documents and the projects —
//! so the two projections cannot disagree about a row that changed between two reads.
//!
//! **Deterministic.** `generated_at` is the newest published document's `updated_at`, never the
//! request clock; an empty tenant gets [`EMPTY_GENERATED_AT`]. Two reads of an unchanged database
//! are byte-identical, which is what makes the HTTP `ETag` stable.
//!
//! **Redacted and safe.** Titles and bodies pass the snapshot's `redact` safety net before they
//! are rendered. The Markdown renderer (in [`crate::outputs`]) neutralises raw HTML and refuses
//! non-http(s)/mailto URLs. Only `published_at IS NOT NULL AND status='active'` rows with a body are visible —
//! the same filter the CLI writer uses.

use std::collections::HashMap;

use conveyor_domain::TenantId;
use conveyor_store as store;
use conveyor_store::document::DocumentRecord;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::outputs::{PublishOutputsError, render_outputs};
use crate::snapshot::{SnapshotError, feedback_issues, redact_text, section_json, snapshot_js};

/// The clock an empty tenant reports: a constant, so the bytes never depend on the request time.
pub const EMPTY_GENERATED_AT: &str = "1970-01-01T00:00:00Z";

/// Everything the public routes serve for one tenant.
#[derive(Debug, Clone)]
pub struct SiteView {
    /// `window.CONVEYOR_SNAPSHOT = {…};` — the programme, organisation, config, work, artefacts,
    /// architecture, ops and measure sections, plus the issue registry and feedback list.
    pub snapshot_js: String,
    /// `window.CONVEYOR_OUTPUTS = {…};` — the published-output index.
    pub outputs_js: String,
    /// `/design/data/programme.json` — the programme section as the standalone file the CLI
    /// writes, so a reader can download the same rows the page renders.
    pub programme_json: String,
    /// `/design/data/programme.csv` — the same projects, one line each.
    pub programme_csv: String,
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
    /// The committed rework register (`conveyor-rework/v1`), when this image carries one.
    /// Absent means the wrapper publishes no `rework` section.
    pub rework: Option<Value>,
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

    // ⛔ The live view is the PROGRAMME's view. A project outside a programme is a session
    // fixture (a proof, a scratch reproduction), so its page row and its published documents are
    // not public — the same rule `snapshot_tx` applies to the work sections (SPEC_V2 §5).
    let bound: std::collections::HashSet<Uuid> = projects
        .iter()
        .filter(|project| project.programme_id.is_some())
        .map(|project| project.id)
        .collect();
    let projects: Vec<_> = projects
        .into_iter()
        .filter(|project| project.programme_id.is_some())
        .collect();
    let records = in_programme(records, &bound, |record| record.project_id);
    let attachments = in_programme(attachments, &bound, |attachment| attachment.project_id);

    let generated_at = generated_at(&records);

    // The issue registry is a commit-time artefact, not a database row: inject it before the
    // wrapper is built, exactly as `snapshot::run_with_pool` does.
    let feedback = feedback_issues(&args.issues);
    if let Some(object) = data.programme.as_object_mut() {
        object.insert("issue_registry".to_owned(), args.issues.clone());
        object.insert("feedback".to_owned(), Value::Array(feedback));
    }
    // The declared-vs-evidenced register, built from the same rows this view renders. The
    // committed gap register is a checkout artefact; a pod without one reports the posture as
    // absent rather than inventing zero gaps.
    let register = crate::snapshot::with_header(
        crate::register::build(
            &data.programme,
            &crate::register::from_records(&records),
            crate::register::read_gaps(&std::env::current_dir()?).as_ref(),
        ),
        crate::snapshot::SCHEMA,
        &generated_at,
    );
    // The rework register is a commit-time artefact like the issue registry: it is carried into
    // the wrapper from `args`, and omitted from it when the image has no register.
    let snapshot_js = snapshot_js(
        &data,
        args.rework.as_ref(),
        Some(&register),
        args.tenant_slug,
        args.feedback_url,
        &generated_at,
        args.git_sha,
    )?;
    // The downloadable pair, from the same section the page renders — one header, one redaction
    // pass, so the file and the page cannot disagree.
    let programme_json = section_json(&data.programme, &generated_at)?;
    let programme_csv = programme_csv(&data.programme);

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
        crate::outputs::rendered_commit().as_deref(),
    )?;

    Ok(SiteView {
        snapshot_js,
        outputs_js,
        programme_json,
        programme_csv,
        pages,
        generated_at,
    })
}

/// The programme's projects as CSV — the download for a reader who does not want the JSON.
///
/// Columns: `key,name,tranche,status,blocked_by,units_open,units_claimed,issues,plan`.
/// `blocked_by` names the declared dependencies that are not `closed`; unit counts come from the
/// committed issue registry the programme section carries; `plan` is the project's plan path.
/// Ordered by key, like the JSON.
pub(crate) fn programme_csv(programme: &Value) -> String {
    const HEADER: &str = "key,name,tranche,status,blocked_by,units_open,units_claimed,issues,plan";
    let empty = Vec::new();
    let projects = programme
        .get("projects")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let registry = programme
        .get("issue_registry")
        .and_then(|registry| registry.get("issues"))
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let by_number: HashMap<i64, &Value> = registry
        .iter()
        .filter_map(|issue| Some((issue.get("number")?.as_i64()?, issue)))
        .collect();
    let status_by_key: HashMap<&str, &str> = projects
        .iter()
        .filter_map(|project| {
            Some((
                project.get("key")?.as_str()?,
                project.get("status")?.as_str()?,
            ))
        })
        .collect();

    let mut out = String::from(HEADER);
    out.push('\n');
    for project in projects {
        let key = project
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let meta = project.get("meta").cloned().unwrap_or(Value::Null);
        let strings = |field: &str| -> Vec<String> {
            meta.get(field)
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let numbers: Vec<i64> = meta
            .get("issues")
            .and_then(Value::as_array)
            .map(|rows| rows.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default();

        let depends_on = strings("depends_on");
        let blocked_by: Vec<&str> = depends_on
            .iter()
            .map(String::as_str)
            .filter(|dependency| status_by_key.get(dependency).copied() != Some("closed"))
            .collect();
        let mut open = 0;
        let mut claimed = 0;
        for number in &numbers {
            let Some(issue) = by_number.get(number) else {
                continue;
            };
            if issue.get("state").and_then(Value::as_str) == Some("OPEN") {
                open += 1;
                let is_claimed = issue
                    .get("labels")
                    .and_then(Value::as_array)
                    .map(|labels| {
                        labels.iter().any(|label| {
                            label.get("name").and_then(Value::as_str) == Some("status:claimed")
                        })
                    })
                    .unwrap_or(false);
                if is_claimed {
                    claimed += 1;
                }
            }
        }
        let issues: Vec<String> = numbers.iter().map(|number| format!("#{number}")).collect();
        let fields = [
            key.to_owned(),
            project
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            meta.get("tranche").map(tranche_text).unwrap_or_default(),
            project
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            blocked_by.join(";"),
            open.to_string(),
            claimed.to_string(),
            issues.join(";"),
            meta.get("plan")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        ];
        out.push_str(
            &fields
                .iter()
                .map(|field| csv_field(field))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

fn tranche_text(value: &Value) -> String {
    value
        .as_i64()
        .map(|number| number.to_string())
        .or_else(|| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// RFC 4180 quoting: a field with a comma, a quote or a newline is quoted and its quotes doubled.
fn csv_field(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
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

/// Keep the rows whose project is part of the programme. A row with no project (a tenant-level
/// note the tenant chose to publish) stays: the filter removes fixtures, not orphans.
fn in_programme<T>(
    rows: Vec<T>,
    bound: &std::collections::HashSet<Uuid>,
    project_of: impl Fn(&T) -> Option<Uuid>,
) -> Vec<T> {
    rows.into_iter()
        .filter(|row| project_of(row).is_none_or(|id| bound.contains(&id)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_tenant_has_a_constant_clock() {
        assert_eq!(generated_at(&[]), EMPTY_GENERATED_AT);
    }

    /// The download's shape: one header, one line per project, blockers and unit counts derived.
    #[test]
    fn the_programme_csv_names_the_blockers_and_counts_the_units() {
        let programme = serde_json::json!({
            "projects": [
                {"key": "A", "name": "Closed, with a comma", "status": "closed",
                 "meta": {"tranche": 1, "depends_on": [], "issues": [], "plan": "docs/a.md"}},
                {"key": "B", "name": "Draft", "status": "draft",
                 "meta": {"tranche": 2, "depends_on": ["A", "C"], "issues": [2, 3],
                          "plan": "docs/b.md"}},
                {"key": "C", "name": "Also draft", "status": "draft",
                 "meta": {"tranche": 3, "depends_on": [], "issues": [], "plan": "docs/c.md"}},
            ],
            "issue_registry": {"issues": [
                {"number": 2, "state": "OPEN", "labels": [{"name": "status:claimed"}]},
                {"number": 3, "state": "CLOSED", "labels": []},
            ]},
        });
        let csv = programme_csv(&programme);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "key,name,tranche,status,blocked_by,units_open,units_claimed,issues,plan"
        );
        assert_eq!(lines.len(), 4, "header plus three projects");
        assert_eq!(
            lines[1], "A,\"Closed, with a comma\",1,closed,,0,0,,docs/a.md",
            "a comma is quoted; a closed dependency does not block"
        );
        assert_eq!(
            lines[2], "B,Draft,2,draft,C,1,1,#2;#3,docs/b.md",
            "only the unfinished dependency blocks; the open claimed unit counts"
        );
    }

    /// P2.1 of #382: an unbound project's page row and published documents are not public.
    #[test]
    fn an_unbound_project_is_not_public() {
        let t0 = Uuid::now_v7();
        let t1 = Uuid::now_v7();
        let bound: std::collections::HashSet<Uuid> = [t0, t1].into_iter().collect();
        let unbound = Uuid::now_v7();
        let rows = vec![
            (Some(t0), "the programme's own"),
            (Some(t1), "bound"),
            (Some(unbound), "a scratch fixture"),
            (None, "a tenant-level note"),
        ];
        let kept = in_programme(rows, &bound, |row| row.0);
        let names: Vec<&str> = kept.iter().map(|row| row.1).collect();
        assert_eq!(
            names,
            vec!["the programme's own", "bound", "a tenant-level note"],
            "an unbound project's row and its documents are filtered; orphan rows stay"
        );
    }
}
