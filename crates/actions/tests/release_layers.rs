//! v1-w1a-2 — a release compiles the knowledge layers and refuses an unresolved draft.
//!
//! SPEC_V2 §4.1: the compile is **base → industry → department archetype → each department in
//! declared order → job**; `required` can only be tightened downward; an unresolvable required
//! kind is `409 release_not_ready` naming the kind and department; two sibling departments at
//! different values are `422 department_conflict`, resolved only by a job-layer value; and a
//! successful release records every effective value with its source and is byte-for-byte
//! reproducible.
//!
//! `cargo test -p conveyor-actions --test release_layers`.

mod support;

use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::organisation::release_hash;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

fn request(action: &str, input: Value, actor_id: Uuid) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: Authority::Internal,
        request_id: Some(format!("rid-release-layers-{action}")),
        trace_id: Some("trace-release-layers".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// Invoke one action and return its output document.
async fn invoke_ok(
    pg: &support::Pg,
    seeded: &support::Seeded,
    action: &str,
    input: Value,
) -> Value {
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(action, input, seeded.user_id),
    )
    .await
    .unwrap_or_else(|error| panic!("{action}: {error:?}"));
    assert!(
        outcome.status == 200 || outcome.status == 201,
        "{action}: {}",
        outcome.body
    );
    outcome.body["output"].clone()
}

