//! Live action-path proof for S15 (v1-s15-1): **a capability bound to a published pack returns a
//! real hit carrying `pack` and `layer` from the cluster's own `strata_rag`**.
//!
//! `knowledge_binding` proves the conveyor's half against a stub. `strata_live` proves the bare
//! transport. This target proves the product path end to end: the platform owner publishes a pack
//! through strata's owner routes, the tenant binds it with `capabilities.bind_knowledge`, and
//! `knowledge.query` returns the upstream hit unchanged — `pack` naming the published pack and
//! `layer` naming the platform layer.
//!
//! `#[ignore]` by default: it needs a reachable `strata_rag`.
//!
//! ```sh
//! CONVEYOR_STRATA_RAG_URL=http://127.0.0.1:8092 ~/.claude/scripts/cargo-guard.py test \
//!   -p conveyor-actions --test strata_live_action -- --ignored --nocapture
//! ```
//!
//! The URL is read from `CONVEYOR_STRATA_RAG_URL`, defaulting to the compose name
//! (`http://strata-rag:8092`). The token is the compose profile's multi-tenant service token
//! (`*:conveyor-strata-dev`), so the same bearer acts as the platform owner and as the seeded
//! tenant by naming each in `X-Tenant-Id`.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const URL: &str = "http://strata-rag:8092";
/// The compose profile's `*:` service token: acts for whichever tenant `X-Tenant-Id` names.
const STRATA_TOKEN: &str = "conveyor-strata-dev";
/// `STRATA_PLATFORM_OWNER` of the running stack — the tenant that owns platform packs.
const OWNER_TENANT: &str = "0197a0c0-0000-7000-8000-0000000000a1";
const TOKEN: &str = "dev:alice";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// One call to strata as the platform owner (bearer + owner tenant header).
async fn owner_call(
    client: &reqwest::Client,
    url: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let mut request = client
        .request(method, format!("{url}{path}"))
        .bearer_auth(STRATA_TOKEN)
        .header("x-tenant-id", OWNER_TENANT)
        .header("accept", "application/json");
    if let Some(body) = &body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .unwrap_or_else(|error| panic!("strata {path}: {error}"));
    let status = response.status().as_u16();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a reachable strata_rag (CONVEYOR_STRATA_RAG_URL)"]
async fn a_bound_pack_returns_a_platform_hit_from_the_real_strata() {
    // SAFETY: set before the first `Strata::from_env` call in this binary (the action's).
    let url = std::env::var("CONVEYOR_STRATA_RAG_URL").unwrap_or_else(|_| URL.to_owned());
    support::set_env("CONVEYOR_STRATA_RAG_URL", &url);

    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the capability catalogue seed");
    let running = support::Running::start(&pg).await;

    // The capability the tenant will bind the pack to.
    let (status, body) = running
        .invoke(
            "capabilities.list",
            TOKEN,
            json!({"limit": 50}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let capability_id = output(&body)["items"][0]["id"]
        .as_str()
        .expect("a seeded capability")
        .to_owned();
    let capability_code = output(&body)["items"][0]["code"]
        .as_str()
        .expect("the capability's code")
        .to_owned();

    // The platform owner publishes a pack: root leaf → document → registration → publish.
    let client = reqwest::Client::new();
    let unique = Uuid::now_v7().simple().to_string();
    let (status, body) = owner_call(
        &client,
        &url,
        reqwest::Method::POST,
        "/api/stores",
        Some(json!({
            "kind": "leaf",
            "name": format!("s15-live-action-{unique}"),
            "description": "",
        })),
    )
    .await;
    assert_eq!(status, 201, "create the pack root: {body}");
    let root_id = body["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no store id in {body}"))
        .to_owned();

    let (status, body) = owner_call(
        &client,
        &url,
        reqwest::Method::POST,
        &format!("/api/stores/{root_id}/documents"),
        Some(json!({
            "text": format!(
                "The S15 live action-path proof: a capability with the code {capability_code} \
                 retrieves the platform knowledge published for it."
            ),
            "source": "conveyor-s15-live-action",
            "metadata": {"capability": capability_code},
            "provenance": {
                "source_url": "https://example.invalid/conveyor/s15-live-action",
                "license": "CC-BY-4.0",
                "retrieved_at": "2026-11-28T00:00:00Z",
            },
        })),
    )
    .await;
    assert_eq!(status, 201, "ingest the pack document: {body}");

    let key = format!("s15-live-action-{unique}");
    let (status, body) = owner_call(
        &client,
        &url,
        reqwest::Method::POST,
        "/api/platform/packs",
        Some(json!({"key": key, "version": 1, "root_id": root_id})),
    )
    .await;
    assert_eq!(status, 201, "register the pack: {body}");
    let (status, body) = owner_call(
        &client,
        &url,
        reqwest::Method::POST,
        &format!("/api/platform/packs/{key}/1/publish"),
        None,
    )
    .await;
    assert_eq!(status, 200, "publish the pack: {body}");

    // The tenant's own upstream token: `knowledge.query` refuses without it, before any call.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "strata_rag.token",
        STRATA_TOKEN.as_bytes(),
        "tenant",
    )
    .await
    .expect("seed the tenant's strata token");
    tx.commit().await.expect("commit");

    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            TOKEN,
            json!({
                "capability_id": capability_id,
                "pack_key": key,
                "pack_version": 1,
                "required": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "knowledge.query",
            TOKEN,
            json!({
                "capability_id": capability_id,
                "query": "which capability does the S15 live action-path proof cover",
                "top_k": 5,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let out = output(&body);
    let expected = format!("{key}@1");
    println!(
        "knowledge.query packs={} hits={}",
        out["packs"], out["hits"]
    );
    assert_eq!(
        out["packs"],
        json!([expected]),
        "the published pack answered"
    );
    let hit = &out["hits"][0];
    assert_eq!(hit["pack"], expected, "the hit keeps its pack");
    assert_eq!(hit["layer"], "platform", "the hit keeps its layer");
}
