//! The cadence tick — T10 (#82, SPEC_V2 §8.2).
//!
//! One pass materialises every `programme_cadence(kind='pi')` row still in `planned`: the
//! standing backlog becomes draft cards ordered by WSJF inside the active lane's WIP slots. The
//! materialisation moves the cadence to `materialised` in the same transaction, so a second pass
//! creates nothing — which is what makes the tick idempotent rather than merely repeatable.
//!
//! A failure on one cadence rolls that cadence back and is logged; the pass continues, so one
//! misconfigured cadence cannot strand the tenant's others.

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::spine::{self, PlanStoreError};
use sqlx::PgPool;
use uuid::Uuid;

/// Materialise every planned PI cadence in one tenant; returns the cards created, in order.
pub async fn tick(pool: &PgPool, tenant: TenantId) -> Result<Vec<Uuid>, PlanStoreError> {
    // The listing runs in a tenant-scoped transaction: `programme_cadence` is FORCE-RLS, and a
    // bare pool read has no `app_current_tenant` to match.
    let mut listing = begin_scoped(pool, tenant).await?;
    let cadences: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM programme_cadence
          WHERE tenant_id = $1 AND kind = 'pi' AND state = 'planned'
          ORDER BY created_at, id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *listing)
    .await?;
    listing.commit().await?;
    let mut created = Vec::new();
    for cadence in cadences {
        let mut tx = begin_scoped(pool, tenant).await?;
        match spine::materialise_pi_tx(&mut tx, tenant, cadence).await {
            Ok(materialised) => {
                created.extend(materialised.cards);
                tx.commit().await?;
            }
            Err(error) => {
                tx.rollback().await?;
                tracing::warn!(%error, %cadence, "cadence materialisation failed");
            }
        }
    }
    // A demo cadence credits every completed deliverable that exhibits its evidence; a retro
    // cadence opens its one coach card. Both move to `materialised` in the same write, so a
    // second pass repeats neither.
    let due: Vec<(Uuid, String)> = {
        let mut listing = begin_scoped(pool, tenant).await?;
        let rows = sqlx::query_as(
            "SELECT id, kind FROM programme_cadence
              WHERE tenant_id = $1 AND kind IN ('demo', 'retro') AND state = 'planned'
              ORDER BY created_at, id",
        )
        .bind(tenant.as_uuid())
        .fetch_all(&mut *listing)
        .await?;
        listing.commit().await?;
        rows
    };
    for (cadence, kind) in due {
        let mut tx = begin_scoped(pool, tenant).await?;
        let outcome = match kind.as_str() {
            "demo" => demo(&mut tx, tenant, cadence).await,
            _ => spine::open_retro_tx(&mut tx, tenant, cadence)
                .await
                .map(|_| ()),
        };
        match outcome {
            Ok(()) => {
                sqlx::query(
                    "UPDATE programme_cadence
                        SET state = 'materialised', version = version + 1, updated_at = now()
                      WHERE tenant_id = $1 AND id = $2",
                )
                .bind(tenant.as_uuid())
                .bind(cadence)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
            }
            Err(error) => {
                tx.rollback().await?;
                tracing::warn!(%error, %cadence, %kind, "cadence event failed");
            }
        }
    }
    Ok(created)
}

/// Credit every completed deliverable of the programme that exhibits evidence. A deliverable
/// without evidence is not credited — that is the rule, not a failure of the pass.
async fn demo(
    conn: &mut sqlx::PgConnection,
    tenant: TenantId,
    cadence: Uuid,
) -> Result<(), PlanStoreError> {
    let cadence_row = spine::get_cadence_tx(&mut *conn, tenant, cadence)
        .await?
        .ok_or(PlanStoreError::NotFound {
            what: "cadence row",
        })?;
    let deliverables: Vec<Uuid> = sqlx::query_scalar(
        "SELECT w.id
           FROM wbs_node w
           JOIN project p ON p.tenant_id = w.tenant_id AND p.id = w.project_id
          WHERE w.tenant_id = $1 AND p.programme_id = $2
            AND w.kind = 'deliverable' AND w.status = 'accepted'
            AND jsonb_array_length(w.evidence) > 0
          ORDER BY w.id",
    )
    .bind(tenant.as_uuid())
    .bind(cadence_row.programme_id)
    .fetch_all(&mut *conn)
    .await?;
    for deliverable in deliverables {
        spine::credit_demo_tx(&mut *conn, tenant, cadence, deliverable).await?;
    }
    Ok(())
}
