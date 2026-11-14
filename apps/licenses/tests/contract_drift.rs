//! The lock check's document drift must not name an operation the port already serves.
//!
//! Sibling slices land routes on their own branches, so the absolute gap count moves with every
//! tree. What must hold on every tree is the other direction: a ported operation must not drift.
//! This pins the drift entries the #45 contract slice closed — the token response components, the
//! validation-error body, the shared `X-Tenant-Id` `$ref` parameters and the delete query's
//! nullable integer — and requires every remaining document-drift row to belong to the in-flight
//! BYO-mode slice, which is not on this branch.

/// One substring per closed drift entry; `--check-lock` names each by its normalized path.
const CLOSED: [&str; 13] = [
    "/components/TokenIntrospectResponse",
    "/components/TokenRevokeResponse",
    "/components/ValidationError",
    "/paths//apps/get/parameters/0",
    "/paths//apps/post/parameters/0",
    "/paths//apps/{app_id}/delete/parameters/0",
    "/paths//apps/{app_id}/get/parameters/0",
    "/paths//apps/{app_id}/licenses/get/parameters/0",
    "/paths//apps/{app_id}/delete/parameters/2/schema",
    "/paths//internal/tenants/{tenant_id}/seat-count",
    "/paths//licenses/active",
    "/paths//licenses/{license_id}/tokens",
    "/paths//tenants/{name_or_id}/grants/{owner_key}",
];

#[test]
fn the_ported_operations_do_not_drift_from_the_capture() {
    let served = serde_json::to_value(conveyor_licenses::router::openapi()).expect("serialises");
    let message =
        match conveyor_licenses::contract::check(&served, conveyor_licenses::router::ROUTES) {
            // Nothing drifts at all: every closed entry is trivially absent.
            Ok(()) => return,
            Err(error) => error.to_string(),
        };

    for entry in CLOSED {
        assert!(
            !message.contains(entry),
            "{entry} still drifts from the capture: {message}"
        );
    }

    // A NEW drift row is a regression and must fail here rather than wait for a manual run. Only
    // the BYO-mode slice (in flight on a sibling branch) may still appear.
    if let Some((_, drift)) = message.split_once("document drift") {
        for row in drift.split('|') {
            let row = row.trim();
            if row.contains("/components/") {
                assert!(row.contains("ByoMode"), "unexpected component drift: {row}");
            }
            if row.contains("/paths//") {
                assert!(row.contains("byo-mode"), "unexpected path drift: {row}");
            }
        }
    }
}
