//! The captured-contract replay for the `media-discovery` half of this crate.
//!
//! Every golden in `contract/media/golden/` is replayed **in `cases.txt` order on one app
//! instance**, against a fresh database loaded with `contract/media/seed.sql`. The order is
//! load-bearing: the collection cases run create → add → get → remove → delete on the collection
//! `collection_create` mints, and `{COLLECTION_ID}` is substituted from that response exactly as
//! `capture-goldens.sh` substitutes it while capturing.
//!
//! Fields whose volatility is declared in `contract/media/tolerances.toml` are normalised on BOTH
//! sides before comparison; everything else must match exactly, and a difference under an
//! undeclared key fails the run.
//!
//! There is no "not routed yet" bucket any more. Every captured case must replay, or the port is
//! not the port.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use std::collections::BTreeSet;
use tower::ServiceExt;

use conveyor_research::media::{self, Config};

const GOLDEN_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/contract/media/golden");
const TOLERANCES: &str = include_str!("../contract/media/tolerances.toml");
const SEED: &str = include_str!("../contract/media/seed.sql");

/// The synthetic tenant every artifact carries. The service has no default, so the harness sends
/// it — which is what makes a port that ignored the header fail.
const TENANT: &str = "00000000-0000-7000-8000-000000000001";
/// The first row of `seed.sql`, and the subject of the `items_*` happy-path cases.
const ITEM_ID: &str = "10000000-0000-7000-8000-000000000001";

/// Key names whose values are normalised before comparison.
fn volatile_keys() -> BTreeSet<String> {
    TOLERANCES
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix('"')?;
            let end = line.find('"')?;
            Some(line[..end].to_string())
        })
        .collect()
}

/// Replace every declared-volatile value with a sentinel, recursively.
fn normalise(value: &mut Value, volatile: &BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                if volatile.contains(key) {
                    *inner = Value::String("<volatile>".to_string());
                } else {
                    normalise(inner, volatile);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                normalise(item, volatile);
            }
        }
        _ => {}
    }
}

/// The `name|METHOD|path|body` rows from `cases.txt`, comments and blanks dropped.
fn cases() -> Vec<(String, String, String)> {
    include_str!("../contract/media/cases.txt")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let mut parts = line.split('|');
            let name = parts.next()?.trim().to_string();
            let method = parts.next()?.trim().to_string();
            let path = parts.next()?.trim().to_string();
            Some((name, method, path))
        })
        .collect()
}

fn app(pool: sqlx::PgPool) -> axum::Router {
    let config = Config {
        bind_host: "127.0.0.1".to_string(),
        bind_port: 0,
        enable_docs: false,
        service_name: "media-discovery".to_string(),
        version: "0.1.0".to_string(),
        database_url: None,
        max_connections: 4,
    };
    media::build_app(media::AppState::with_pool(config, pool))
}

fn read(path: &str) -> String {
    std::fs::read_to_string(format!("{GOLDEN_DIR}/{path}"))
        .unwrap_or_else(|error| panic!("cannot read {path}: {error}"))
}

#[tokio::test]
async fn every_captured_case_replays_its_answer() {
    let db = common::fresh().await;
    media::db::migrate(&db.pool)
        .await
        .expect("apply the embedded migrations");
    sqlx::raw_sql(SEED)
        .execute(&db.pool)
        .await
        .expect("load the capture's pre-state");

    let volatile = volatile_keys();
    assert!(
        volatile.contains("request_id"),
        "the tolerance list must declare request_id"
    );
    assert!(
        !volatile.contains("tenant_id"),
        "tenant_id must stay exact: it is the field the sanitised capture pins"
    );

    let app = app(db.pool.clone());
    let mut collection_id = String::new();
    let mut replayed = 0usize;
    let captured = cases();
    assert!(!captured.is_empty(), "cases.txt is empty");

    for (name, method, raw_path) in &captured {
        let path = raw_path
            .replace("{ITEM_ID}", ITEM_ID)
            .replace("{COLLECTION_ID}", &collection_id);

        // The body comes from the capture; the path does not, because two of the paths carry an
        // id this run mints.
        let request_text = read(&format!("{name}.request"));
        let body = request_text.lines().nth(1).unwrap_or_default().to_string();

        let captured_status: u16 = read(&format!("{name}.status"))
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("{name}: captured status is not a number"));

        let mut builder = Request::builder()
            .method(method.as_str())
            .uri(&path)
            .header("x-tenant-id", TENANT);
        if !body.is_empty() {
            builder = builder.header("content-type", "application/json");
        }
        let request = builder.body(Body::from(body)).expect("build the request");

        let response = app.clone().oneshot(request).await.expect("the app answers");
        assert_eq!(
            response.status(),
            StatusCode::from_u16(captured_status).expect("captured status is a StatusCode"),
            "{name}: status differs from the capture"
        );

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("read the body")
            .to_bytes();
        let mut got: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            panic!(
                "{name}: body is not JSON: {:?}",
                String::from_utf8_lossy(&bytes)
            )
        });
        let mut want: Value = serde_json::from_str(&read(&format!("{name}.body")))
            .unwrap_or_else(|_| panic!("{name}: captured body is not JSON"));

        // From `got`, not `want`: the id this run minted is the one the later cases must address.
        // The captured id names a row that lives in the reference's scratch database, not here.
        if name == "collection_create" {
            collection_id = got
                .get("id")
                .and_then(Value::as_str)
                .expect("collection_create carries an id")
                .to_string();
        }

        normalise(&mut got, &volatile);
        normalise(&mut want, &volatile);
        assert_eq!(want, got, "{name}: the answer drifted from the capture");
        replayed += 1;
    }

    assert_eq!(
        replayed,
        captured.len(),
        "every captured case must replay; {replayed} of {} did",
        captured.len()
    );
}

#[tokio::test]
async fn the_served_routes_equal_the_capture() {
    media::contract::check(media::SERVED).expect("the lock check is green at this head");
    assert_eq!(
        media::contract::operations(&media::openapi()).len(),
        media::SERVED.len(),
        "a green lock and a short route list cannot both be true"
    );
    assert_eq!(media::SERVED.len(), 23, "the capture has 23 operations");
}
