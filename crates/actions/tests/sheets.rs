//! #366 — `sheets.export` and `sheets.import` over the real engine.
//!
//! P3's four properties: an inline export carries bytes a caller can verify; a document export
//! stores exactly one row and one object and returns the id; an import then an export reproduces
//! the cells; and a workbook is an accepted document type while a non-ZIP body is not.

mod support;

use conveyor_actions::integrations::objects::Objects;
use conveyor_actions::{InvokeRequest, UsageContext, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::inference::CallerKind;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

fn request(action: &str, user_id: Uuid, input: Value) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-sheets".to_owned()),
        trace_id: Some("trace-sheets".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: Some(UsageContext {
            caller_kind: CallerKind::AgentRun,
            run_id: Some(Uuid::now_v7()),
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: Some("agent:sheets".to_owned()),
            prompt_version: None,
            prompt_sha256: None,
            correlation_id: Some("corr-sheets".to_owned()),
        }),
    }
}

/// A small matrix, the shape the owner asked for: a header row and data rows.
fn matrix() -> Value {
    json!({
        "workbook": {
            "sheets": [{
                "name": "Report",
                "rows": [
                    ["region", "total"],
                    ["north", 1200.0],
                    ["south", 800.5]
                ]
            }]
        }
    })
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// P3.1 and P3.3 — an inline export verifies, and an import then an export reproduces it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn import_then_export_roundtrips() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let exported = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request("sheets.export", seeded.user_id, matrix()),
    )
    .await
    .expect("the export");
    assert_eq!(exported.status, 200, "{}", exported.body);
    let output = &exported.body["output"];
    assert_eq!(output["media_type"], XLSX);
    let base64 = output["base64"]
        .as_str()
        .expect("inline mode carries the bytes");
    let reported = output["sha256"].as_str().expect("a hash").to_owned();
    assert_eq!(
        output["document_id"],
        Value::Null,
        "no document in inline mode"
    );

    let imported = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "sheets.import",
            seeded.user_id,
            json!({ "content_base64": base64 }),
        ),
    )
    .await
    .expect("the import");
    assert_eq!(imported.status, 200, "{}", imported.body);
    let workbook = imported.body["output"]["workbook"].clone();
    assert_eq!(workbook["sheets"][0]["name"], json!("Report"));
    assert_eq!(workbook["sheets"][0]["rows"][0][0], json!("region"));
    assert_eq!(workbook["sheets"][0]["rows"][1][1], json!(1200.0));
    assert_eq!(workbook["sheets"][0]["rows"][2][1], json!(800.5));

    let again = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "sheets.export",
            seeded.user_id,
            json!({ "workbook": workbook }),
        ),
    )
    .await
    .expect("the second export");
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(
        again.body["output"]["sha256"].as_str(),
        Some(reported.as_str()),
        "reading a workbook and writing it back produces the same bytes"
    );
}

/// P3.2 — `as: "document"` stores one row and one object, and returns the id, not the bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn export_as_document_stores_one_object() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "sheets.export",
            seeded.user_id,
            json!({ "workbook": matrix()["workbook"].clone(), "as": "document" }),
        ),
    )
    .await
    .expect("the export");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    let output = &outcome.body["output"];
    assert_eq!(
        output["base64"],
        Value::Null,
        "document mode carries no bytes"
    );
    let document_id = output["document_id"].as_str().expect("a document id");
    let reported = output["sha256"].as_str().expect("a hash").to_owned();

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        kind: String,
        title: String,
        object_key: Option<String>,
        content_hash: Option<String>,
        meta: Value,
    }
    let rows: Vec<Row> =
        sqlx::query_as("SELECT id, kind, title, object_key, content_hash, meta FROM document")
            .fetch_all(&pg.super_pool)
            .await
            .expect("the document rows");
    assert_eq!(rows.len(), 1, "one export, one document");
    let row = &rows[0];
    assert_eq!(row.id.to_string(), document_id);
    assert_eq!(row.kind, "export");
    assert_eq!(row.title, "Report.xlsx");
    assert_eq!(row.meta["content_type"], XLSX);

    let key = row.object_key.as_deref().expect("the row addresses bytes");
    let objects = Objects::from_env().expect("the test object backend");
    let stored = objects.get(key).await.expect("the stored bytes");
    assert!(stored.starts_with(b"PK"), "a package, not JSON");
    assert_eq!(
        sha256(&stored),
        reported,
        "the response hashes the stored bytes"
    );
    assert_eq!(
        row.content_hash.as_deref(),
        Some(reported.as_str()),
        "the row hashes what it stored"
    );

    // The stored bytes are the same workbook a caller can read back.
    let imported = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "sheets.import",
            seeded.user_id,
            json!({ "content_base64": base64(&stored) }),
        ),
    )
    .await
    .expect("the import");
    assert_eq!(imported.status, 200, "{}", imported.body);
    assert_eq!(
        imported.body["output"]["workbook"]["sheets"][0]["rows"][1][1],
        json!(1200.0)
    );
}

/// P3.5 — a workbook is an accepted document type, and the sniffer still refuses a non-ZIP body.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn xlsx_is_an_allowed_document_type() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let exported = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request("sheets.export", seeded.user_id, matrix()),
    )
    .await
    .expect("the export");
    let bytes = exported.body["output"]["base64"]
        .as_str()
        .expect("inline bytes")
        .to_owned();

    let accepted = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "documents.create",
            seeded.user_id,
            json!({ "title": "Workbook", "content_type": XLSX, "content_base64": bytes }),
        ),
    )
    .await
    .expect("the upload");
    assert_eq!(accepted.status, 201, "{}", accepted.body);

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "documents.create",
            seeded.user_id,
            json!({
                "title": "Not a workbook",
                "content_type": XLSX,
                "content_base64": base64(b"this is not a zip"),
            }),
        ),
    )
    .await
    .expect_err("a non-ZIP body is not a workbook");
    assert_eq!(refused.status, 415, "{}", refused.detail);
    assert_eq!(refused.code, "unsupported_media_type");
}

fn base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
