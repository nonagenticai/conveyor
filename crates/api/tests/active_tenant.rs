//! S02 amendment — one identity, many memberships, one active tenant per request.
//!
//! The owner's ruling (2026-11-09): a person is tenant-admin in their own tenants, member or
//! absent in another's, and **the absent row is the non-reputation proof**. So the tenant is
//! chosen by the caller (`X-Active-Tenant`), resolved only against the caller's own membership
//! rows, and never guessed:
//!
//! * no header + more than one membership: `/api/me` answers `200` with the membership list and
//!   `active_tenant: null` — it is the switcher's bootstrap, and it names no tenant, so no tenant
//!   is guessed; every tenant-SCOPED endpoint answers `400 active_tenant_required`;
//! * a header naming a tenant the caller has no row in → `403 active_tenant_not_permitted`
//!   (indistinguishable from a tenant that does not exist, because only the caller's rows are read);
//! * a malformed header (empty, repeated) → `400 active_tenant_invalid`;
//! * a single-membership caller keeps working with no header.
//!
//! The second test is the one that matters for authorisation: the **role** comes from the
//! selected membership, so the same person is admin in tenant A and refused an admin action in
//! tenant B within the same server.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_store::begin_scoped;
use reqwest::Method;
use serde_json::{Value, json};

const HEADER: &str = "x-active-tenant";

/// alice: **owner** in `dev-a` (seeded), **member** in `dev-b` (added here); `dev-c` exists and
/// alice has no row in it. bob: owner of `dev-b` only, the single-membership control.
struct Fixture {
    /// Held so the harness's pools outlive the assertions; the served app has its own.
    #[allow(dead_code)]
    pg: support::Pg,
    running: support::Running,
    alice: support::Seeded,
    /// The `dev-b` seed: alice is a member here and bob an owner.
    b: support::Seeded,
    c: support::Seeded,
}

async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let c = support::seed(&pg, "dev-c", "carol", "Carol").await;

    // alice also belongs to dev-b, as a member — the second row that makes the header necessary.
    let mut tx = begin_scoped(&pg.owner, bob.tenant)
        .await
        .expect("scoped tx");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET role = 'member'",
    )
    .bind(bob.tenant_id)
    .bind(alice.user_id)
    .execute(&mut *tx)
    .await
    .expect("insert alice's dev-b membership");
    tx.commit().await.expect("commit");

    let running = support::Running::start(&pg).await;
    Fixture {
        pg,
        running,
        alice,
        b: bob,
        c,
    }
}

async fn me(fixture: &Fixture, login: &str, active: Option<&str>) -> (u16, Value) {
    let token = format!("dev:{login}");
    let headers: Vec<(&str, &str)> = active
        .map(|tenant| vec![(HEADER, tenant)])
        .unwrap_or_default();
    fixture
        .running
        .http(Method::GET, "/api/me", Some(&token), &headers, None)
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_person_with_two_memberships_names_the_tenant_and_gets_that_rows_role() {
    let fixture = fixture().await;
    let a_id = fixture.alice.tenant_id.to_string();
    let b_id = fixture.b.tenant_id.to_string();
    let c_id = fixture.c.tenant_id.to_string();

    // No header with two memberships: `/api/me` is the switcher bootstrap — it REPORTS both
    // memberships and selects no tenant, which is not a guess.
    let (status, body) = me(&fixture, "alice", None).await;
    assert_eq!(status, 200, "/api/me with no tenant: {body}");
    assert_eq!(body["active_tenant"], Value::Null, "{body}");
    assert_eq!(
        body["memberships"].as_array().map(Vec::len),
        Some(2),
        "the switcher needs both memberships: {body}"
    );

    // ... while an endpoint that ACTS in a tenant refuses: no header is a 400, never a guess.
    let (status, body) = fixture
        .running
        .http(
            Method::POST,
            "/api/actions/departments.create/invocations",
            Some("dev:alice"),
            &[],
            Some(json!({"input": {"name": "Guessed"}})),
        )
        .await;
    assert_eq!(status, 400, "no header on an action: {body}");
    assert_eq!(body["type"], "active_tenant_required", "{body}");

    // The header by id, then by slug, selects that row — and the role is that row's role.
    let (status, body) = me(&fixture, "alice", Some(&a_id)).await;
    assert_eq!(status, 200, "tenant A: {body}");
    assert_eq!(body["active_tenant"]["tenant_id"], a_id);
    assert_eq!(body["active_tenant"]["role"], "admin");
    assert_eq!(
        body["memberships"].as_array().map(Vec::len),
        Some(2),
        "both memberships are listed: {body}"
    );

    let (status, body) = me(&fixture, "alice", Some("dev-b")).await;
    assert_eq!(status, 200, "tenant B by slug: {body}");
    assert_eq!(body["active_tenant"]["tenant_id"], b_id);
    assert_eq!(body["active_tenant"]["role"], "member");

    // A tenant alice has no row in: refused, and the refusal does not say whether it exists.
    let (status, body) = me(&fixture, "alice", Some(&c_id)).await;
    assert_eq!(status, 403, "tenant C: {body}");
    assert_eq!(body["type"], "active_tenant_not_permitted", "{body}");

    // An empty header is malformed, not "no header".
    let (status, body) = me(&fixture, "alice", Some("")).await;
    assert_eq!(status, 400, "empty header: {body}");
    assert_eq!(body["type"], "active_tenant_invalid", "{body}");

    // The single-membership control is unchanged: bob needs no header.
    let (status, body) = me(&fixture, "bob", None).await;
    assert_eq!(status, 200, "single membership: {body}");
    assert_eq!(body["active_tenant"]["tenant_id"], b_id);

    // ... and naming his own tenant is still allowed.
    let (status, _) = me(&fixture, "bob", Some("dev-b")).await;
    assert_eq!(status, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_selected_membership_is_what_authorises_an_action() {
    let fixture = fixture().await;
    let a_id = fixture.alice.tenant_id.to_string();
    let b_id = fixture.b.tenant_id.to_string();

    // departments.create is `departments:write` — admin only. alice is admin in A ...
    let (status, body) = fixture
        .running
        .http(
            Method::POST,
            "/api/actions/departments.create/invocations",
            Some("dev:alice"),
            &[(HEADER, a_id.as_str())],
            Some(json!({"input": {"name": "Operations"}})),
        )
        .await;
    assert_eq!(status, 201, "owner in tenant A: {body}");

    // ... and a member in B, so the same action is refused there in the same server.
    let (status, body) = fixture
        .running
        .http(
            Method::POST,
            "/api/actions/departments.create/invocations",
            Some("dev:alice"),
            &[(HEADER, b_id.as_str())],
            Some(json!({"input": {"name": "Operations"}})),
        )
        .await;
    assert_eq!(status, 403, "member in tenant B: {body}");
    assert_eq!(body["type"], "action-forbidden", "{body}");
}
