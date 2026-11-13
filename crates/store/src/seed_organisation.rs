//! `migrate --seed` organisation import: the design site's organisation fixtures become real
//! rows (SPEC_V2 §4; T0 P4).
//!
//! The document is `seed/organisation.json`, transcribed from `docs/design/v2.js`'s
//! `F.industries|departments|jobs` fixtures and SPEC_V2 §4.6. It is an **image input**
//! (`include_str!`), so a publish must never rewrite it — a seed change is a repository PR, a
//! new image and a `migrate --seed` run.
//!
//! **Idempotent, precisely.** Natural keys are `(tenant, industry.key)`,
//! `(tenant, department.name)`, `(tenant, job_catalogue.job_key)` and "a job has at least one
//! release". A second run creates no row, bumps no version and leaves `updated_at` alone.
//!
//! Every tenant named in the document receives only its own rows: `nonagentic` gets the
//! software industry and its five departments/jobs; `AHTAPol` and `w2v` get their industry
//! (their departments and jobs are not specified yet — D60 keeps them in scope but names no
//! row). No binding is seeded: a binding needs a real executor configuration (a process or an
//! agent), and inventing one would be the mock data pretending to be execution state.

use conveyor_domain::TenantId;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::actions::{NewDepartment, department_create};
use crate::organisation::{
    NewIndustry, NewJob, PlacementIn, industry_create_tx, job_create_tx, job_release_publish_tx,
};
use crate::scope::begin_scoped;
use crate::seed::SeedError;
use crate::seed_programme::targets;

/// The checked-in organisation document.
pub const ORGANISATION_EXPORT: &str = include_str!("../../../seed/organisation.json");

/// The document's top level.
#[derive(Debug, Deserialize)]
pub struct OrganisationExport {
    /// The document format; only `1` is accepted.
    pub version: u32,
    /// Every industry in the document, each naming its tenant.
    pub industries: Vec<IndustrySeed>,
    /// Every department, placed in one industry.
    pub departments: Vec<DepartmentSeed>,
    /// Every catalogue position.
    pub jobs: Vec<JobSeed>,
}

/// One industry row the seed owns.
#[derive(Debug, Deserialize)]
pub struct IndustrySeed {
    /// Stable slug.
    pub key: String,
    /// Display name.
    pub name: String,
    /// The tenant slug this row belongs to (`nonagentic`, `AHTAPol`, `w2v`).
    pub tenant: String,
    /// A one-line note, stored in `meta.note`.
    #[serde(default)]
    pub note: Option<String>,
}

/// One department row the seed owns.
#[derive(Debug, Deserialize)]
pub struct DepartmentSeed {
    /// The seed's own key (department names are unique per tenant; this is the readable id).
    pub key: String,
    /// The name written to `department.name` (unique per tenant).
    pub name: String,
    /// The industry key it belongs to.
    pub industry: String,
    /// 1-based order inside the industry.
    pub ordinal: i32,
    /// The industry's primary department.
    pub is_primary: bool,
    /// The department's first work (documentation only; the table has no column for it).
    #[serde(default)]
    #[allow(dead_code)]
    pub first: Option<String>,
}

/// One catalogue position the seed owns.
#[derive(Debug, Deserialize)]
pub struct JobSeed {
    /// Stable job key (`^[a-z][a-z0-9_]{1,63}$`).
    pub key: String,
    /// Display title.
    pub title: String,
    /// The job family.
    pub family: String,
    /// The industry key.
    pub industry: String,
    /// The department name it is placed in.
    pub department: String,
    /// `draft | active`.
    pub status: String,
    /// The draft anatomy (§4.2), compiled into the release snapshot.
    #[serde(default)]
    pub definition: Value,
}

/// What one organisation seed run did in one tenant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OrganisationCounts {
    /// Rows inserted (industries + departments + jobs).
    pub created: i64,
    /// Rows already present.
    pub skipped: i64,
    /// Releases compiled for jobs that had none.
    pub releases_published: i64,
}

/// Parse the embedded document. A malformed document is a build-time error.
pub fn organisation_export() -> Result<OrganisationExport, SeedError> {
    let export: OrganisationExport = serde_json::from_str(ORGANISATION_EXPORT)
        .map_err(|error| SeedError::OrganisationDocument(error.to_string()))?;
    if export.version != 1 {
        return Err(SeedError::OrganisationDocument(format!(
            "unsupported version {}",
            export.version
        )));
    }
    Ok(export)
}

