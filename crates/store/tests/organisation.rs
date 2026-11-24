//! T0 — the organisation spine (SPEC_V2 §4, §10.3; issue #34).
//!
//! `cargo test -p conveyor-store --test organisation -- --nocapture`.
//!
//! Proves, at the store boundary:
//! - P1.1/P6.1 every new table is FORCE RLS;
//! - P1.3 the ordered placement and its refusals;
//! - P2.1 the release hash is deterministic and publishing is idempotent by content;
//! - P2.2/P2.3 a run pins its binding and a retired or mismatched pin is refused with both hashes;
//! - P4.1/P4.2 the seed is idempotent and the seeded anatomy compiles;
//! - P6.2 a test-only third executor kind registers and resolves a run without touching the run
//!   path.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::executor::{BindingConfig, BindingError, Executor, ExecutorKind, Registry};
use conveyor_domain::organisation::{BindingStatus, release_hash};
use conveyor_store::actions::{NewDepartment, department_create};
use conveyor_store::begin_scoped;
use conveyor_store::organisation::{
    NewBinding, NewIndustry, NewJob, OrganisationStoreError, PlacementIn, compile_release_tx,
    industry_create_tx, job_binding_create_tx, job_binding_retire_tx, job_create_tx,
    job_placements_tx, job_release_publish_tx,
};
use conveyor_store::run::{NewRun, RunStoreError, insert_run_tx};
use conveyor_store::seed_organisation::seed_organisation;
use serde_json::json;
use uuid::Uuid;

/// One organisation fixture: an industry, two departments, a described job and an agent.
struct Org {
    tenant: TenantId,
    industry: Uuid,
    delivery: Uuid,
    verification: Uuid,
    job: Uuid,
    agent: Uuid,
    project: Uuid,
}

async fn org_fixture(db: &support::TestDb, slug: &str) -> Org {
    let seeded = support::seed_tenant(&db.super_pool, slug).await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("non-nil tenant");
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let industry = industry_create_tx(
        &mut tx,
        tenant,
        &NewIndustry {
            key: "software".to_owned(),
            name: "Software".to_owned(),
            meta: json!({}),
        },
    )
    .await
    .expect("industry");
    let delivery = department_create(
        &mut tx,
        tenant,
        &NewDepartment {
            name: "delivery",
            parent_id: None,
            industry_id: Some(industry.id),
            ordinal: Some(1),
            is_primary: true,
        },
    )
    .await
    .expect("delivery");
    let verification = department_create(
        &mut tx,
        tenant,
        &NewDepartment {
            name: "verification",
            parent_id: None,
            industry_id: Some(industry.id),
            ordinal: Some(2),
            is_primary: false,
        },
    )
    .await
    .expect("verification");
    let job = job_create_tx(
        &mut tx,
        tenant,
        &NewJob {
            job_key: "backend_engineer".to_owned(),
            industry_id: industry.id,
            family: "engineering".to_owned(),
            title: "Backend engineer".to_owned(),
            status: "active".to_owned(),
            definition: json!({
                "charter": "Implement bounded backend changes; prove them with tests.",
                "capability": "delivery",
                "limits": {"max_iterations": 12},
            }),
            meta: json!({}),
            departments: vec![
                PlacementIn {
                    department_id: delivery.id,
                    ordinal: 1,
                    is_primary: true,
                },
                PlacementIn {
                    department_id: verification.id,
                    ordinal: 2,
                    is_primary: false,
                },
            ],
        },
    )
    .await
    .expect("job");
    tx.commit().await.expect("commit fixture");

    // The capability + agent the internal_agent executor validates against. Raw SQL as the
    // superuser is the positive control (RI bypasses RLS); the behaviour under test then runs
    // as the runtime role.
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label) VALUES ($1, 'delivery', 'Delivery')
         RETURNING id",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("capability");
    let agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent (tenant_id, capability_id, key, model_ref, role)
         VALUES ($1, $2, 'implementation', 'router/medium', 'tool_agent') RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(capability)
    .fetch_one(&db.super_pool)
    .await
    .expect("agent");

    Org {
        tenant,
        industry: industry.id,
        delivery: delivery.id,
        verification: verification.id,
        job: job.id,
        agent,
        project: seeded.project,
    }
}

