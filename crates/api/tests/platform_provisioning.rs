//! The Platform surface against a REAL server, a REAL database and a stub upstream.
//!
//! `cargo test -p conveyor-api --test platform_provisioning`.
//!
//! The stub is one axum server playing the three services the wizard talks to (Keycloak
//! admin, provisioning, billing) and the secure-apps token endpoint. What is proven here is
//! the conveyor's own contract — auth, ordering, the local rows, and the refusal paths — not
//! the upstreams' behaviour, which their own suites own.
//!
//! Three promises, each a way this could be wrong in production:
//! 1. a tenant admin is refused; a super-admin is admitted;
//! 2. create writes the tenant with PROVISIONING's id, the invited person and the admin
//!    membership, and a provisioning failure writes nothing;
//! 3. billing is readable by the tenant's own admin and refused to a member.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use uuid::Uuid;

struct Stub {
    tenant_id: Uuid,
    users: Mutex<HashMap<String, Uuid>>,
    provision_fail: bool,
}

async fn token() -> Json<Value> {
    Json(json!({"access_token": "svc-token", "expires_in": 3600, "token_type": "Bearer"}))
}

async fn provision(State(stub): State<Arc<Stub>>, body: Json<Value>) -> (StatusCode, Json<Value>) {
    if stub.provision_fail {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": "upstream exploded"})),
        );
    }
    let keycloak_user_id = body
        .get("keycloak_user_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    (
        StatusCode::CREATED,
        Json(json!({
            "idempotency_key": body.get("idempotency_key").cloned().unwrap_or(Value::Null),
            "status": "success",
            "tenant_id": stub.tenant_id,
            "keycloak_user_id": keycloak_user_id,
            "already_existed": true,
            "temporary_password": null,
            "licenses": [{"app_key": "conveyor", "license_id": "lic-1", "token": null}],
            "warnings": [],
            "partial_failures": [],
            "replayed": false
        })),
    )
}

async fn find_user(
    State(stub): State<Arc<Stub>>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    let email = query.get("email").cloned().unwrap_or_default();
    let created = stub.users.lock().expect("stub lock").get(&email).copied();
    match created {
        None => Json(json!([])),
        Some(id) => Json(json!([{"id": id.to_string(), "username": email, "email": email}])),
    }
}

async fn create_user(State(stub): State<Arc<Stub>>, body: Json<Value>) -> StatusCode {
    let email = body
        .get("email")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    stub.users
        .lock()
        .expect("stub lock")
        .entry(email)
        .or_insert_with(Uuid::now_v7);
    StatusCode::CREATED
}

async fn email(_id: Path<String>) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn active_licence() -> Json<Value> {
    Json(json!({"active": true, "status": "active", "expires_at": null}))
}

async fn billing(Path(_id): Path<String>) -> Json<Value> {
    Json(
        json!({"data": {"licenses": [{"app_key": "conveyor", "costs": {"billed_cost_usd": "1.20"}}]}, "meta": {}}),
    )
}

async fn costs(Path(_id): Path<String>) -> Json<Value> {
    Json(json!({"data": {"by_project": [], "total_cost_usd": "1.20"}, "meta": {}}))
}

async fn actors(Path(_id): Path<String>) -> Json<Value> {
    Json(json!({"data": {"by_actor": [], "total_cost_usd": "1.20"}, "meta": {}}))
}

/// Start the stub; `provision_fail` makes `/api/provision` answer 500.
async fn start_stub(provision_fail: bool) -> (String, Uuid) {
    let tenant_id = Uuid::now_v7();
    let stub = Arc::new(Stub {
        tenant_id,
        users: Mutex::new(HashMap::new()),
        provision_fail,
    });
    let app = Router::new()
        .route("/token", post(token))
        .route(
            "/realms/conveyor/protocol/openid-connect/token",
            post(token),
        )
        .route("/api/provision", post(provision))
        .route("/admin/realms/conveyor/users", get(find_user))
        .route("/admin/realms/conveyor/users", post(create_user))
        .route(
            "/admin/realms/conveyor/users/{id}/execute-actions-email",
            put(email),
        )
        .route("/licenses/active", get(active_licence))
        .route("/api/billing/tenant/{id}", get(billing))
        .route("/api/costs/tenant/{id}", get(costs))
        .route("/api/costs/tenant/{id}/actors", get(actors))
        .with_state(stub);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
    let addr = listener.local_addr().expect("stub addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("stub server");
    });
    (format!("http://{addr}"), tenant_id)
}

