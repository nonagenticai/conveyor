//! Positions: exact midpoints, appends, and the board-scoped advisory lock.
//!
//! `position` is `numeric` in every table that carries one, never a float (data-arch §2.3,
//! trap 2). A midpoint insert is `(a+b)/2` on [`Decimal`] — exact, so a thousand drags
//! cannot drift; an append is `max+1024`. A float would degrade after ~50 halvings and need
//! a renumber job nobody writes.
//!
//! Ordering is `(position, id)`, never `position` alone: two rows may share a position (a
//! vanished neighbour, a corrupted row), and the uuid is the deterministic tiebreak the
//! keyset cursor also uses.
//!
//! Concurrent inserts and moves on one board take a transaction-scoped advisory lock keyed
//! by `(tenant, board)`. The lock is taken BEFORE the neighbour read, so two clients cannot
//! both read the same "next" row and compute the same midpoint. The key is a hash of the
//! tenant and board ids, so it is stable across pooler connections and never blocks another
//! board (a hash collision only serialises two boards momentarily).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use conveyor_domain::TenantId;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

/// The append step: `max + 1024`. Wide enough that a midpoint insert between two appends
/// has room for ~10 halvings before the decimal reaches its scale.
pub const APPEND_STEP: i64 = 1024;

/// The first position on an empty list: `0 + APPEND_STEP`.
pub fn first() -> Decimal {
    Decimal::from(APPEND_STEP)
}

/// Append after the current maximum (`None` on an empty lane).
pub fn append_after(max: Option<Decimal>) -> Decimal {
    max.map_or_else(first, |max| max + Decimal::from(APPEND_STEP))
}

/// The position between two neighbours, either of which may be absent.
///
/// * both present → the exact midpoint;
/// * left only → `left + 1024` (the tail);
/// * right only → `right - 1024` (the head; below zero is legal — `position` is signed);
/// * neither → the first position.
///
/// Inverted neighbours (`right <= left`) can only come from a corrupted row; a midpoint
/// would then not advance, so the tail step is used instead.
pub fn between(left: Option<Decimal>, right: Option<Decimal>) -> Decimal {
    match (left, right) {
        (Some(left), Some(right)) => {
            if right <= left {
                append_after(Some(left))
            } else {
                (left + right) / Decimal::from(2)
            }
        }
        (Some(left), None) => append_after(Some(left)),
        (None, Some(right)) => right - Decimal::from(APPEND_STEP),
        (None, None) => first(),
    }
}

