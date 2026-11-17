//! #170 P3.2 — the deployed service account reads and **cannot write**.
//!
//! `#[ignore]`: this test needs the mounted in-cluster credential, so it is run from the worker
//! pod (or with the token and address in the environment), never in CI:
//!
//! ```text
//! kubectl -n conveyor exec deploy/conveyor-worker -- \
//!   cargo test -p conveyor-ops --test client_live -- --ignored --nocapture
//! ```

use conveyor_ops::OpsConfig;

/// A create is outside every grant the worker holds, so the API must answer `403`.
#[tokio::test]
#[ignore = "needs the in-cluster service-account credential"]
async fn the_service_account_cannot_write() {
    let config = OpsConfig::from_env().expect("the pod mounts a service-account token");
    let url = format!(
        "{}/api/v1/namespaces/conveyor-jobs/pods",
        config.api_url.trim_end_matches('/')
    );

    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth(&config.token)
        .json(&serde_json::json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {"name": "conveyor-ops-write-probe"},
            "spec": {"containers": [{"name": "probe", "image": "busybox"}]}
        }))
        .send()
        .await
        .expect("the API answers");

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    assert_eq!(
        status,
        reqwest::StatusCode::FORBIDDEN,
        "a pod create must be refused, got {status}: {body}"
    );
    println!("write refused as expected: {status} {body}");
}