fn config_with_services(pg: &support::Pg, stub: &str) -> conveyor_api::Config {
    let runtime_url = pg.runtime_url.clone();
    let owner_url = pg.owner_url.clone();
    let base = stub.to_owned();
    conveyor_api::Config::from_lookup(move |key| match key {
        "DATABASE_URL" | "DATABASE_LISTEN_URL" => Some(runtime_url.clone()),
        "DATABASE_MIGRATE_URL" => Some(owner_url.clone()),
        "CONVEYOR_DEV_AUTH" => Some("1".into()),
        "BIND_ADDR" => Some("127.0.0.1:0".into()),
        "APP_KEY" => Some("conveyor".into()),
        "LICENCES_BASE_URL" => Some(base.clone()),
        "SERVICE_TOKEN_URL" => Some(format!("{base}/token")),
        "SERVICE_CLIENT_ID" => Some("conveyor-backend".into()),
        "SERVICE_CLIENT_SECRET" => Some("test-secret".into()),
        "PROVISIONING_BASE_URL" => Some(base.clone()),
        "BILLING_BASE_URL" => Some(base.clone()),
        "KEYCLOAK_ADMIN_URL" => Some(base.clone()),
        "KEYCLOAK_ADMIN_REALM" => Some("conveyor".into()),
        "KEYCLOAK_ADMIN_CLIENT_ID" => Some("conveyor-backend".into()),
        "KEYCLOAK_ADMIN_CLIENT_SECRET" => Some("admin-secret".into()),
        _ => None,
    })
    .expect("test config")
}

