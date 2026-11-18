//! Shared test support for the scientific suites: a process-local licence-introspection stub.
//!
//! The tenant gate (conveyor#299) verifies every credential at `LICENSE_INTROSPECT_URL`; tests
//! point it here and choose a credential SHAPE by token prefix, so nothing inside the binary under
//! test is mocked:
//!
//!   licence-<tenant>   an ACTIVE licence naming that tenant, no subject   (carrier 1)
//!   onbehalf-<tenant>  an ACTIVE licence naming <tenant>, subject allowlisted (carrier 2 vouch)
//!   stranger-<tenant>  an ACTIVE licence naming <tenant>, subject NOT allowlisted
//!   unavailable-…      a 500 from the introspector (NO VERDICT → the 503 path)
//!   anything else      inactive (the negative control every refusal is paired with)

#![allow(dead_code)]

use std::sync::OnceLock;

pub const ONBEHALF_SUBJECT: &str = "0195f48b-d000-7000-8000-0000000000c3";
pub const STRANGER_SUBJECT: &str = "0195f48b-d000-7000-8000-0000000000d4";

/// The env pairs every suite appends to its `Config::from_lookup` input.
pub fn auth_vars() -> Vec<(String, String)> {
    vec![
        (
            "LICENSE_INTROSPECT_URL".to_owned(),
            introspect_url().to_owned(),
        ),
        (
            "SCIENTIFIC_PROCESSOR_ONBEHALF_SUBJECTS".to_owned(),
            ONBEHALF_SUBJECT.to_owned(),
        ),
        ("LICENSE_INTROSPECT_TIMEOUT".to_owned(), "5".to_owned()),
    ]
}

/// `X-Tenant-Id: Bearer <licence>` for a tenant, as the suites' credentialed callers send it.
pub fn licence_header(tenant: &str) -> String {
    format!("Bearer licence-{tenant}")
}

pub fn introspect_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("stub runtime");
            runtime.block_on(async move {
                let app = axum::Router::new()
                    .route("/license/introspect", axum::routing::post(stub_introspect));
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("stub bind");
                let address = listener.local_addr().expect("stub address");
                ready_tx
                    .send(format!("http://{address}/license/introspect"))
                    .expect("stub ready channel");
                axum::serve(listener, app).await.expect("stub serve");
            });
        });
        ready_rx.recv().expect("the stub starts")
    })
}

async fn stub_introspect(
    axum::Json(body): axum::Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let token = body
        .get("token")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if token.starts_with("unavailable-") {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "no verdict").into_response();
    }
    let payload = if let Some(tenant) = token.strip_prefix("licence-") {
        serde_json::json!({
            "active": true, "tenant_id": tenant, "subject_id": null, "license_id": "stub"
        })
    } else if let Some(tenant) = token.strip_prefix("onbehalf-") {
        serde_json::json!({
            "active": true, "tenant_id": tenant, "subject_id": ONBEHALF_SUBJECT,
            "license_id": "stub"
        })
    } else if let Some(tenant) = token.strip_prefix("stranger-") {
        serde_json::json!({
            "active": true, "tenant_id": tenant, "subject_id": STRANGER_SUBJECT,
            "license_id": "stub"
        })
    } else {
        serde_json::json!({ "active": false })
    };
    axum::Json(payload).into_response()
}
