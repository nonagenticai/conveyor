//! Least-privilege runtime grants (`scripts/grants-runtime.sql`), proven on a migrated
//! database as `conveyor_runtime`.
//!
//! `has_table_privilege` reports the effective privilege for the role (it includes anything
//! granted through `PUBLIC`), which is exactly the question: can the request path rewrite a
//! fact or delete the isolation anchor?
//!
//! `cargo test -p conveyor-store --test privileges -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

#[tokio::test]
async fn the_runtime_role_cannot_rewrite_facts_or_delete_the_anchor() {
    let db = support::TestDb::fresh().await;

    let rows: Vec<(String, bool, bool, bool, bool, bool)> = sqlx::query_as(
        "SELECT t,
                has_table_privilege('conveyor_runtime', t, 'SELECT') AS can_select,
                has_table_privilege('conveyor_runtime', t, 'INSERT') AS can_insert,
                has_table_privilege('conveyor_runtime', t, 'UPDATE') AS can_update,
                has_table_privilege('conveyor_runtime', t, 'DELETE') AS can_delete,
                has_table_privilege('conveyor_runtime', t, 'TRUNCATE') AS can_truncate
           FROM unnest(ARRAY['audit_event', 'event', 'action_invocation', 'tenant',
                             'app_user', 'job']) AS t
          ORDER BY t",
    )
    .fetch_all(&db.super_pool)
    .await
    .expect("read the effective privileges");

    let mut seen = 0;
    for (table, can_select, can_insert, can_update, can_delete, can_truncate) in &rows {
        seen += 1;
        println!(
            "{table}: select={can_select} insert={can_insert} update={can_update} \
             delete={can_delete} truncate={can_truncate}"
        );
        assert!(!can_truncate, "{table}: TRUNCATE was never granted");
        assert!(can_select, "{table}: the runtime role reads it");
        assert!(can_insert, "{table}: the runtime role writes new rows");
        match table.as_str() {
            // Append-only facts: written once, read forever.
            "audit_event" | "event" | "action_invocation" => {
                assert!(!can_update, "{table} must not be rewritable");
            }
            _ => {}
        }
        match table.as_str() {
            // `event.prune` and `job.prune` delete by retention; everything else keeps DELETE
            // only where no path uses it (the assertion below is the contract).
            "event" | "job" => assert!(can_delete, "{table}.prune needs DELETE"),
            "tenant" | "app_user" => {
                assert!(
                    !can_delete,
                    "{table} must not be deletable by the runtime role"
                )
            }
            _ => {}
        }
    }
    assert_eq!(seen, 6, "every probed table answered");
}