async fn make_super_admin(pg: &support::Pg, login: &str) {
    let subject = format!("dev:{login}");
    sqlx::query(
        "INSERT INTO platform_admin (user_id)
         SELECT id FROM app_user WHERE oidc_subject = $1
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(subject)
    .execute(&pg.owner)
    .await
    .expect("grant platform_admin");
}

#[tokio::test]
async fn super_admin_only_and_the_create_sequence_writes_the_local_rows() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-a", "bob", "Bob").await;
    // `support::seed` makes every login an admin; demote bob so dev-a has one admin.
    let tenant_a = conveyor_domain::TenantId::from_uuid(seeded.tenant_id).expect("tenant");
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant_a)
        .await
        .expect("scope");
    sqlx::query(
        "UPDATE membership SET role = 'member'
          WHERE user_id = (SELECT id FROM app_user WHERE oidc_subject = 'dev:bob')",
    )
    .execute(&mut *tx)
    .await
    .expect("demote bob");
    tx.commit().await.expect("commit");
    make_super_admin(&pg, "alice").await;

    let (stub, provisioning_tenant_id) = start_stub(false).await;
    let running = support::Running::start_with_config(config_with_services(&pg, &stub)).await;

    // A tenant admin (not a super-admin) is refused.
    let (status, body) = running
        .http(
            reqwest::Method::GET,
            "/api/platform/tenants",
            Some("dev:bob"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 403, "a non-super-admin must be refused: {body}");

    // The super-admin sees the seeded tenant.
    let (status, body) = running
        .http(
            reqwest::Method::GET,
            "/api/platform/tenants",
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body.as_array().expect("array").len(), 1);
    assert_eq!(body[0]["slug"], "dev-a");
    assert_eq!(body[0]["admins"], 1);

    // Create: the local id is PROVISIONING's, not one we minted.
    let (status, body) = running
        .http(
            reqwest::Method::POST,
            "/api/platform/tenants",
            Some("dev:alice"),
            &[],
            Some(json!({
                "name": "Acme",
                "slug": "acme",
                "admin_email": "owner@acme.example",
                "admin_name": "Acme Owner"
            })),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        body["provisioning"]["tenant_id"],
        provisioning_tenant_id.to_string()
    );
    assert_eq!(body["tenant"]["id"], provisioning_tenant_id.to_string());
    assert_eq!(body["tenant"]["licence"]["status"], "active");
    assert_eq!(body["email_sent"], true);

    // The rows exist and the membership is admin.
    let tenant = conveyor_domain::TenantId::from_uuid(provisioning_tenant_id).expect("tenant");
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope");
    let (members, admins): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE role = 'admin') FROM membership WHERE tenant_id = $1",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("count");
    tx.commit().await.expect("commit");
    assert_eq!((members, admins), (1, 1));

    let email: String =
        sqlx::query_scalar("SELECT email FROM app_user WHERE lower(email) = 'owner@acme.example'")
            .fetch_one(&pg.owner)
            .await
            .expect("app_user");
    assert_eq!(email, "owner@acme.example");

    // The same slug again is a conflict before any external call.
    let (status, body) = running
        .http(
            reqwest::Method::POST,
            "/api/platform/tenants",
            Some("dev:alice"),
            &[],
            Some(json!({"name": "Acme 2", "slug": "acme", "admin_email": "x@y.example"})),
        )
        .await;
    assert_eq!(status, 409, "{body}");

    // The recovery invite writes a pending admin invite.
    let (status, body) = running
        .http(
            reqwest::Method::POST,
            &format!("/api/platform/tenants/{}/invite-admin", seeded.tenant_id),
            Some("dev:alice"),
            &[],
            Some(json!({"email": "new-admin@acme.example", "name": "New Admin"})),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let mut tx = conveyor_store::begin_scoped(
        &pg.owner,
        conveyor_domain::TenantId::from_uuid(seeded.tenant_id).expect("id"),
    )
    .await
    .expect("scope");
    let (role, status_text): (String, String) = sqlx::query_as(
        "SELECT role, status FROM invite WHERE tenant_id = $1 AND lower(email) = 'new-admin@acme.example'",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("invite");
    tx.commit().await.expect("commit");
    assert_eq!((role.as_str(), status_text.as_str()), ("admin", "pending"));
}

#[tokio::test]
async fn a_provisioning_failure_writes_no_tenant_row() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    make_super_admin(&pg, "alice").await;

    let (stub, _tenant) = start_stub(true).await;
    let running = support::Running::start_with_config(config_with_services(&pg, &stub)).await;

    let (status, body) = running
        .http(
            reqwest::Method::POST,
            "/api/platform/tenants",
            Some("dev:alice"),
            &[],
            Some(json!({"name": "Broken", "slug": "broken", "admin_email": "b@broken.example"})),
        )
        .await;
    assert_eq!(status, 502, "{body}");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tenant WHERE slug = 'broken'")
        .fetch_one(&pg.owner)
        .await
        .expect("count");
    assert_eq!(count, 0, "a failed provision must leave no local row");
}

#[tokio::test]
async fn billing_is_readable_by_the_tenant_admin_and_refused_to_a_member() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-a", "bob", "Bob").await;
    // `support::seed` makes every login an admin; demote bob so the role check is the subject.
    let tenant = conveyor_domain::TenantId::from_uuid(seeded.tenant_id).expect("tenant");
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope");
    sqlx::query(
        "UPDATE membership SET role = 'member'
          WHERE user_id = (SELECT id FROM app_user WHERE oidc_subject = 'dev:bob')",
    )
    .execute(&mut *tx)
    .await
    .expect("demote bob");
    tx.commit().await.expect("commit");

    let (stub, _tenant) = start_stub(false).await;
    let running = support::Running::start_with_config(config_with_services(&pg, &stub)).await;

    // The seeded admin is an ADMIN of dev-a (support::seed grants admin to the first login).
    let (status, body) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/tenants/{}/billing", seeded.tenant_id),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["billing"]["available"], true);
    assert_eq!(body["costs"]["available"], true);
    assert_eq!(body["actors"]["available"], true);

    let (status, body) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/tenants/{}/billing", seeded.tenant_id),
            Some("dev:bob"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 403, "a member must not read spend: {body}");
}