/// Take the board's transaction-scoped advisory lock.
///
/// Called before reading neighbours on any insert or move; released with the transaction.
pub async fn lock_board_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<(), sqlx::Error> {
    let key = format!("{}:{}", tenant.as_uuid(), board_id);
    // `execute`, not `query_scalar`: the function returns `void`, which has no Rust decoder,
    // and the row is not read either way.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The position for an insert at the end of a card lane.
pub async fn next_card_position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
    list_id: Uuid,
) -> Result<Decimal, sqlx::Error> {
    let max: Option<Decimal> = sqlx::query_scalar(
        "SELECT max(position) FROM card
          WHERE tenant_id = $1 AND board_id = $2 AND list_id = $3 AND archived_at IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(list_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(append_after(max))
}

/// The position for an insert at the end of a board's lanes.
pub async fn next_list_position_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
) -> Result<Decimal, sqlx::Error> {
    let max: Option<Decimal> =
        sqlx::query_scalar("SELECT max(position) FROM list WHERE tenant_id = $1 AND board_id = $2")
            .bind(tenant.as_uuid())
            .bind(board_id)
            .fetch_one(&mut *conn)
            .await?;
    Ok(append_after(max))
}

/// The position that places a card immediately after `after_id` in its lane.
///
/// `Ok(None)` means `after_id` is not a live card of this lane — the caller answers a 404
/// rather than appending, because "after a card that does not exist" is a client mistake.
pub async fn position_after_card_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
    list_id: Uuid,
    after_id: Uuid,
) -> Result<Option<Decimal>, sqlx::Error> {
    let Some(anchor): Option<Decimal> = sqlx::query_scalar(
        "SELECT position FROM card
          WHERE tenant_id = $1 AND board_id = $2 AND list_id = $3 AND id = $4
            AND archived_at IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(list_id)
    .bind(after_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let next: Option<Decimal> = sqlx::query_scalar(
        "SELECT position FROM card
          WHERE tenant_id = $1 AND board_id = $2 AND list_id = $3 AND archived_at IS NULL
            AND (position, id) > ($4, $5)
          ORDER BY position, id
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(list_id)
    .bind(anchor)
    .bind(after_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Some(between(Some(anchor), next)))
}

/// The position that places a lane immediately after `after_id` on its board.
pub async fn position_after_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    board_id: Uuid,
    after_id: Uuid,
) -> Result<Option<Decimal>, sqlx::Error> {
    let Some(anchor): Option<Decimal> = sqlx::query_scalar(
        "SELECT position FROM list WHERE tenant_id = $1 AND board_id = $2 AND id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(after_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let next: Option<Decimal> = sqlx::query_scalar(
        "SELECT position FROM list
          WHERE tenant_id = $1 AND board_id = $2 AND (position, id) > ($3, $4)
          ORDER BY position, id
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(board_id)
    .bind(anchor)
    .bind(after_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Some(between(Some(anchor), next)))
}

/// A card page cursor: `base64url("<position>|<uuid>")`, unpadded.
///
/// Opaque by contract (CONTRACT §5): a client must round-trip it, never parse it. It is the
/// `(sort_key, id)` pair of the last row of the previous page.
pub fn encode_cursor(position: Decimal, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(format!("{position}|{id}"))
}

/// Decode a cursor produced by [`encode_cursor`]; anything else is `None`.
pub fn decode_cursor(raw: &str) -> Option<(Decimal, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(raw).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (position, id) = text.split_once('|')?;
    let position = Decimal::from_str_exact(position).ok()?;
    let id = Uuid::parse_str(id).ok()?;
    Some((position, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(raw: &str) -> Decimal {
        Decimal::from_str_exact(raw).expect("decimal")
    }

    #[test]
    fn appends_step_by_1024_from_zero() {
        assert_eq!(first(), dec("1024"));
        assert_eq!(append_after(None), dec("1024"));
        assert_eq!(append_after(Some(dec("1024"))), dec("2048"));
        assert_eq!(append_after(Some(dec("0"))), dec("1024"));
    }

    #[test]
    fn midpoints_are_exact_and_repeatable() {
        assert_eq!(between(Some(dec("1024")), Some(dec("2048"))), dec("1536"));
        assert_eq!(between(Some(dec("1024")), Some(dec("1025"))), dec("1024.5"));
        // Ten successive halvings stay exact (a float would carry 0.0009765625000000001…).
        let mut low = dec("0");
        let high = dec("1");
        for _ in 0..10 {
            low = between(Some(low), Some(high));
        }
        assert_eq!(low, dec("0.9990234375"));

        // And from the worked positions: ten drags into the head of a 1024 gap land on
        // exactly 2047, not 2047.000000001.
        let mut head = dec("1024");
        let tail = dec("2048");
        for _ in 0..10 {
            head = between(Some(head), Some(tail));
        }
        assert_eq!(head, dec("2047"));
    }

    #[test]
    fn head_and_tail_have_a_step_either_side() {
        assert_eq!(between(Some(dec("1024")), None), dec("2048"));
        assert_eq!(between(None, Some(dec("1024"))), dec("0"));
        assert_eq!(between(None, None), dec("1024"));
    }

    #[test]
    fn inverted_neighbours_fall_back_to_the_tail_step() {
        assert_eq!(between(Some(dec("2048")), Some(dec("1024"))), dec("3072"));
        assert_eq!(between(Some(dec("1024")), Some(dec("1024"))), dec("2048"));
    }

    #[test]
    fn a_cursor_round_trips_and_rejects_anything_else() {
        let id = Uuid::now_v7();
        let cursor = encode_cursor(dec("1536.5"), id);
        assert_eq!(decode_cursor(&cursor), Some((dec("1536.5"), id)));
        assert_eq!(decode_cursor("not-a-cursor"), None);
        assert_eq!(decode_cursor(""), None);
        let mut tampered = cursor.clone();
        tampered.push('x');
        assert_eq!(decode_cursor(&tampered), None);
    }
}
