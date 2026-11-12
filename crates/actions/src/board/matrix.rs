#![forbid(unsafe_code)]
//! The project-type matrix: one cached read per `(tenant, project_type_code)`, with a TTL.
//!
//! The 280 generated Planka flows are one lookup: `catalogue_entry(kind='project_type',
//! code=<project.project_type_code>).meta.matrix`, keyed by card-type code and then by the
//! list kind the card is leaving. `domain::board::may_move` is the only legality function;
//! this module only resolves its input and maps a denial to the one wire code.
//!
//! ## Why the cache has a TTL
//!
//! A warm process that cached a matrix forever would keep serving a **changed** or
//! **deleted** row: a catalogue edit (or a retired project type) would not take effect until
//! the process restarted, and the estate has no restart to rely on. The entry therefore
//! carries the instant it was read and is re-read once it is older than [`ttl`] —
//! [`DEFAULT_MATRIX_TTL`] is 60 s, and `CONVEYOR_MATRIX_TTL_SECS` (or [`set_ttl`]) overrides
//! it. Tests set the TTL to zero, which makes every lookup a fresh read.
//!
//! The cache still never stores a MISS: an absent row is a denial now, and the same process
//! may legitimately see the row appear (a seed, a catalogue import) on its next read.
//!
//! The map is keyed by tenant, so one tenant's catalogue can never answer another's lookup.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use conveyor_domain::{
    Card, CardType, CardTypeRules, CatalogueEntry, CatalogueKind, Code, ListKind, Move, MoveError,
    Version, may_move,
};
use conveyor_store as store;
use conveyor_store::TenantId;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::ActionError;

/// The one refusal code every matrix denial maps to (S05 §5).
pub const MOVE_REFUSED: &str = "card-move-not-allowed";

/// How long a cached matrix is trusted when nothing overrides it.
pub const DEFAULT_MATRIX_TTL: Duration = Duration::from_secs(60);

/// The environment variable that overrides [`DEFAULT_MATRIX_TTL`] (whole seconds).
pub const MATRIX_TTL_ENV: &str = "CONVEYOR_MATRIX_TTL_SECS";

/// One cached matrix and the instant it was read.
struct CachedMatrix {
    matrix: Value,
    loaded_at: Instant,
}

/// `(tenant, project_type_code)` → the row's `meta.matrix`, with its read time.
static MATRICES: LazyLock<Mutex<HashMap<(Uuid, String), CachedMatrix>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The configured TTL. `None` means "use the environment/default".
static MATRIX_TTL: LazyLock<Mutex<Option<Duration>>> = LazyLock::new(|| Mutex::new(None));

fn default_ttl() -> Duration {
    static DEFAULT: OnceLock<Duration> = OnceLock::new();
    *DEFAULT.get_or_init(|| {
        std::env::var(MATRIX_TTL_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_MATRIX_TTL)
    })
}

/// The effective TTL: an explicit [`set_ttl`] wins, then `CONVEYOR_MATRIX_TTL_SECS`, then
/// [`DEFAULT_MATRIX_TTL`].
pub fn ttl() -> Duration {
    MATRIX_TTL
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .unwrap_or_else(default_ttl)
}

/// Override the cache TTL for this process. Zero means "never serve a cached matrix".
pub fn set_ttl(ttl: Duration) {
    *MATRIX_TTL
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(ttl);
}

/// Drop one tenant's entries. A fixture that writes a catalogue row after an earlier denial
/// in the same process calls this for its own tenant.
///
/// **Prefer this to [`clear_cache`]** anywhere more than one test runs in a process: the map
/// is keyed by tenant, so dropping one tenant's entries cannot disturb another test's warm
/// cache. The whole-map clear is what made
/// `the_matrix_cache_expires_so_a_changed_or_deleted_row_takes_effect` flaky — every
/// concurrent `fixture()` in the same binary emptied the entry between that test's warm read
/// and its "still cached" assertion (measured 2026-11-08: the test passes alone and 5/5 alone,
/// and failed 2/2 under `cargo test --workspace`).
pub fn clear_cache_for(tenant: TenantId) {
    MATRICES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .retain(|(cached_tenant, _), _| *cached_tenant != tenant.as_uuid());
}

