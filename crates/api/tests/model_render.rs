//! S08 Part B2's served surface: the three exchange actions and the two byte routes.
//!
//! What only the served surface can prove, and what the pure tests cannot:
//!
//! * the action and the byte route return **the same bytes** — the whole reason the route is
//!   allowed to exist is that it shares one renderer, and this is where that claim is measured;
//! * an import writes a **draft** revision (not a publication) and leaves the model publishable;
//! * a refused document is refused over HTTP with a problem body, not a panic;
//! * a second tenant sees nothing.
//!
//! Calls are HTTP against the served router as a dev principal. Assertions about what was
//! written read the **superuser** pool, because RLS would hide exactly the row a refused write
//! might have created (CONTRACT §2).

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// The smallest ArchiMate exchange document: two concepts, one relationship, and ids that are
/// not UUIDs so the v5 mapping runs.
const ARCHIMATE_XMI: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<model xmlns="http://www.opengroup.org/xsd/archimate/3.0/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" identifier="m-one">
  <name xml:lang="en">Imported onboarding</name>
  <elements>
    <element identifier="id-actor" xsi:type="BusinessActor"><name xml:lang="en">New engineer</name></element>
    <element identifier="id-component" xsi:type="ApplicationComponent"><name xml:lang="en">Conveyor</name></element>
  </elements>
  <relationships>
    <relationship identifier="id-serving" source="id-actor" target="id-component" xsi:type="ServingRelationship"/>
  </relationships>
</model>
"#;

/// A raw GET — the byte routes answer with a document, so the JSON-parsing helper is no use.
async fn get_bytes(
    running: &support::Running,
    path: &str,
    token: &str,
    headers: &[(&str, &str)],
) -> (u16, String, Vec<u8>) {
    let client = reqwest::Client::new();
    let mut request = client
        .get(format!("{}{}", running.base(), path))
        .header("authorization", format!("Bearer {token}"));
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request.send().await.expect("http request");
    let status = response.status().as_u16();
    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = response.bytes().await.unwrap_or_default().to_vec();
    (status, etag, bytes)
}

