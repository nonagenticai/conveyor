//! v1-p6-5: `card.lease_sweep` is the only place that sees a claim expire unanswered, and the
//! §6.2 stale-lease input is a flow interruption, not a failed run. A real `conveyor worker`
//! child runs the tick (the placement rule: a test that must drive a job handler lives where
//! `CARGO_BIN_EXE_conveyor` exists).

mod common;

use std::time::Duration;

use uuid::Uuid;

#[derive(Clone, Debug)]
struct Fixture {
    tenant_id: Uuid,
    card: Uuid,
}

/// Seed a tenant, a project, a board with one active lane, a card, and the coach substrate the
/// interruption needs. The card carries an already-expired claim; the sweep is what clears it.
async fn seed(db: &common::TestDb) -> Fixture {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('lease', 'Lease') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, display_name) VALUES ('dev:leases', 'Leaseholder')
         RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed app_user");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, project_type_code)
         VALUES ($1, 'lease', 'Lease', 'delivery') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed project");

    // `coach.interrupt` creates its card and starts the coach process in the same transaction.
    common::coach_fixture::seed_coach_handle_error(db, tenant_id, user).await;

    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name) VALUES ($1, $2, 'Board') RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a board");
    let lane: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 1, 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a lane");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label)
         VALUES ($1, 'feature', 'Feature')
         ON CONFLICT (tenant_id, code) DO UPDATE SET label = excluded.label
         RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a card type");
    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
         VALUES ($1, $2, $3, $4, 'Card', 1) RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .bind(lane)
    .bind(card_type)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a card");

    // The claim is a pair: `card_lease_pair` refuses an expiry without a token.
    sqlx::query(
        "UPDATE card
            SET assignee_id = $3, lease_token = uuidv7(),
                lease_expires_at = now() - interval '1 minute',
                claim_fence = claim_fence + 1
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(card)
    .bind(user)
    .execute(&db.super_pool)
    .await
    .expect("expire the card's lease");

    Fixture { tenant_id, card }
}

async fn enqueue_sweep(db: &common::TestDb, fixture: &Fixture, dedupe_key: &str) {
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload)
         VALUES ($1, 'card.lease_sweep', $2, '{}'::jsonb)",
    )
    .bind(fixture.tenant_id)
    .bind(dedupe_key)
    .execute(&db.super_pool)
    .await
    .expect("enqueue card.lease_sweep");
}

async fn job_status(db: &common::TestDb, dedupe_key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM job WHERE kind = 'card.lease_sweep' AND dedupe_key = $1")
        .bind(dedupe_key)
        .fetch_optional(&db.super_pool)
        .await
        .expect("read the job")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stale_lease_opens_one_interruption() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    enqueue_sweep(&db, &fixture, "lease-1").await;
    let _worker = common::WorkerChild::spawn(env!("CARGO_BIN_EXE_conveyor"), &db);

    common::wait_for(
        "the lease sweep to finish",
        Duration::from_secs(15),
        || async { job_status(&db, "lease-1").await.as_deref() == Some("succeeded") },
    )
    .await;

    let rows: Vec<(String, Option<Uuid>, String, Uuid)> = sqlx::query_as(
        "SELECT trigger_kind, failed_run_id, signature, coach_card_id
           FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.card)
    .fetch_all(&db.super_pool)
    .await
    .expect("read the interruptions");
    assert_eq!(
        rows.len(),
        1,
        "one cleared lease, one interruption: {rows:?}"
    );
    assert_eq!(rows[0].0, "stale_lease");
    assert_eq!(rows[0].1, None, "a flow input carries no failed run");
    assert_eq!(rows[0].2, format!("stale_lease|card:{}", fixture.card));

    let coach_card = rows[0].3;
    let (code, name): (String, String) = sqlx::query_as(
        "SELECT ct.code, c.name FROM card c
           JOIN card_type ct ON ct.tenant_id = c.tenant_id AND ct.id = c.card_type_id
          WHERE c.tenant_id = $1 AND c.id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(coach_card)
    .fetch_one(&db.super_pool)
    .await
    .expect("read the coach card");
    assert_eq!(code, "coach");
    assert_eq!(name, "Coach: Card");

    let (assignee, expires, token): (Option<Uuid>, bool, bool) = sqlx::query_as(
        "SELECT assignee_id, lease_expires_at IS NOT NULL, lease_token IS NOT NULL
           FROM card WHERE tenant_id = $1 AND id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.card)
    .fetch_one(&db.super_pool)
    .await
    .expect("read the swept card");
    assert_eq!(assignee, None, "the sweep clears the holder");
    assert!(!expires, "the sweep clears the expiry");
    assert!(!token, "the sweep clears the token");

    // A second sweep has nothing expired to clear, so the card cannot be coached twice.
    enqueue_sweep(&db, &fixture, "lease-2").await;
    common::wait_for(
        "the second lease sweep to finish",
        Duration::from_secs(15),
        || async { job_status(&db, "lease-2").await.as_deref() == Some("succeeded") },
    )
    .await;
    let after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.card)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the interruptions");
    assert_eq!(after, 1, "a cleared lease cannot be swept twice");
}