/// Drop the whole cache. Only safe when nothing else in the process is mid-assertion about a
/// cached value; a per-test fixture should use [`clear_cache_for`].
pub fn clear_cache() {
    MATRICES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
}

/// The refusal: absent row, absent card type, absent from-kind, or a kind not listed.
pub fn refused(detail: impl Into<String>) -> ActionError {
    ActionError::new(409, MOVE_REFUSED, detail)
}

/// Load (or reuse, while it is younger than [`ttl`]) the project type's matrix.
pub async fn matrix_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_type_code: &str,
) -> Result<Value, ActionError> {
    let key = (tenant.as_uuid(), project_type_code.to_owned());
    let ttl = ttl();
    if !ttl.is_zero() {
        let cached = MATRICES
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(&key)
            .filter(|entry| entry.loaded_at.elapsed() < ttl)
            .map(|entry| entry.matrix.clone());
        if let Some(matrix) = cached {
            return Ok(matrix);
        }
    }
    let Some(matrix) = store::board::project_type_matrix_tx(&mut *conn, tenant, project_type_code)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read project type matrix");
            ActionError::internal()
        })?
    else {
        // A row that vanished (or was deactivated) must not leave its entry behind: the next
        // read is what decides, and it must not find a stale copy.
        MATRICES
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&key);
        return Err(refused(format!(
            "no live project_type catalogue row has code {project_type_code:?}"
        )));
    };
    MATRICES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(
            key,
            CachedMatrix {
                matrix: matrix.clone(),
                loaded_at: Instant::now(),
            },
        );
    Ok(matrix)
}

/// Resolve the matrix for one card type.
///
/// Every failure is the same `409 card-move-not-allowed`: the caller cannot distinguish "no
/// row" from "no entry for your type" and must not be able to, because both mean the move is
/// not legal (S3-D3).
pub async fn rules_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_type: &CardType,
    project_type_code: &str,
) -> Result<CardTypeRules, ActionError> {
    let matrix = matrix_tx(&mut *conn, tenant, project_type_code).await?;
    let code = Code::parse(project_type_code)
        .map_err(|error| refused(format!("the project's type code is unusable: {error}")))?;
    let entry = CatalogueEntry {
        // The matrix is the only field this constructor reads; the row id is not used.
        id: Uuid::nil(),
        kind: CatalogueKind::ProjectType,
        code,
        label: String::new(),
        meta: json!({ "matrix": matrix }),
        sort: 0,
        is_active: true,
        version: Version::INITIAL,
    };
    CardTypeRules::from_entry(&entry, card_type).map_err(|error| refused(error.to_string()))
}

/// `may_move`, with a denial mapped to the one wire code.
pub fn ensure_move(
    card: &Card,
    from: ListKind,
    to: ListKind,
    rules: &CardTypeRules,
) -> Result<Move, ActionError> {
    may_move(card, from, to, rules).map_err(|error| match error {
        MoveError::UnknownCardType => {
            refused("the card's type has no rules in this project type's matrix")
        }
        MoveError::Archived => refused("an archived card must be restored before it can move"),
        MoveError::NoEntry(kind) => refused(format!("the matrix has no entry for {kind}")),
        MoveError::NotAllowed { from, to } => {
            refused(format!("a card cannot move from {from} to {to}"))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_ttl_defaults_to_sixty_seconds_and_an_override_wins() {
        // No override was set in this test binary, so the default (or the environment) is in
        // force. The estate sets no `CONVEYOR_MATRIX_TTL_SECS`, so this is the default.
        assert_eq!(default_ttl(), DEFAULT_MATRIX_TTL);
        assert_eq!(DEFAULT_MATRIX_TTL, Duration::from_secs(60));
        set_ttl(Duration::ZERO);
        assert_eq!(ttl(), Duration::ZERO);
        set_ttl(DEFAULT_MATRIX_TTL);
        assert_eq!(ttl(), DEFAULT_MATRIX_TTL);
    }
}