#[tokio::test]
async fn the_byte_route_returns_exactly_the_bytes_the_action_returns() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.import_xmi",
            "dev:alice",
            json!({"xml": ARCHIMATE_XMI}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "an import creates a model: {body}");
    let model_id = output(&body)["model_id"]
        .as_str()
        .expect("model_id")
        .to_owned();

    let (status, body) = running
        .invoke(
            "models.render",
            "dev:alice",
            json!({"model_id": model_id, "format": "png"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "render answers: {body}");
    let out = output(&body);
    let encoded = out["content_b64"].as_str().expect("content_b64");
    let from_action = BASE64.decode(encoded).expect("the payload is base64");
    assert_eq!(
        out["content_sha256"].as_str().expect("content_sha256"),
        conveyor_render::sha256_hex(&from_action),
        "the digest names the bytes it shipped"
    );
    assert_eq!(&from_action[..8], b"\x89PNG\r\n\x1a\n", "a PNG");

    let (status, etag, from_route) = get_bytes(
        &running,
        &format!("/api/models/{model_id}/render?format=png"),
        "dev:alice",
        &[],
    )
    .await;
    assert_eq!(status, 200, "the route answers");
    assert_eq!(
        from_route, from_action,
        "the action and the route must be the same bytes — they share one renderer"
    );
    assert_eq!(
        etag,
        format!("\"{}\"", conveyor_render::sha256_hex(&from_route)),
        "the ETag is the content digest"
    );

    // A second read of the same page is a `304` and no render.
    let (status, _, body) = get_bytes(
        &running,
        &format!("/api/models/{model_id}/render?format=png"),
        "dev:alice",
        &[("if-none-match", etag.as_str())],
    )
    .await;
    assert_eq!(status, 304, "If-None-Match matched: {body:?}");
}

#[tokio::test]
async fn the_xmi_route_returns_the_document_the_action_returned() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.import_xmi",
            "dev:alice",
            json!({"xml": ARCHIMATE_XMI}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "import: {body}");
    let model_id = output(&body)["model_id"]
        .as_str()
        .expect("model_id")
        .to_owned();

    let (status, body) = running
        .invoke(
            "models.export_xmi",
            "dev:alice",
            json!({"model_id": model_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "export: {body}");
    let exported = output(&body)["xml"].as_str().expect("xml").to_owned();
    assert!(
        exported.contains("http://www.opengroup.org/xsd/archimate/3.0/"),
        "the ArchiMate dialect"
    );

    let (status, etag, route) = get_bytes(
        &running,
        &format!("/api/models/{model_id}/xmi"),
        "dev:alice",
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        String::from_utf8(route).expect("the route answers UTF-8"),
        exported,
        "the route and the action export the same document"
    );
    assert!(!etag.is_empty(), "the route sets an ETag");
}

#[tokio::test]
async fn an_import_writes_a_draft_revision_and_the_model_can_still_be_published() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.import_xmi",
            "dev:alice",
            json!({"xml": ARCHIMATE_XMI}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "import: {body}");
    let out = output(&body).clone();
    assert_eq!(out["seq"], 1, "the import freezes revision 1");
    assert_eq!(out["elements"], 2);
    assert_eq!(out["relationships"], 1);
    let model_id = out["model_id"].as_str().expect("model_id").to_owned();

    // The revision is a DRAFT: an import is not a publication.
    let (kind, published, head): (String, Option<uuid::Uuid>, i32) = sqlx::query_as(
        "SELECT r.status, m.published_revision_id, m.head_revision
           FROM model m JOIN revision r
             ON r.tenant_id = m.tenant_id AND r.entity_type = 'model' AND r.entity_id = m.id
          WHERE m.tenant_id = $1 AND m.id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(uuid::Uuid::parse_str(&model_id).expect("a uuid"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the revision");
    assert_eq!(kind, "draft", "the import writes a draft");
    assert!(published.is_none(), "nothing is published by an import");
    assert_eq!(head, 1, "head_revision advanced");

    // And the model is publishable afterwards, at seq 2 — the import did not consume seq 1 in
    // a way that collides with the next publish. `models.publish` replaces the header, so it
    // needs `If-Match` (CONTRACT §3); the import bumped the version, so the caller echoes the
    // version the import returned rather than assuming 1.
    let version = out["version"]
        .as_i64()
        .expect("the import returns the row's version");
    let (status, body) = running
        .invoke(
            "models.publish",
            "dev:alice",
            json!({"id": model_id}),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    // 201, not 200: `models.publish` answers `created` — the revision is the created thing.
    assert_eq!(status, 201, "publish after import: {body}");
    assert_eq!(
        output(&body)["seq"],
        2,
        "the publish is the second revision"
    );
}

#[tokio::test]
async fn an_out_of_profile_construct_is_reported_and_the_rest_imports() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // `Equipment` is the physical layer — outside the curated profile (S08 §5.4).
    let document = ARCHIMATE_XMI.replace(
        r#"<element identifier="id-component" xsi:type="ApplicationComponent"><name xml:lang="en">Conveyor</name></element>"#,
        r#"<element identifier="id-component" xsi:type="ApplicationComponent"><name xml:lang="en">Conveyor</name></element>
    <element identifier="id-rack" xsi:type="Equipment"><name xml:lang="en">Rack 4</name></element>"#,
    );

    let (status, body) = running
        .invoke(
            "models.import_xmi",
            "dev:alice",
            json!({"xml": document}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "the import still succeeds: {body}");
    let out = output(&body);
    assert_eq!(out["elements"], 2, "the two in-profile elements landed");
    let unsupported = out["unsupported"].as_array().expect("unsupported[]");
    assert_eq!(
        unsupported.len(),
        1,
        "the equipment is reported, not dropped"
    );
    assert_eq!(unsupported[0]["construct"], "element");
    assert!(
        unsupported[0]["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "a reason is given"
    );
}

#[tokio::test]
async fn a_document_with_a_doctype_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let hostile = format!(
        r#"<?xml version="1.0"?>
<!DOCTYPE model [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
{}"#,
        ARCHIMATE_XMI
    );
    let (status, body) = running
        .invoke(
            "models.import_xmi",
            "dev:alice",
            json!({"xml": hostile}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "a DOCTYPE is refused: {body}");
    assert!(
        body["type"]
            .as_str()
            .is_some_and(|kind| kind.contains("invalid")),
        "a problem body, not a panic: {body}"
    );
    // And nothing was written: the refusal happens before the first insert.
    let models: i64 = sqlx::query_scalar("SELECT count(*) FROM model WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the models");
    assert_eq!(models, 0, "a refused document writes nothing");
}

#[tokio::test]
async fn another_tenant_cannot_render_the_model() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.import_xmi",
            "dev:alice",
            json!({"xml": ARCHIMATE_XMI}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "import: {body}");
    let model_id = output(&body)["model_id"]
        .as_str()
        .expect("model_id")
        .to_owned();

    let (status, _, bytes) = get_bytes(
        &running,
        &format!("/api/models/{model_id}/render?format=svg"),
        "dev:bob",
        &[],
    )
    .await;
    assert_eq!(status, 404, "another tenant's model is not found");
    assert!(bytes.is_empty() || bytes.starts_with(b"{"));

    let (status, body) = running
        .invoke(
            "models.render",
            "dev:bob",
            json!({"model_id": model_id, "format": "svg"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "and the action agrees: {body}");
}
