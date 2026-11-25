//! #526 (architecture findings) — an artefact names its project and its standard type.
//!
//! The phase gate resolves an approved artefact by `(project_id, artifact_type key, phase)`, so
//! a row without either can never evidence a phase; it would only sit in the repository as a
//! row nothing can read. `artifacts_create` refuses the absent field by name and writes nothing.
//!
//! `cargo test -p conveyor-actions --test togaf_artifact_requirements`.

mod support;

#[path = "../../api/tests/architecture_support/mod.rs"]
mod arch;

use serde_json::{Value, json};

/// Every artefact row of the tenant, read as the superuser so a refused insert cannot hide
/// behind the request's own scope.
async fn artifact_rows(f: &arch::Fixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM architecture_artifact WHERE tenant_id = $1")
        .bind(f.seeded.tenant_id)
        .fetch_one(&f.pg.super_pool)
        .await
        .expect("count the tenant's artefacts")
}

#[tokio::test]
async fn an_artefact_without_a_project_is_refused() {
    let f = arch::fixture().await;
    arch::tailor(&f, json!({}), json!({})).await;
    let artifact_type_id = arch::type_id(&f.running, "principles_catalog").await;
    let body = json!({
        "artifact_type_id": artifact_type_id,
        "phase_code": "preliminary",
        "plateau": "psa",
        "artifact_kind": "principles_catalog",
        "title": "Unowned principles",
    });

    // Absent is refused by name...
    let (status, refused) = arch::call(
        &f.running,
        "architecture.artifacts_create",
        body.clone(),
        None,
    )
    .await;
    assert_eq!(status, 422, "an unowned artefact was created: {refused}");
    assert_eq!(refused["errors"][0]["field"], "project_id", "{refused}");
    assert_eq!(refused["errors"][0]["code"], "required", "{refused}");

    // ...and an explicit null is the same absent field.
    let mut null = body;
    null["project_id"] = Value::Null;
    let (status, refused) =
        arch::call(&f.running, "architecture.artifacts_create", null, None).await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["errors"][0]["field"], "project_id", "{refused}");
    assert_eq!(
        artifact_rows(&f).await,
        0,
        "a refused create wrote an artefact row"
    );
}

#[tokio::test]
async fn an_artefact_without_its_type_is_refused() {
    let f = arch::fixture().await;
    let project = arch::raw_project(&f.pg, f.seeded.tenant_id, "Artefact requirements").await;
    let body = json!({
        "project_id": project,
        "phase_code": "preliminary",
        "plateau": "psa",
        "artifact_kind": "principles_catalog",
        "title": "Untyped principles",
    });

    let (status, refused) = arch::call(
        &f.running,
        "architecture.artifacts_create",
        body.clone(),
        None,
    )
    .await;
    assert_eq!(status, 422, "an untyped artefact was created: {refused}");
    assert_eq!(
        refused["errors"][0]["field"], "artifact_type_id",
        "{refused}"
    );
    assert_eq!(refused["errors"][0]["code"], "required", "{refused}");

    let mut null = body;
    null["artifact_type_id"] = Value::Null;
    let (status, refused) =
        arch::call(&f.running, "architecture.artifacts_create", null, None).await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["errors"][0]["field"], "artifact_type_id",
        "{refused}"
    );
    assert_eq!(
        artifact_rows(&f).await,
        0,
        "a refused create wrote an artefact row"
    );
}