/// The id of an output document.
fn id_of(output: &Value) -> Uuid {
    output["id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("no id in {output}"))
}

/// An industry with `delivery` (primary) and `verification`, and one job placed in both.
struct Org {
    industry: Uuid,
    delivery: Uuid,
    verification: Uuid,
    job: Uuid,
}

async fn create_org(
    pg: &support::Pg,
    seeded: &support::Seeded,
    job_key: &str,
    definition: Value,
) -> Org {
    let industry = invoke_ok(
        pg,
        seeded,
        "industries.create",
        json!({"key": "software", "name": "Software"}),
    )
    .await;
    let industry_id = id_of(&industry);
    let delivery = invoke_ok(
        pg,
        seeded,
        "departments.create",
        json!({
            "name": "delivery",
            "industry_id": industry_id,
            "ordinal": 1,
            "is_primary": true,
        }),
    )
    .await;
    let delivery_id = id_of(&delivery);
    let verification = invoke_ok(
        pg,
        seeded,
        "departments.create",
        json!({
            "name": "verification",
            "industry_id": industry_id,
            "ordinal": 2,
            "is_primary": false,
        }),
    )
    .await;
    let verification_id = id_of(&verification);
    let job = invoke_ok(
        pg,
        seeded,
        "job_catalogue.create",
        json!({
            "job_key": job_key,
            "industry_id": industry_id,
            "family": "engineering",
            "title": "Backend engineer",
            "definition": definition,
            "departments": [
                {"department_id": delivery_id, "ordinal": 1, "is_primary": true},
                {"department_id": verification_id, "ordinal": 2, "is_primary": false},
            ],
        }),
    )
    .await;
    Org {
        industry: industry_id,
        delivery: delivery_id,
        verification: verification_id,
        job: id_of(&job),
    }
}

/// Insert one layer declaration. The tests write rows directly: the action surface for authoring
/// layer rows is not this finding's scope, and the compile is what must be proven here.
#[allow(clippy::too_many_arguments)]
async fn add_setting(
    pg: &support::Pg,
    tenant: Uuid,
    layer: &str,
    industry_id: Option<Uuid>,
    department_id: Option<Uuid>,
    job_id: Option<Uuid>,
    archetype: Option<&str>,
    kind: &str,
    value: Option<Value>,
    required: bool,
) {
    sqlx::query(
        "INSERT INTO organisation_layer_setting
             (tenant_id, layer, industry_id, department_id, job_id, archetype, kind, value,
              required)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(tenant)
    .bind(layer)
    .bind(industry_id)
    .bind(department_id)
    .bind(job_id)
    .bind(archetype)
    .bind(kind)
    .bind(value)
    .bind(required)
    .execute(&pg.super_pool)
    .await
    .expect("insert layer setting");
}

async fn base_setting(
    pg: &support::Pg,
    tenant: Uuid,
    kind: &str,
    value: Option<Value>,
    required: bool,
) {
    add_setting(
        pg, tenant, "base", None, None, None, None, kind, value, required,
    )
    .await;
}

async fn industry_setting(
    pg: &support::Pg,
    tenant: Uuid,
    industry: Uuid,
    kind: &str,
    value: Option<Value>,
    required: bool,
) {
    add_setting(
        pg,
        tenant,
        "industry",
        Some(industry),
        None,
        None,
        None,
        kind,
        value,
        required,
    )
    .await;
}

async fn archetype_setting(
    pg: &support::Pg,
    tenant: Uuid,
    industry: Uuid,
    archetype: &str,
    kind: &str,
    value: Option<Value>,
    required: bool,
) {
    add_setting(
        pg,
        tenant,
        "department_archetype",
        Some(industry),
        None,
        None,
        Some(archetype),
        kind,
        value,
        required,
    )
    .await;
}

async fn department_setting(
    pg: &support::Pg,
    tenant: Uuid,
    department: Uuid,
    kind: &str,
    value: Option<Value>,
    required: bool,
) {
    add_setting(
        pg,
        tenant,
        "department",
        None,
        Some(department),
        None,
        None,
        kind,
        value,
        required,
    )
    .await;
}

async fn job_setting(
    pg: &support::Pg,
    tenant: Uuid,
    job: Uuid,
    kind: &str,
    value: Option<Value>,
    required: bool,
) {
    add_setting(
        pg,
        tenant,
        "job",
        None,
        None,
        Some(job),
        None,
        kind,
        value,
        required,
    )
    .await;
}

/// Find one resolved layer value in a release snapshot.
fn layer_value<'a>(snapshot: &'a Value, kind: &str) -> &'a Value {
    snapshot["layer_values"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["kind"] == kind))
        .unwrap_or_else(|| panic!("no layer value {kind:?} in {snapshot}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unresolvable_required_kind_refuses_with_the_kind_and_department() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let org = create_org(&pg, &seeded, "backend_engineer", json!({})).await;

    // `delivery` requires a charter; nothing in the chain sets one, and the job definition is
    // empty. The publish must refuse instead of hashing an unresolved draft.
    department_setting(&pg, seeded.tenant_id, org.delivery, "charter", None, true).await;

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "job_releases.publish",
            json!({"job_id": org.job}),
            seeded.user_id,
        ),
    )
    .await
    .expect_err("a required kind with no value must refuse");
    assert_eq!(refused.status, 409, "{refused:?}");
    assert_eq!(refused.code, "release_not_ready");
    assert!(
        refused.detail.contains("charter"),
        "the kind must be named: {}",
        refused.detail
    );
    assert!(
        refused.detail.contains("delivery"),
        "the department must be named: {}",
        refused.detail
    );

    let releases: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job_release WHERE tenant_id = $1 AND job_id = $2")
            .bind(seeded.tenant_id)
            .bind(org.job)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count releases");
    assert_eq!(releases, 0, "the refusal wrote no release row");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sibling_departments_at_different_values_refuse_until_the_job_layer_resolves() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let org = create_org(&pg, &seeded, "backend_engineer", json!({})).await;

    department_setting(
        &pg,
        seeded.tenant_id,
        org.delivery,
        "tone",
        Some(json!("direct")),
        false,
    )
    .await;
    department_setting(
        &pg,
        seeded.tenant_id,
        org.verification,
        "tone",
        Some(json!("formal")),
        false,
    )
    .await;

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "job_releases.publish",
            json!({"job_id": org.job}),
            seeded.user_id,
        ),
    )
    .await
    .expect_err("sibling values must conflict");
    assert_eq!(refused.status, 422, "{refused:?}");
    assert_eq!(refused.code, "department_conflict");
    for named in ["tone", "delivery", "verification"] {
        assert!(
            refused.detail.contains(named),
            "{named:?} must be named: {}",
            refused.detail
        );
    }

    // The job-layer row is the documented resolution: both siblings keep their value, the
    // release takes the job's.
    job_setting(
        &pg,
        seeded.tenant_id,
        org.job,
        "tone",
        Some(json!("direct")),
        false,
    )
    .await;
    let published = invoke_ok(
        &pg,
        &seeded,
        "job_releases.publish",
        json!({"job_id": org.job}),
    )
    .await;
    let tone = layer_value(&published["release_snapshot"], "tone");
    assert_eq!(tone["value"], json!("direct"));
    assert_eq!(tone["source"]["layer"], json!("job"));
    assert_eq!(tone["source"]["job"], json!("backend_engineer"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_complete_resolution_publishes_with_sources_and_two_compiles_are_identical() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let org = create_org(
        &pg,
        &seeded,
        "backend_engineer",
        json!({"charter": "job charter"}),
    )
    .await;

    sqlx::query(
        "UPDATE department SET archetype = 'knowledge_worker' WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(org.delivery)
    .execute(&pg.super_pool)
    .await
    .expect("name the department archetype");

    base_setting(&pg, seeded.tenant_id, "tone", Some(json!("neutral")), false).await;
    industry_setting(
        &pg,
        seeded.tenant_id,
        org.industry,
        "charter",
        Some(json!("industry charter")),
        false,
    )
    .await;
    industry_setting(
        &pg,
        seeded.tenant_id,
        org.industry,
        "capability",
        Some(json!("delivery")),
        true,
    )
    .await;
    archetype_setting(
        &pg,
        seeded.tenant_id,
        org.industry,
        "knowledge_worker",
        "tone",
        Some(json!("analytical")),
        false,
    )
    .await;
    department_setting(
        &pg,
        seeded.tenant_id,
        org.delivery,
        "limits",
        Some(json!({"turns": 8})),
        false,
    )
    .await;

    // Two compiles of the same pin are byte-identical (P2.1).
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let first = conveyor_store::organisation::compile_release_tx(&mut tx, seeded.tenant, org.job)
        .await
        .expect("compile once");
    let second = conveyor_store::organisation::compile_release_tx(&mut tx, seeded.tenant, org.job)
        .await
        .expect("compile twice");
    assert_eq!(first, second, "two compiles are byte-identical");
    assert_eq!(
        release_hash(&first),
        release_hash(&second),
        "the canonical hash is identical"
    );
    tx.commit().await.expect("commit");

    let published = invoke_ok(
        &pg,
        &seeded,
        "job_releases.publish",
        json!({"job_id": org.job}),
    )
    .await;
    let snapshot = &published["release_snapshot"];
    assert_eq!(
        snapshot, &first,
        "the published snapshot is the compiled one"
    );
    assert_eq!(
        published["release_hash"]
            .as_str()
            .map(|hash| hash.to_owned()),
        Some(release_hash(&first)),
        "the hash is over the recorded snapshot"
    );

    // Every effective value names its layer and the declaration it came from.
    let tone = layer_value(snapshot, "tone");
    assert_eq!(tone["value"], json!("analytical"));
    assert_eq!(tone["source"]["layer"], json!("department_archetype"));
    assert_eq!(tone["source"]["archetype"], json!("knowledge_worker"));
    assert_eq!(tone["source"]["industry"], json!("software"));

    let charter = layer_value(snapshot, "charter");
    assert_eq!(charter["value"], json!("job charter"));
    assert_eq!(charter["source"]["layer"], json!("job"));
    assert_eq!(charter["source"]["job"], json!("backend_engineer"));

    let capability = layer_value(snapshot, "capability");
    assert_eq!(capability["value"], json!("delivery"));
    assert_eq!(capability["required"], json!(true));
    assert_eq!(capability["source"]["layer"], json!("industry"));

    let limits = layer_value(snapshot, "limits");
    assert_eq!(limits["value"], json!({"turns": 8}));
    assert_eq!(limits["source"]["layer"], json!("department"));
    assert_eq!(limits["source"]["department"], json!("delivery"));
    assert_eq!(limits["source"]["ordinal"], json!(1));

    // Republishing unchanged content returns the same byte-identical release.
    let again = invoke_ok(
        &pg,
        &seeded,
        "job_releases.publish",
        json!({"job_id": org.job}),
    )
    .await;
    assert_eq!(again["id"], published["id"]);
    assert_eq!(again["release_hash"], published["release_hash"]);
    assert_eq!(again["release_snapshot"], published["release_snapshot"]);
}
