#![forbid(unsafe_code)]
//! Programme ordering — a project may not start while a declared dependency is open, and a
//! delivery project may not start until its inception outputs are published.
//!
//! `project.meta.depends_on` is the programme's declared order (`seed/programme.json`, written
//! by the seed loader) and `project.meta.expected_outputs` is its output standard. Both are
//! enforced at the two run-start paths, never left to the UI:
//!
//! * `process_runs.start` — a card's run (`crates/actions/src/process/runs.rs`);
//! * `projects.execute` — the project's durable execution task.
//!
//! A dependency is met when its project is `closed` or `archived` (an archived/superseded slice
//! cannot gate a successor forever). A key that names no project fails closed: a programme with a
//! dangling dependency is a data defect, not an unblocked project.

use std::collections::HashMap;

use conveyor_domain::Project;
use conveyor_store::{TenantId, document as document_store, project as project_store};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::ActionError;

/// The page size while walking the tenant's projects. The programme is 47 today; the loop is
/// paginated so a larger tenant measures exactly the same way.
const PAGE: i64 = 200;

/// The dependency keys a project declares, trimmed, de-duplicated and in declaration order.
pub(crate) fn dependency_keys(meta: &Value) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    if let Some(items) = meta.get("depends_on").and_then(Value::as_array) {
        for item in items {
            let Some(key) = item.as_str().map(str::trim).filter(|key| !key.is_empty()) else {
                continue;
            };
            if !keys.iter().any(|seen| seen == key) {
                keys.push(key.to_owned());
            }
        }
    }
    keys
}

/// Refuse with `409 blocked_by` unless every project named in `meta.depends_on` is met.
///
/// The error names the project and the open keys, so the console and an agent see the same
/// reason. With no dependencies the call is a no-op and the existing start behaviour is
/// unchanged.
pub(crate) async fn assert_dependencies_met(
    tx: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(), ActionError> {
    let project = project_store::get_project_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "programme: read project");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("project"))?;
    let waits_on = dependency_keys(&project.meta);
    if waits_on.is_empty() {
        return Ok(());
    }

    let mut all: Vec<Project> = Vec::new();
    let mut after: Option<(String, Uuid)> = None;
    loop {
        let page = project_store::list_projects_tx(
            &mut *tx,
            tenant,
            after.as_ref().map(|(key, id)| (key.as_str(), *id)),
            PAGE,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "programme: list projects");
            ActionError::internal()
        })?;
        let count = page.len();
        if let Some(last) = page.last() {
            after = Some((last.key.clone(), last.id));
        }
        all.extend(page);
        if count < PAGE as usize {
            break;
        }
    }

    let by_key: HashMap<&str, &Project> = all.iter().map(|p| (p.key.as_str(), p)).collect();
    let open: Vec<String> = waits_on
        .into_iter()
        .filter(|key| {
            by_key
                .get(key.as_str())
                .is_none_or(|dependency| !is_met(&dependency.status))
        })
        .collect();
    if open.is_empty() {
        return Ok(());
    }
    Err(ActionError::new(
        409,
        "blocked_by",
        format!(
            "project {} is blocked by open {}: {}",
            project.key,
            if open.len() == 1 {
                "dependency"
            } else {
                "dependencies"
            },
            open.join(", ")
        ),
    ))
}

/// The two inception categories the output standard declares for a delivery project (P0's
/// inception standard, #221's canonical spelling). A project that names both in
/// `meta.expected_outputs` may not start a build run until both are published.
pub(crate) const INCEPTION_CATEGORIES: [&str; 2] = ["inception-analysis", "architecture-design"];

/// The output-standard categories a project declares it will produce, trimmed, de-duplicated and
/// in declaration order. Absent or non-array `expected_outputs` is empty — a project that declares
/// nothing is subject to no output gate.
pub(crate) fn expected_outputs(meta: &Value) -> Vec<String> {
    let mut outputs: Vec<String> = Vec::new();
    if let Some(items) = meta.get("expected_outputs").and_then(Value::as_array) {
        for item in items {
            let Some(slot) = item.as_str().map(str::trim).filter(|slot| !slot.is_empty()) else {
                continue;
            };
            if !outputs.iter().any(|seen| seen.as_str() == slot) {
                outputs.push(slot.to_owned());
            }
        }
    }
    outputs
}

/// Refuse with `409 inception_missing` unless every inception category the project declares is
/// published.
///
/// The switch is the project's own `meta.expected_outputs`: a project that names neither
/// category (P0, the platform, every V1-built slice) is unchanged, and a project that gains the
/// pair applies the gate without a code change. Publication is `published_at IS NOT NULL` on an
/// active document of this project with the category — a draft is not publication.
pub(crate) async fn assert_inception_published(
    tx: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(), ActionError> {
    let project = project_store::get_project_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "inception: read project");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("project"))?;
    let outputs = expected_outputs(&project.meta);
    let required: Vec<&str> = INCEPTION_CATEGORIES
        .iter()
        .copied()
        .filter(|category| outputs.iter().any(|slot| slot.as_str() == *category))
        .collect();
    if required.is_empty() {
        return Ok(());
    }
    let published = document_store::published_categories_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "inception: read published categories");
            ActionError::internal()
        })?;
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|category| !published.iter().any(|slot| slot.as_str() == *category))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(ActionError::new(
        409,
        "inception_missing",
        format!(
            "project {} cannot start until its inception outputs are published (missing: {})",
            project.key,
            missing.join(", ")
        ),
    ))
}

/// The one build-start gate both run paths call, so they cannot diverge: the programme's declared
/// order first, then the inception outputs.
pub(crate) async fn assert_project_can_start(
    tx: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(), ActionError> {
    assert_dependencies_met(&mut *tx, tenant, project_id).await?;
    assert_inception_published(&mut *tx, tenant, project_id).await
}

/// A dependency is met when its project is closed or archived. `archived` covers a superseded
/// slice (S06a); every other status — draft, active, paused — is still open work.
fn is_met(status: &str) -> bool {
    matches!(status, "closed" | "archived")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::dependency_keys;

    #[test]
    fn dependency_keys_trims_dedupes_and_ignores_non_strings() {
        let meta = json!({"depends_on": ["S00", " S00 ", "S01", 7, "", null]});
        assert_eq!(
            dependency_keys(&meta),
            vec!["S00".to_owned(), "S01".to_owned()]
        );
    }

    #[test]
    fn dependency_keys_absent_is_empty() {
        assert!(dependency_keys(&json!({})).is_empty());
        assert!(dependency_keys(&json!({"depends_on": "S00"})).is_empty());
    }
}