fn placement(department_id: Uuid, ordinal: i32, is_primary: bool) -> PlacementIn {
    PlacementIn {
        department_id,
        ordinal,
        is_primary,
    }
}

#[tokio::test]
async fn organisation_tables_are_forced_rls() {
    let db = support::TestDb::fresh().await;
    for table in [
        "industry",
        "job_catalogue",
        "job_department",
        "job_release",
        "job_binding",
        "decision",
        "decision_option",
        "decision_policy",
        "decision_policy_refinement",
        "laya_inference_config",
    ] {
        let (enabled, forced): (bool, bool) = sqlx::query_as(
            "SELECT relrowsecurity, relforcerowsecurity
               FROM pg_class
              WHERE relname = $1 AND relnamespace = 'public'::regnamespace",
        )
        .bind(table)
        .fetch_one(&db.super_pool)
        .await
        .unwrap_or_else(|error| panic!("{table}: {error}"));
        assert!(enabled, "{table} must enable RLS");
        assert!(forced, "{table} must FORCE RLS");
    }
}

#[tokio::test]
async fn job_departments_are_ordered_and_placement_is_validated() {
    let db = support::TestDb::fresh().await;
    let org = org_fixture(&db, "placement").await;
    let mut tx = begin_scoped(&db.runtime, org.tenant).await.expect("scope");

    let placements = job_placements_tx(&mut tx, org.tenant, org.job)
        .await
        .expect("placements");
    assert_eq!(placements.len(), 2);
    assert_eq!(placements[0].department_name, "delivery");
    assert_eq!(placements[0].ordinal, 1);
    assert!(placements[0].is_primary);
    assert_eq!(placements[1].ordinal, 2);
    assert!(!placements[1].is_primary);

    let base = |key: &str, departments: Vec<PlacementIn>| NewJob {
        job_key: key.to_owned(),
        industry_id: org.industry,
        family: "engineering".to_owned(),
        title: format!("Job {key}"),
        status: "active".to_owned(),
        definition: json!({"charter": "x"}),
        meta: json!({}),
        departments,
    };

    let gap = job_create_tx(
        &mut tx,
        org.tenant,
        &base(
            "gap",
            vec![
                placement(org.delivery, 1, true),
                placement(org.verification, 3, false),
            ],
        ),
    )
    .await;
    assert!(
        matches!(gap, Err(OrganisationStoreError::InvalidPlacement)),
        "a gap between ordinals is refused: {gap:?}"
    );

    let two_primaries = job_create_tx(
        &mut tx,
        org.tenant,
        &base(
            "two",
            vec![
                placement(org.delivery, 1, true),
                placement(org.verification, 2, true),
            ],
        ),
    )
    .await;
    assert!(
        matches!(two_primaries, Err(OrganisationStoreError::InvalidPlacement)),
        "exactly one primary is required: {two_primaries:?}"
    );

    // A department of another industry is refused even though the FK would accept it.
    let other = industry_create_tx(
        &mut tx,
        org.tenant,
        &NewIndustry {
            key: "media".to_owned(),
            name: "Media".to_owned(),
            meta: json!({}),
        },
    )
    .await
    .expect("second industry");
    let newsroom = department_create(
        &mut tx,
        org.tenant,
        &NewDepartment {
            name: "newsroom",
            parent_id: None,
            industry_id: Some(other.id),
            ordinal: Some(1),
            is_primary: true,
        },
    )
    .await
    .expect("newsroom");
    let wrong_industry = job_create_tx(
        &mut tx,
        org.tenant,
        &base("wrong", vec![placement(newsroom.id, 1, true)]),
    )
    .await;
    assert!(
        matches!(
            wrong_industry,
            Err(OrganisationStoreError::DepartmentNotInIndustry)
        ),
        "a cross-industry placement is refused: {wrong_industry:?}"
    );

    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn release_hash_is_deterministic_and_publish_is_idempotent() {
    let db = support::TestDb::fresh().await;
    let org = org_fixture(&db, "release").await;
    let mut tx = begin_scoped(&db.runtime, org.tenant).await.expect("scope");

    let first = compile_release_tx(&mut tx, org.tenant, org.job)
        .await
        .expect("compile once");
    let second = compile_release_tx(&mut tx, org.tenant, org.job)
        .await
        .expect("compile twice");
    assert_eq!(first, second, "the compiled snapshot is byte-identical");
    assert_eq!(release_hash(&first), release_hash(&second));
    assert_eq!(
        first
            .get("anatomy")
            .and_then(|anatomy| anatomy.get("charter"))
            .and_then(|charter| charter.as_str()),
        Some("Implement bounded backend changes; prove them with tests."),
        "the anatomy is compiled verbatim"
    );
    assert_eq!(
        first
            .get("departments")
            .and_then(|rows| rows.as_array())
            .map(Vec::len),
        Some(2),
        "the placement is part of the snapshot"
    );

    let published = job_release_publish_tx(&mut tx, org.tenant, org.job)
        .await
        .expect("publish");
    let again = job_release_publish_tx(&mut tx, org.tenant, org.job)
        .await
        .expect("publish again");
    assert_eq!(published.id, again.id, "unchanged content is one release");
    assert_eq!(published.release_hash, release_hash(&first));
    let releases: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job_release WHERE tenant_id = $1 AND job_id = $2")
            .bind(org.tenant.as_uuid())
            .bind(org.job)
            .fetch_one(&mut *tx)
            .await
            .expect("count releases");
    assert_eq!(releases, 1);

    // Rotation is allowed and touches only the bookkeeping columns; the runtime role has no
    // UPDATE on `release_hash`/`release_snapshot` (scripts/grants-runtime.sql), so this also
    // proves the column grant the migration promises.
    let retired =
        conveyor_store::organisation::job_release_retire_tx(&mut tx, org.tenant, published.id)
            .await
            .expect("retire the release");
    assert_eq!(retired.status, "retired");
    assert_eq!(retired.release_hash, published.release_hash);
    assert_eq!(retired.release_snapshot, published.release_snapshot);

    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_publish_refuses_an_untyped_or_unknown_grant() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "grants").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("non-nil tenant");
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let industry = industry_create_tx(
        &mut tx,
        tenant,
        &NewIndustry {
            key: "software".to_owned(),
            name: "Software".to_owned(),
            meta: json!({}),
        },
    )
    .await
    .expect("industry");
    let delivery = department_create(
        &mut tx,
        tenant,
        &NewDepartment {
            name: "delivery",
            parent_id: None,
            industry_id: Some(industry.id),
            ordinal: Some(1),
            is_primary: true,
        },
    )
    .await
    .expect("delivery");
    let job = job_create_tx(
        &mut tx,
        tenant,
        &NewJob {
            job_key: "granted".to_owned(),
            industry_id: industry.id,
            family: "engineering".to_owned(),
            title: "Granted".to_owned(),
            status: "active".to_owned(),
            definition: json!({
                "charter": "Prove the grant is typed.",
                "grants": [
                    "coding.jobs.get",
                    {"op": "coding.jobs.get", "max_side_effect": "admin"}
                ],
            }),
            meta: json!({}),
            departments: vec![PlacementIn {
                department_id: delivery.id,
                ordinal: 1,
                is_primary: true,
            }],
        },
    )
    .await
    .expect("job");

    let error = job_release_publish_tx(&mut tx, tenant, job.id)
        .await
        .expect_err("a bare string and an unknown effect are refused");
    let OrganisationStoreError::ReleaseAnatomyInvalid(fields) = error else {
        panic!("expected ReleaseAnatomyInvalid, got {error:?}");
    };
    assert!(
        fields.iter().any(|field| field.field == "grants[0]"),
        "the bare string is named: {fields:?}"
    );
    assert!(
        fields
            .iter()
            .any(|field| field.field == "grants[1].max_side_effect"),
        "the unknown effect is named: {fields:?}"
    );
    let releases: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job_release WHERE tenant_id = $1 AND job_id = $2")
            .bind(tenant.as_uuid())
            .bind(job.id)
            .fetch_one(&mut *tx)
            .await
            .expect("count releases");
    assert_eq!(releases, 0, "nothing was hashed or inserted");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn a_binding_must_agree_with_the_release_executor() {
    let db = support::TestDb::fresh().await;
    let org = org_fixture(&db, "kind-agreement").await;
    let mut tx = begin_scoped(&db.runtime, org.tenant).await.expect("scope");
    let job = job_create_tx(
        &mut tx,
        org.tenant,
        &NewJob {
            job_key: "procedure_job".to_owned(),
            industry_id: org.industry,
            family: "engineering".to_owned(),
            title: "Procedure job".to_owned(),
            status: "active".to_owned(),
            definition: json!({"executor_kind": "procedure"}),
            meta: json!({}),
            departments: vec![PlacementIn {
                department_id: org.delivery,
                ordinal: 1,
                is_primary: true,
            }],
        },
    )
    .await
    .expect("job");
    let release = job_release_publish_tx(&mut tx, org.tenant, job.id)
        .await
        .expect("publish");
    let error = job_binding_create_tx(
        &mut tx,
        org.tenant,
        &NewBinding {
            job_id: job.id,
            release_id: release.id,
            executor_kind: ExecutorKind::internal_agent(),
            process_id: None,
            capability_agent_id: Some(org.agent),
            capability_id: None,
            meta: json!({}),
            status: BindingStatus::Active,
        },
        &Registry::builtin(),
    )
    .await
    .expect_err("the release declares a procedure executor");
    match error {
        OrganisationStoreError::BindingKindMismatch { binding, release } => {
            assert_eq!(binding, "internal_agent");
            assert_eq!(release, "procedure");
        }
        other => panic!("expected BindingKindMismatch, got {other:?}"),
    }
    tx.commit().await.expect("commit");
}

async fn active_binding(
    db: &support::TestDb,
    org: &Org,
    registry: &Registry,
) -> conveyor_store::organisation::BindingRow {
    let mut tx = begin_scoped(&db.runtime, org.tenant).await.expect("scope");
    let release = job_release_publish_tx(&mut tx, org.tenant, org.job)
        .await
        .expect("release");
    let binding = job_binding_create_tx(
        &mut tx,
        org.tenant,
        &NewBinding {
            job_id: org.job,
            release_id: release.id,
            executor_kind: ExecutorKind::internal_agent(),
            process_id: None,
            capability_agent_id: Some(org.agent),
            capability_id: None,
            meta: json!({"department_scope": ["delivery"]}),
            status: BindingStatus::Active,
        },
        registry,
    )
    .await
    .expect("binding");
    tx.commit().await.expect("commit");
    binding
}

#[tokio::test]
async fn a_run_pins_its_binding_and_a_retired_or_mismatched_pin_is_refused() {
    let db = support::TestDb::fresh().await;
    let org = org_fixture(&db, "binding").await;
    let registry = Registry::builtin();
    let binding = active_binding(&db, &org, &registry).await;

    // P2.3 — an active binding with its pin records both columns.
    let mut tx = begin_scoped(&db.runtime, org.tenant).await.expect("scope");
    let mut run_in = NewRun::new(org.project, None);
    run_in.job_binding_id = Some(binding.id);
    run_in.release_hash = Some(binding.release_hash.clone());
    let run = insert_run_tx(&mut tx, org.tenant, &run_in)
        .await
        .expect("run on the active binding");
    let (stored_binding, stored_hash): (Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT job_binding_id, release_hash FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(org.tenant.as_uuid())
    .bind(run.id)
    .fetch_one(&mut *tx)
    .await
    .expect("read the run pin");
    assert_eq!(stored_binding, Some(binding.id));
    assert_eq!(stored_hash.as_deref(), Some(binding.release_hash.as_str()));

    // A requested release that is not the binding's pin names both hashes and writes no run.
    let mut wrong = NewRun::new(org.project, None);
    wrong.job_binding_id = Some(binding.id);
    wrong.release_hash = Some("0".repeat(64));
    let refused = insert_run_tx(&mut tx, org.tenant, &wrong).await;
    match refused {
        Err(RunStoreError::JobBindingStale {
            binding_hash,
            requested_hash,
        }) => {
            assert_eq!(binding_hash, binding.release_hash);
            assert_eq!(requested_hash, "0".repeat(64));
        }
        other => panic!("a mismatched pin must be refused, got {other:?}"),
    }
    let runs_on_binding: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1 AND job_binding_id = $2")
            .bind(org.tenant.as_uuid())
            .bind(binding.id)
            .fetch_one(&mut *tx)
            .await
            .expect("count runs");
    assert_eq!(runs_on_binding, 1, "the refusal wrote no run");
    tx.commit().await.expect("commit");

    // P2.2 — retiring the binding refuses the next run, naming the pin and the request.
    let mut tx = begin_scoped(&db.runtime, org.tenant).await.expect("scope");
    job_binding_retire_tx(&mut tx, org.tenant, binding.id, binding.version)
        .await
        .expect("retire");
    let mut retired_run = NewRun::new(org.project, None);
    retired_run.job_binding_id = Some(binding.id);
    let refused = insert_run_tx(&mut tx, org.tenant, &retired_run).await;
    match refused {
        Err(RunStoreError::JobBindingStale {
            binding_hash,
            requested_hash,
        }) => {
            assert_eq!(binding_hash, binding.release_hash);
            assert_eq!(requested_hash, binding.release_hash);
        }
        other => panic!("a retired binding must be refused, got {other:?}"),
    }
    tx.commit().await.expect("commit");
}

struct ThirdExecutor;

impl Executor for ThirdExecutor {
    fn kind(&self) -> ExecutorKind {
        ExecutorKind::parse("third").expect("valid slug")
    }

    fn validate_binding(&self, _binding: &BindingConfig) -> Result<(), BindingError> {
        Ok(())
    }
}

#[tokio::test]
async fn third_executor_kind_registers_without_editing_the_run_path() {
    let db = support::TestDb::fresh().await;
    let org = org_fixture(&db, "third-kind").await;
    let mut registry = Registry::builtin();
    registry
        .register(Box::new(ThirdExecutor))
        .expect("a new kind registers");

    let mut tx = begin_scoped(&db.runtime, org.tenant).await.expect("scope");
    let release = job_release_publish_tx(&mut tx, org.tenant, org.job)
        .await
        .expect("release");
    let binding = job_binding_create_tx(
        &mut tx,
        org.tenant,
        &NewBinding {
            job_id: org.job,
            release_id: release.id,
            executor_kind: ExecutorKind::parse("third").expect("valid"),
            process_id: None,
            capability_agent_id: Some(org.agent),
            capability_id: None,
            meta: json!({}),
            status: BindingStatus::Active,
        },
        &registry,
    )
    .await
    .expect("binding with the third kind");
    let mut run_in = NewRun::new(org.project, None);
    run_in.job_binding_id = Some(binding.id);
    let run = insert_run_tx(&mut tx, org.tenant, &run_in)
        .await
        .expect("a run resolves the third kind without a run-path edit");
    assert!(run.id != Uuid::nil());

    // The builtin registry refuses the same kind at binding creation: the registry, not the
    // run path, is the authority.
    let refused = job_binding_create_tx(
        &mut tx,
        org.tenant,
        &NewBinding {
            job_id: org.job,
            release_id: release.id,
            executor_kind: ExecutorKind::parse("third").expect("valid"),
            process_id: None,
            capability_agent_id: Some(org.agent),
            capability_id: None,
            meta: json!({}),
            status: BindingStatus::Active,
        },
        &Registry::builtin(),
    )
    .await;
    assert!(
        matches!(
            &refused,
            Err(OrganisationStoreError::Executor(BindingError::UnknownKind(kind)))
                if kind.as_str() == "third"
        ),
        "the builtin registry refuses the third kind: {refused:?}"
    );
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn seed_organisation_is_idempotent_and_the_seeded_anatomy_compiles() {
    let db = support::TestDb::fresh().await;
    let _ = support::seed_tenant(&db.super_pool, "nonagentic").await;

    let first = seed_organisation(&db.super_pool, Some("nonagentic"))
        .await
        .expect("first seed");
    let counts = first
        .first()
        .map(|(_, counts)| *counts)
        .expect("one tenant");
    assert_eq!(counts.created, 28, "1 industry + 5 departments + 22 jobs");
    assert_eq!(counts.releases_published, 5, "the five described jobs");

    let second = seed_organisation(&db.super_pool, Some("nonagentic"))
        .await
        .expect("second seed");
    let counts = second
        .first()
        .map(|(_, counts)| *counts)
        .expect("one tenant");
    assert_eq!(counts.created, 0, "a second run creates nothing");
    assert_eq!(counts.skipped, 28);
    assert_eq!(counts.releases_published, 0);

    let anatomy: serde_json::Value = sqlx::query_scalar(
        "SELECT jr.release_snapshot -> 'anatomy'
           FROM job_release jr
           JOIN job_catalogue j ON j.id = jr.job_id AND j.tenant_id = jr.tenant_id
          WHERE j.tenant_id = $1 AND j.job_key = 'backend_engineer'",
    )
    .bind(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM tenant WHERE slug = 'nonagentic'")
            .fetch_one(&db.super_pool)
            .await
            .expect("tenant id"),
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seeded release");
    assert!(
        anatomy
            .get("charter")
            .and_then(|charter| charter.as_str())
            .is_some_and(|charter| charter.contains("Implement bounded backend changes")),
        "the design's anatomy is in the release: {anatomy}"
    );
    assert_eq!(
        anatomy
            .get("capabilities")
            .and_then(|value| value.as_array())
            .and_then(|values| values.first())
            .and_then(|value| value.as_str()),
        Some("delivery")
    );
    let grants = anatomy
        .get("grants")
        .and_then(|value| value.as_array())
        .expect("the seeded job carries a typed grant list");
    assert!(!grants.is_empty(), "backend_engineer is a tool-backed job");
    for grant in grants {
        assert!(
            grant
                .get("op")
                .and_then(|op| op.as_str())
                .is_some_and(|op| op.contains('.')),
            "every seeded grant names <toolkit>.<operation>: {grant}"
        );
        assert!(
            grant.get("max_side_effect").is_some(),
            "every seeded grant names its ceiling: {grant}"
        );
    }
    assert_eq!(
        anatomy
            .get("prompt_recipe")
            .and_then(|value| value.as_str()),
        Some("agent@1"),
        "the seeded jobs pin a recipe every tenant has"
    );
    assert!(
        anatomy.get("limits").is_some_and(|value| value.is_object()),
        "the seeded limits are the typed object, not a display string"
    );

    let draft_releases: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM job_release jr
           JOIN job_catalogue j ON j.id = jr.job_id AND j.tenant_id = jr.tenant_id
          WHERE j.tenant_id = $1 AND j.job_key = 'product_owner'",
    )
    .bind(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM tenant WHERE slug = 'nonagentic'")
            .fetch_one(&db.super_pool)
            .await
            .expect("tenant id"),
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("count draft releases");
    assert_eq!(draft_releases, 0, "a draft row compiles no release");
}

#[tokio::test]
async fn ahtapol_receives_only_its_industry() {
    let db = support::TestDb::fresh().await;
    let _ = support::seed_tenant(&db.super_pool, "AHTAPol").await;
    let counts = seed_organisation(&db.super_pool, Some("AHTAPol"))
        .await
        .expect("seed AHTAPol");
    let counts = counts
        .first()
        .map(|(_, counts)| *counts)
        .expect("one tenant");
    assert_eq!(counts.created, 1, "its own industry only");
    let industries: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM industry WHERE key = 'health-technology-assessment'",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("count industries");
    assert_eq!(industries, 1);
}

// ── S21 step 11 (issue #697): the release compiles the knowledge layers (SPEC_V2 §4.1) ─────

#[allow(clippy::too_many_arguments)]
async fn set_layer(
    db: &support::TestDb,
    tenant: TenantId,
    level: &str,
    industry: Option<Uuid>,
    department: Option<Uuid>,
    job: Option<Uuid>,
    kind: &str,
    value: Option<serde_json::Value>,
    required: bool,
) {
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    conveyor_store::organisation::set_layer_tx(
        &mut tx,
        tenant,
        &conveyor_store::organisation::LayerSet {
            level: level.to_owned(),
            industry_id: industry,
            department_id: department,
            job_id: job,
            kind: kind.to_owned(),
            value,
            required,
        },
    )
    .await
    .expect("author the layer");
    tx.commit().await.expect("commit the layer");
}

async fn publish(
    db: &support::TestDb,
    tenant: TenantId,
    job: Uuid,
) -> Result<conveyor_store::organisation::ReleaseRow, OrganisationStoreError> {
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    match job_release_publish_tx(&mut tx, tenant, job).await {
        Ok(row) => {
            tx.commit().await.expect("commit the publish");
            Ok(row)
        }
        Err(error) => {
            tx.rollback().await.ok();
            Err(error)
        }
    }
}

/// A parented department pair: an archetype and one child the job is placed in.
async fn archetype_fixture(db: &support::TestDb, slug: &str) -> (TenantId, Uuid, Uuid, Uuid, Uuid) {
    let seeded = support::seed_tenant(&db.super_pool, slug).await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("non-nil tenant");
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let industry = industry_create_tx(
        &mut tx,
        tenant,
        &NewIndustry {
            key: format!("ind-{slug}"),
            name: "Layer industry".to_owned(),
            meta: json!({}),
        },
    )
    .await
    .expect("industry");
    let archetype = department_create(
        &mut tx,
        tenant,
        &NewDepartment {
            name: "engineering-archetype",
            parent_id: None,
            industry_id: Some(industry.id),
            ordinal: Some(1),
            is_primary: false,
        },
    )
    .await
    .expect("archetype");
    let child = department_create(
        &mut tx,
        tenant,
        &NewDepartment {
            name: "delivery",
            parent_id: Some(archetype.id),
            industry_id: Some(industry.id),
            ordinal: Some(2),
            is_primary: true,
        },
    )
    .await
    .expect("child");
    let job = job_create_tx(
        &mut tx,
        tenant,
        &NewJob {
            job_key: format!("job_{}", slug.replace('-', "_")),
            industry_id: industry.id,
            family: "engineering".to_owned(),
            title: "Layer job".to_owned(),
            status: "active".to_owned(),
            definition: json!({"charter": "prove the layers"}),
            meta: json!({}),
            departments: vec![PlacementIn {
                department_id: child.id,
                ordinal: 1,
                is_primary: true,
            }],
        },
    )
    .await
    .expect("job");
    tx.commit().await.expect("commit fixture");
    (tenant, industry.id, archetype.id, child.id, job.id)
}

#[tokio::test]
async fn the_release_resolves_layers_in_declared_order() {
    let db = support::TestDb::fresh().await;
    let (tenant, industry, archetype, child, job) = archetype_fixture(&db, "layer-order").await;
    for (level, scope, value) in [
        ("base", None, "base-style"),
        ("industry", Some(industry), "industry-style"),
        ("department_archetype", Some(archetype), "archetype-style"),
        ("department", Some(child), "department-style"),
    ] {
        set_layer(
            &db,
            tenant,
            level,
            if level == "industry" { scope } else { None },
            if level.starts_with("department") {
                scope
            } else {
                None
            },
            None,
            "style",
            Some(json!(value)),
            false,
        )
        .await;
    }
    // A second kind set only at the industry layer keeps that layer's source.
    set_layer(
        &db,
        tenant,
        "industry",
        Some(industry),
        None,
        None,
        "tone",
        Some(json!("plain")),
        false,
    )
    .await;
    set_layer(
        &db,
        tenant,
        "job",
        None,
        None,
        Some(job),
        "style",
        Some(json!("job-style")),
        false,
    )
    .await;

    let release = publish(&db, tenant, job).await.expect("publish");
    let layers = &release.release_snapshot["layers"];
    assert_eq!(layers["style"]["value"], json!("job-style"));
    assert_eq!(layers["style"]["level"], json!("job"));
    assert_eq!(layers["style"]["source"], json!("job_layer_order"));
    assert_eq!(
        layers["tone"]["level"],
        json!("industry"),
        "a kind the job does not set keeps its industry source: {layers}"
    );
    assert_eq!(layers["tone"]["source"], json!("ind-layer-order"));
}

#[tokio::test]
async fn required_only_tightens_downward() {
    let db = support::TestDb::fresh().await;
    let (tenant, industry, _archetype, child, job) = archetype_fixture(&db, "layer-required").await;
    set_layer(
        &db,
        tenant,
        "industry",
        Some(industry),
        None,
        None,
        "policy",
        Some(json!("industry-policy")),
        true,
    )
    .await;
    // A lower layer cannot loosen it: value overridden, required stays true.
    set_layer(
        &db,
        tenant,
        "department",
        None,
        Some(child),
        None,
        "policy",
        Some(json!("department-policy")),
        false,
    )
    .await;
    set_layer(
        &db,
        tenant,
        "job",
        None,
        None,
        Some(job),
        "extra",
        Some(json!("job-extra")),
        true,
    )
    .await;

    let release = publish(&db, tenant, job).await.expect("publish");
    let layers = &release.release_snapshot["layers"];
    assert_eq!(layers["policy"]["value"], json!("department-policy"));
    assert_eq!(
        layers["policy"]["required"],
        json!(true),
        "required is the OR over the path"
    );
    assert_eq!(layers["extra"]["required"], json!(true));
}

#[tokio::test]
async fn an_unresolved_required_kind_refuses_publish() {
    let db = support::TestDb::fresh().await;
    let (tenant, _industry, _archetype, child, job) =
        archetype_fixture(&db, "layer-not-ready").await;
    // A declaration with no value anywhere on the path.
    set_layer(
        &db,
        tenant,
        "department",
        None,
        Some(child),
        None,
        "compliance",
        None,
        true,
    )
    .await;
    match publish(&db, tenant, job).await {
        Err(OrganisationStoreError::ReleaseNotReady { kind, department }) => {
            assert_eq!(kind, "compliance");
            assert_eq!(department, "delivery");
        }
        other => panic!("expected ReleaseNotReady, got {other:?}"),
    }
    let releases: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job_release WHERE tenant_id = $1 AND job_id = $2")
            .bind(tenant.as_uuid())
            .bind(job)
            .fetch_one(&db.super_pool)
            .await
            .expect("count releases");
    assert_eq!(releases, 0, "a refusal writes no release row");
}

#[tokio::test]
async fn sibling_departments_conflict_unless_the_job_resolves() {
    let db = support::TestDb::fresh().await;
    let org = org_fixture(&db, "layer-conflict").await;
    set_layer(
        &db,
        org.tenant,
        "department",
        None,
        Some(org.delivery),
        None,
        "region",
        Some(json!("eu")),
        false,
    )
    .await;
    set_layer(
        &db,
        org.tenant,
        "department",
        None,
        Some(org.verification),
        None,
        "region",
        Some(json!("us")),
        false,
    )
    .await;
    match publish(&db, org.tenant, org.job).await {
        Err(OrganisationStoreError::DepartmentConflict { kind, departments }) => {
            assert_eq!(kind, "region");
            assert_eq!(
                departments,
                vec!["delivery".to_owned(), "verification".to_owned()]
            );
        }
        other => panic!("expected DepartmentConflict, got {other:?}"),
    }

    // The job-layer row resolves it.
    set_layer(
        &db,
        org.tenant,
        "job",
        None,
        None,
        Some(org.job),
        "region",
        Some(json!("global")),
        false,
    )
    .await;
    let release = publish(&db, org.tenant, org.job).await.expect("publish");
    assert_eq!(
        release.release_snapshot["layers"]["region"]["value"],
        json!("global")
    );
    assert_eq!(
        release.release_snapshot["layers"]["region"]["level"],
        json!("job")
    );
}

#[tokio::test]
async fn two_identical_publishes_hash_identically() {
    let db = support::TestDb::fresh().await;
    let (tenant, industry, _archetype, child, job) = archetype_fixture(&db, "layer-hash").await;
    set_layer(
        &db,
        tenant,
        "industry",
        Some(industry),
        None,
        None,
        "style",
        Some(json!("industry-style")),
        true,
    )
    .await;
    set_layer(
        &db,
        tenant,
        "department",
        None,
        Some(child),
        None,
        "style",
        Some(json!("department-style")),
        false,
    )
    .await;
    let first = publish(&db, tenant, job).await.expect("first publish");
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let second = compile_release_tx(&mut tx, tenant, job)
        .await
        .expect("compile");
    tx.commit().await.expect("commit");
    assert_eq!(first.release_hash, release_hash(&second));
    assert_eq!(
        first.release_snapshot["layers"], second["layers"],
        "the layer section is stable"
    );
}
