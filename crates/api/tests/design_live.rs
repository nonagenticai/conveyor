//! #128: the live, unauthenticated design window.
//!
//! `cargo test -p conveyor-api --test design_live`.
//!
//! The pack is served from Postgres for one configured public tenant: the snapshot and the
//! published outputs are live, a private row is a `404 no-store`, the ETag revalidates, HEAD has
//! no body, a malformed id is a live `404`, an unset tenant is a `404`, and a closed pool is a
//! `503`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use base64::Engine as _;
use serde_json::json;

/// The support config with one extra setting: the tenant whose published rows are public.
fn config_with_public_tenant(pg: &support::Pg, slug: &str) -> conveyor_api::Config {
    let runtime_url = pg.runtime_url.clone();
    let owner_url = pg.owner_url.clone();
    let slug = slug.to_owned();
    conveyor_api::Config::from_lookup(move |key| match key {
        "DATABASE_URL" | "DATABASE_LISTEN_URL" => Some(runtime_url.clone()),
        "DATABASE_MIGRATE_URL" => Some(owner_url.clone()),
        "CONVEYOR_DEV_AUTH" => Some("1".into()),
        "BIND_ADDR" => Some("127.0.0.1:0".into()),
        "APP_KEY" => Some("conveyor".into()),
        "CONVEYOR_PUBLIC_TENANT" => Some(slug.clone()),
        "CONVEYOR_GIT_SHA" => Some("0123456789abcdef".into()),
        _ => None,
    })
    .expect("test config")
}

async fn get(running: &support::Running, path: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{}{path}", running.base()))
        .send()
        .await
        .expect("http request")
}