/// Apply the organisation document to one tenant, or to every active tenant.
pub async fn seed_organisation(
    pool: &PgPool,
    tenant_slug: Option<&str>,
) -> Result<Vec<(String, OrganisationCounts)>, SeedError> {
    let export = organisation_export()?;
    let targets = targets(pool, tenant_slug).await?;
    let mut results = Vec::with_capacity(targets.len());
    for (tenant_id, slug) in targets {
        let tenant = TenantId::from_uuid(tenant_id)?;
        let industries: Vec<&IndustrySeed> = export
            .industries
            .iter()
            .filter(|row| row.tenant == slug)
            .collect();
        if industries.is_empty() {
            results.push((slug, OrganisationCounts::default()));
            continue;
        }
        let mut tx = begin_scoped(pool, tenant).await?;
        let mut counts = OrganisationCounts::default();

        let mut industry_ids: Vec<(String, Uuid)> = Vec::new();
        for industry in &industries {
            let id = match industry_id(&mut tx, tenant_id, &industry.key).await? {
                Some(id) => {
                    counts.skipped += 1;
                    id
                }
                None => {
                    let row = industry_create_tx(
                        &mut tx,
                        tenant,
                        &NewIndustry {
                            key: industry.key.clone(),
                            name: industry.name.clone(),
                            meta: json!({"note": industry.note}),
                        },
                    )
                    .await?;
                    counts.created += 1;
                    row.id
                }
            };
            industry_ids.push((industry.key.clone(), id));
        }

        let mut department_ids: Vec<(String, Uuid)> = Vec::new();
        for department in export
            .departments
            .iter()
            .filter(|row| industry_ids.iter().any(|(key, _)| key == &row.industry))
        {
            let industry_id = *industry_ids
                .iter()
                .find(|(key, _)| key == &department.industry)
                .map(|(_, id)| id)
                .ok_or(SeedError::Missing("industry", "department placement"))?;
            let id = match department_id_by_name(&mut tx, tenant_id, &department.name).await? {
                Some(id) => {
                    counts.skipped += 1;
                    id
                }
                None => {
                    let row = department_create(
                        &mut tx,
                        tenant,
                        &NewDepartment {
                            name: &department.name,
                            parent_id: None,
                            industry_id: Some(industry_id),
                            ordinal: Some(department.ordinal),
                            is_primary: department.is_primary,
                        },
                    )
                    .await?;
                    counts.created += 1;
                    row.id
                }
            };
            department_ids.push((department.name.clone(), id));
        }

        for job in export
            .jobs
            .iter()
            .filter(|row| industry_ids.iter().any(|(key, _)| key == &row.industry))
        {
            let industry_id = *industry_ids
                .iter()
                .find(|(key, _)| key == &job.industry)
                .map(|(_, id)| id)
                .ok_or(SeedError::Missing("industry", "job"))?;
            let department_id = *department_ids
                .iter()
                .find(|(name, _)| name == &job.department)
                .map(|(_, id)| id)
                .ok_or(SeedError::Missing("department", "job placement"))?;
            let job_id = match job_id_by_key(&mut tx, tenant_id, &job.key).await? {
                Some(id) => {
                    counts.skipped += 1;
                    id
                }
                None => {
                    let created = job_create_tx(
                        &mut tx,
                        tenant,
                        &NewJob {
                            job_key: job.key.clone(),
                            industry_id,
                            family: job.family.clone(),
                            title: job.title.clone(),
                            status: job.status.clone(),
                            definition: job.definition.clone(),
                            meta: json!({"source": "docs/design/v2.js + SPEC_V2 §4.6"}),
                            departments: vec![PlacementIn {
                                department_id,
                                ordinal: 1,
                                is_primary: true,
                            }],
                        },
                    )
                    .await?;
                    counts.created += 1;
                    created.id
                }
            };
            // Only a described job compiles a release: a draft catalogue row carries no anatomy,
            // so a release of `{}` would be a meaningless immutable artefact.
            let described = job
                .definition
                .as_object()
                .is_some_and(|definition| !definition.is_empty());
            if !described || release_exists(&mut tx, tenant_id, job_id).await? {
                continue;
            }
            job_release_publish_tx(&mut tx, tenant, job_id).await?;
            counts.releases_published += 1;
        }

        tx.commit().await?;
        results.push((slug, counts));
    }
    Ok(results)
}

async fn industry_id(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    key: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM industry WHERE tenant_id = $1 AND key = $2")
        .bind(tenant_id)
        .bind(key)
        .fetch_optional(&mut *conn)
        .await
}

async fn department_id_by_name(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    name: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM department WHERE tenant_id = $1 AND name = $2")
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(&mut *conn)
        .await
}

async fn job_id_by_key(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    key: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM job_catalogue WHERE tenant_id = $1 AND job_key = $2")
        .bind(tenant_id)
        .bind(key)
        .fetch_optional(&mut *conn)
        .await
}

async fn release_exists(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    job_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM job_release WHERE tenant_id = $1 AND job_id = $2)",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_one(&mut *conn)
    .await
}