#[tokio::test(flavor = "multi_thread")]
async fn published_attachments_are_served_safely_and_leave_with_the_parent() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "public", "alice", "Alice").await;
    let running =
        support::Running::start_with_config(config_with_public_tenant(&pg, "public")).await;

    let parent = compose(&running, "dev:alice", "Evidence", "# Evidence", true).await;
    let png_bytes = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let png = base64::engine::general_purpose::STANDARD.encode(png_bytes);
    let (status, attachment) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "evidence.png",
                "content_type": "image/png",
                "content_base64": png,
                "parent_document_id": parent,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{attachment}");
    let attachment = &attachment["output"];
    let id = attachment["id"].as_str().expect("attachment id").to_owned();
    let hash = attachment["content_hash"]
        .as_str()
        .expect("content hash")
        .to_owned();

    let response = get(&running, &format!("/design/docs/attachment-{id}")).await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );
    assert!(
        response
            .headers()
            .get("content-disposition")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .starts_with("inline"),
        "a png is inline"
    );
    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    assert!(
        response
            .headers()
            .get("content-security-policy")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .contains("sandbox"),
        "every attachment is sandboxed"
    );
    let etag = format!("\"{hash}\"");
    assert_eq!(
        response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str())
    );
    assert_eq!(response.bytes().await.expect("bytes").as_ref(), png_bytes);

    // The same ETag revalidates, and HEAD carries no body.
    let revalidated = reqwest::Client::new()
        .get(format!("{}/design/docs/attachment-{id}", running.base()))
        .header("if-none-match", &etag)
        .send()
        .await
        .expect("revalidate");
    assert_eq!(revalidated.status(), 304);
    let head = reqwest::Client::new()
        .head(format!("{}/design/docs/attachment-{id}", running.base()))
        .send()
        .await
        .expect("head");
    assert_eq!(head.status(), 200);
    assert!(head.bytes().await.expect("head bytes").is_empty());

    // An SVG is a download, never inline.
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
    let (status, svg_attachment) = running
        .invoke(
            "documents.create",
            "dev:alice",
            json!({
                "title": "diagram.svg",
                "content_type": "image/svg+xml",
                "content_base64": base64::engine::general_purpose::STANDARD.encode(svg),
                "parent_document_id": parent,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{svg_attachment}");
    let svg_id = svg_attachment["output"]["id"]
        .as_str()
        .expect("svg id")
        .to_owned();
    let response = get(&running, &format!("/design/docs/attachment-{svg_id}")).await;
    assert_eq!(response.status(), 200);
    assert!(
        response
            .headers()
            .get("content-disposition")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .starts_with("attachment"),
        "an svg is never inline"
    );

    // The public index carries both attachments under the published output.
    let outputs = get(&running, "/design/docs/outputs.js").await;
    assert_eq!(outputs.status(), 200);
    let index = outputs.text().await.expect("outputs body");
    assert!(index.contains(&id), "{index}");
    assert!(index.contains(&svg_id), "{index}");

    // Unpublishing the parent removes both on the next request.
    let (status, unpublished) = running
        .invoke(
            "documents.publish",
            "dev:alice",
            json!({"id": parent, "publish": false}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{unpublished}");
    let response = get(&running, &format!("/design/docs/attachment-{id}")).await;
    assert_eq!(response.status(), 404);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
}

async fn compose(
    running: &support::Running,
    token: &str,
    title: &str,
    body: &str,
    publishable: bool,
) -> String {
    let (status, document) = running
        .invoke(
            "documents.compose",
            token,
            json!({"title": title, "body": body, "publishable": publishable}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{document}");
    document["output"]["id"].as_str().expect("id").to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_live_window_serves_published_rows_and_hides_the_rest() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "public", "alice", "Alice").await;
    support::seed(&pg, "other", "bob", "Bob").await;
    let running =
        support::Running::start_with_config(config_with_public_tenant(&pg, "public")).await;

    let body = "# Programme note\n\nA decision recorded from the console.";
    let published = compose(&running, "dev:alice", "Design note", body, true).await;
    let private = compose(&running, "dev:alice", "Private note", body, false).await;
    let foreign = compose(&running, "dev:bob", "Other tenant", body, true).await;

    // The snapshot is live and names the configured tenant and the deployed sha.
    let response = get(&running, "/design/data/snapshot.js").await;
    assert_eq!(response.status(), 200);
    let cache = response
        .headers()
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(cache.contains("must-revalidate"), "{cache}");
    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .expect("an ETag")
        .to_owned();
    let snapshot = response.text().await.expect("snapshot");
    assert!(
        snapshot.starts_with("window.CONVEYOR_SNAPSHOT = {"),
        "{snapshot}"
    );
    assert!(snapshot.contains("\"tenant\":\"public\""), "{snapshot}");
    assert!(snapshot.contains("0123456789abcdef"), "{snapshot}");

    // A matching If-None-Match is a 304 with the same ETag and no new body.
    let revalidated = reqwest::Client::new()
        .get(format!("{}/design/data/snapshot.js", running.base()))
        .header("if-none-match", &etag)
        .send()
        .await
        .expect("revalidate");
    assert_eq!(revalidated.status(), 304);
    assert_eq!(
        revalidated
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str())
    );

    // HEAD mirrors GET without a body.
    let head = reqwest::Client::new()
        .head(format!("{}/design/data/snapshot.js", running.base()))
        .send()
        .await
        .expect("head");
    assert_eq!(head.status(), 200);
    assert!(head.headers().get("etag").is_some());
    assert!(head.bytes().await.expect("head body").is_empty());

    // The index lists the published row and not the private or the foreign one.
    let index = get(&running, "/design/docs/outputs.js").await;
    assert_eq!(index.status(), 200);
    let index = index.text().await.expect("outputs");
    assert!(index.starts_with("window.CONVEYOR_OUTPUTS = {"), "{index}");
    assert!(index.contains(&published), "{index}");
    assert!(!index.contains(&private), "{index}");
    assert!(!index.contains(&foreign), "{index}");

    // One rendered page carries the Markdown as HTML.
    let page = get(&running, &format!("/design/docs/output-{published}.html")).await;
    assert_eq!(page.status(), 200);
    let page = page.text().await.expect("page");
    assert!(page.contains("<h1>Programme note</h1>"), "{page}");
    assert!(
        page.contains("A decision recorded from the console."),
        "{page}"
    );

    // A private row, a foreign row and a malformed id are live 404s that are never cached.
    for path in [
        format!("/design/docs/output-{private}.html"),
        format!("/design/docs/output-{foreign}.html"),
        "/design/docs/output-not-a-uuid.html".to_owned(),
    ] {
        let miss = get(&running, &path).await;
        assert_eq!(miss.status(), 404, "{path}");
        assert_eq!(
            miss.headers()
                .get("cache-control")
                .and_then(|value| value.to_str().ok()),
            Some("no-store"),
            "{path}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn public_bytes_are_redacted_and_inert() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "public", "alice", "Alice").await;
    let running =
        support::Running::start_with_config(config_with_public_tenant(&pg, "public")).await;

    // Built at runtime so the source does not carry a credential-shaped literal.
    let token = format!("ghp_{}", "A".repeat(30));
    let body = format!(
        "# Unsafe\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n\
         [click](javascript:alert(1))\n\nalice@example.com\n\n{token}\n"
    );
    let id = compose(&running, "dev:alice", &format!("note {token}"), &body, true).await;

    let page = get(&running, &format!("/design/docs/output-{id}.html"))
        .await
        .text()
        .await
        .expect("page");
    assert!(!page.contains("<script"), "{page}");
    assert!(!page.contains("<img"), "{page}");
    assert!(page.contains("&lt;img"), "{page}");
    assert!(!page.to_ascii_lowercase().contains("javascript:"), "{page}");
    assert!(!page.contains("alice@example.com"), "{page}");
    assert!(!page.contains(&token), "{page}");
    assert!(page.contains("[redacted-token]"), "{page}");

    let index = get(&running, "/design/docs/outputs.js")
        .await
        .text()
        .await
        .expect("index");
    assert!(!index.contains(&token), "{index}");
    assert!(index.contains("[redacted-token]"), "{index}");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_public_tenant_is_a_closed_window() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "public", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    for path in [
        "/design/data/snapshot.js",
        "/design/docs/outputs.js",
        "/design/docs/output-00000000-0000-0000-0000-000000000000.html",
    ] {
        let response = get(&running, path).await;
        assert_eq!(response.status(), 404, "{path}");
        assert_eq!(
            response
                .headers()
                .get("cache-control")
                .and_then(|value| value.to_str().ok()),
            Some("no-store"),
            "{path}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_pool_is_a_503_not_a_placeholder() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "public", "alice", "Alice").await;
    let running =
        support::Running::start_with_config(config_with_public_tenant(&pg, "public")).await;
    running.state.pool.close().await;

    let response = get(&running, "/design/data/snapshot.js").await;
    assert_eq!(response.status(), 503);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_public_tenant_is_a_closed_window() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "public", "alice", "Alice").await;
    let running =
        support::Running::start_with_config(config_with_public_tenant(&pg, "no-such-tenant")).await;

    let response = get(&running, "/design/data/snapshot.js").await;
    assert_eq!(response.status(), 404);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
}
