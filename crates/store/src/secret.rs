//! The secret store: sealed values, and the SQL around them (S09 §5.4).
//!
//! **A secret is never returned in plaintext by an action.** The registry has no `secrets.reveal`; the only
//! way a value comes back out is [`open_tx`], which is called by the process that needs to *use* it (an
//! outbound webhook signature, a repository credential) — never by a request handler that would echo it.
//!
//! # The envelope
//!
//! ```text
//! KEK  = base64(CONVEYOR_SECRET_KEK)        32 bytes, from the pod environment, never the database
//! kek_t = HKDF-SHA256(ikm = KEK, salt = tenant_id, info = "conveyor-secret-v1")   32 bytes
//! ct    = XChaCha20-Poly1305(kek_t, nonce, plaintext)  = nonce(24) ‖ ciphertext ‖ tag(16)
//! key_id = "kek:v1"
//! ```
//!
//! XChaCha20 rather than the IETF 12-byte nonce because the nonce is generated randomly per write: a random
//! 96-bit nonce has a birthday bound a long-lived shared key can reach, a 192-bit one does not.
//!
//! `key_id` is stored beside the ciphertext so a future KEK rotation can be a re-seal by generation rather
//! than a blind decrypt-everything. Today there is exactly one generation and an unknown `key_id` is a
//! **refusal**, not a guess.
//!
//! # Rotation
//!
//! `rotate` replaces the ciphertext and bumps `version`. The old value does not survive (S09 decision 5):
//! there is no history table, so "the secret we rotated away from" cannot leak from a backup of this row.
//!
//! # Failure modes
//!
//! A missing or malformed `CONVEYOR_SECRET_KEK` is [`SecretError::KeyUnavailable`] — the caller answers 500
//! and stores nothing. There is deliberately no path that writes plaintext when the key is absent; the
//! pre-S09 webhook secret was a plaintext column and that is exactly what this module removes.

use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{AeadCore, XChaCha20Poly1305, XNonce};
use conveyor_domain::{TenantId, Version};
use hkdf::Hkdf;
use sha2::Sha256;
use sqlx::PgConnection;
use std::sync::OnceLock;
use uuid::Uuid;

/// The one key generation this build writes and reads.
pub const KEY_ID: &str = "kek:v1";

/// HKDF `info` for the per-tenant expansion. Changing it rotates every derived key at once.
const HKDF_INFO: &[u8] = b"conveyor-secret-v1";

/// The XChaCha20 nonce length.
pub const NONCE_LEN: usize = 24;

/// The Poly1305 tag length.
pub const TAG_LEN: usize = 16;

/// Why a seal, an open or a secret row failed.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// `CONVEYOR_SECRET_KEK` is absent or not 32 base64-encoded bytes.
    #[error("the secret key is not configured (CONVEYOR_SECRET_KEK)")]
    KeyUnavailable,
    /// The ciphertext is shorter than a nonce and a tag, or the tag did not verify.
    #[error("the stored secret cannot be opened")]
    Undecryptable,
    /// `key_id` names a generation this build does not hold.
    #[error("secret key generation {0:?} is not available")]
    UnknownKeyId(String),
    /// No such row in this tenant.
    #[error("the secret does not exist in this tenant")]
    NotFound,
    /// The row moved; the caller's `If-Match` is stale.
    #[error("the secret is at version {current}, not the expected version")]
    VersionMismatch {
        /// The version the row holds now.
        current: conveyor_domain::Version,
    },
    /// `(tenant_id, name)` is unique.
    #[error("a secret named {0:?} already exists in this tenant")]
    NameTaken(String),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The process-wide sealer: the KEK is read from the environment once and never re-read.
///
/// A per-call environment read would let a mid-flight change of the variable make one request seal with a
/// key another request cannot open; one read at first use makes the key a property of the process.
pub fn sealer() -> Result<&'static Sealer, SecretError> {
    static SEALER: OnceLock<Result<Sealer, SecretError>> = OnceLock::new();
    match SEALER.get_or_init(Sealer::from_env) {
        Ok(sealer) => Ok(sealer),
        Err(error) => Err(match error {
            // The cached error is not `Copy`; re-create the same variant.
            SecretError::KeyUnavailable => SecretError::KeyUnavailable,
            _ => SecretError::KeyUnavailable,
        }),
    }
}

/// The KEK and the per-tenant derivation.
#[derive(Clone)]
pub struct Sealer {
    kek: [u8; 32],
}

impl std::fmt::Debug for Sealer {
    /// Never print the key material — not even a prefix.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sealer { kek: <redacted> }")
    }
}

impl Sealer {
    /// Parse a base64 32-byte KEK.
    pub fn from_base64(encoded: &str) -> Result<Self, SecretError> {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| SecretError::KeyUnavailable)?;
        let kek: [u8; 32] = bytes.try_into().map_err(|_| SecretError::KeyUnavailable)?;
        Ok(Self { kek })
    }

    /// The process's KEK from `CONVEYOR_SECRET_KEK`.
    pub fn from_env() -> Result<Self, SecretError> {
        match std::env::var("CONVEYOR_SECRET_KEK") {
            Ok(value) => Self::from_base64(&value),
            Err(_) => Err(SecretError::KeyUnavailable),
        }
    }

    /// The tenant's key: `HKDF-SHA256(KEK, salt = tenant_id)`.
    fn tenant_key(&self, tenant: TenantId) -> [u8; 32] {
        let hkdf = Hkdf::<Sha256>::new(Some(tenant.as_uuid().as_bytes()), &self.kek);
        let mut okm = [0u8; 32];
        hkdf.expand(HKDF_INFO, &mut okm)
            .expect("32 bytes is a valid HKDF-SHA256 output length");
        okm
    }

    /// Seal a value for one tenant. Returns `(ciphertext, key_id)`.
    pub fn seal(
        &self,
        tenant: TenantId,
        plaintext: &[u8],
    ) -> Result<(Vec<u8>, String), SecretError> {
        let cipher = XChaCha20Poly1305::new((&self.tenant_key(tenant)).into());
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let sealed = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| SecretError::Undecryptable)?;
        let mut out = Vec::with_capacity(NONCE_LEN + sealed.len());
        out.extend_from_slice(&nonce[..]);
        out.extend_from_slice(&sealed);
        Ok((out, KEY_ID.to_owned()))
    }

    /// Open a sealed value for one tenant.
    pub fn open(
        &self,
        tenant: TenantId,
        ciphertext: &[u8],
        key_id: &str,
    ) -> Result<Vec<u8>, SecretError> {
        if key_id != KEY_ID {
            return Err(SecretError::UnknownKeyId(key_id.to_owned()));
        }
        if ciphertext.len() < NONCE_LEN + TAG_LEN {
            return Err(SecretError::Undecryptable);
        }
        let (nonce, body) = ciphertext.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| SecretError::Undecryptable)?;
        let cipher = XChaCha20Poly1305::new((&self.tenant_key(tenant)).into());
        cipher
            .decrypt(&XNonce::from(nonce), body)
            .map_err(|_| SecretError::Undecryptable)
    }
}

// ── rows ────────────────────────────────────────────────────────────────────────────────────

/// A secret as the API returns it: **never** the ciphertext and never the plaintext.
#[derive(Clone, Debug, PartialEq)]
pub struct SecretRecord {
    /// The row id.
    pub id: Uuid,
    /// The caller-chosen name; unique per tenant.
    pub name: String,
    /// `tenant` today.
    pub scope: String,
    /// The key generation the ciphertext was sealed under.
    pub key_id: String,
    /// Bumped by every rotation.
    pub version: i64,
    /// When the row was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the row last changed (a rotation).
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

const SECRET_COLUMNS: &str = "id, name, scope, key_id, version, created_at, updated_at";

type SecretRow = (
    Uuid,
    String,
    String,
    String,
    i64,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

impl From<SecretRow> for SecretRecord {
    fn from(row: SecretRow) -> Self {
        Self {
            id: row.0,
            name: row.1,
            scope: row.2,
            key_id: row.3,
            version: row.4,
            created_at: row.5,
            updated_at: row.6,
        }
    }
}

/// Seal `value` and insert one secret. The plaintext never reaches SQL.
pub async fn create_secret_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    name: &str,
    value: &[u8],
    scope: &str,
) -> Result<SecretRecord, SecretError> {
    let sealer = sealer()?;
    let (ciphertext, key_id) = sealer.seal(tenant, value)?;
    let sql = format!(
        "INSERT INTO secret (tenant_id, name, ciphertext, key_id, scope)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {SECRET_COLUMNS}"
    );
    let row: SecretRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(name)
        .bind(ciphertext)
        .bind(key_id)
        .bind(scope)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                SecretError::NameTaken(name.to_owned())
            }
            _ => SecretError::Database(error),
        })?;
    Ok(row.into())
}

/// One secret's metadata, or `None` when it is not this tenant's.
pub async fn get_secret_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<SecretRecord>, SecretError> {
    let sql = format!("SELECT {SECRET_COLUMNS} FROM secret WHERE tenant_id = $1 AND id = $2");
    let row: Option<SecretRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(Into::into))
}

/// This tenant's secrets, ordered by name (the list has no cursor: a tenant has few secrets).
pub async fn list_secrets_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<SecretRecord>, SecretError> {
    let sql = format!(
        "SELECT {SECRET_COLUMNS} FROM secret WHERE tenant_id = $1 ORDER BY name, id LIMIT 1000"
    );
    let rows: Vec<SecretRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Replace the ciphertext with a seal of `value`, bumping `version`.
///
/// There is no history: the row is the secret, and a rotation is a new value under the same name.
///
/// The version is optimistic concurrency like every other write (CONTRACT §3): two rotations racing must not
/// both succeed, because the loser's value would be silently discarded by the winner.
pub async fn rotate_secret_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: conveyor_domain::Version,
    value: &[u8],
) -> Result<SecretRecord, SecretError> {
    let current: Option<(i64,)> =
        sqlx::query_as("SELECT version FROM secret WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    let (current,) = current.ok_or(SecretError::NotFound)?;
    let current_version = crate::version_of(current);
    if current_version != expected_version {
        return Err(SecretError::VersionMismatch {
            current: current_version,
        });
    }
    let sealer = sealer()?;
    let (ciphertext, key_id) = sealer.seal(tenant, value)?;
    let sql = format!(
        "UPDATE secret
            SET ciphertext = $3, key_id = $4, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {SECRET_COLUMNS}"
    );
    let row: Option<SecretRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(ciphertext)
        .bind(key_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(Into::into).ok_or(SecretError::NotFound)
}

/// Delete one secret, guarded by the caller's version.
///
/// Same optimistic-concurrency shape as [`rotate_secret_tx`]: the handler's read and this delete are
/// two statements, so a rotation racing between them must fail the delete (412) rather than let a
/// stale `If-Match` destroy the rotated value.
pub async fn delete_secret_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    expected_version: Version,
) -> Result<(), SecretError> {
    let current: Option<(i64,)> =
        sqlx::query_as("SELECT version FROM secret WHERE tenant_id = $1 AND id = $2 FOR UPDATE")
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    let (current,) = current.ok_or(SecretError::NotFound)?;
    let current_version = crate::version_of(current);
    if current_version != expected_version {
        return Err(SecretError::VersionMismatch {
            current: current_version,
        });
    }
    sqlx::query("DELETE FROM secret WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The one path that returns plaintext: open a stored secret for the process that must use it.
///
/// Deliberately not reachable from an action: no handler calls this, and a caller that did would have to
/// construct the request itself. Its consumers are the webhook signer and the repository credential path.
pub async fn open_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Vec<u8>, SecretError> {
    let row: Option<(Vec<u8>, String)> =
        sqlx::query_as("SELECT ciphertext, key_id FROM secret WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    let (ciphertext, key_id) = row.ok_or(SecretError::NotFound)?;
    sealer()?.open(tenant, &ciphertext, &key_id)
}

/// Whether this tenant has a secret with this name — a configuration check that does not
/// decrypt anything (S06d's `channels.bind` verifies the names a connector config declares).
pub async fn exists_named_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    name: &str,
) -> Result<bool, sqlx::Error> {
    let found: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM secret WHERE tenant_id = $1 AND name = $2 LIMIT 1")
            .bind(tenant.as_uuid())
            .bind(name)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(found.is_some())
}

/// Open the tenant's secret with this **name**, for a consumer that is configured by name rather than by id.
///
/// The same single path as [`open_tx`]: plaintext never leaves the process that must use it, and it is not
/// reachable from any action that returns a value. A missing row is [`SecretError::NotFound`] and the caller
/// decides what that means — `documents.ingest` reads it as "this tenant is not configured for ingestion",
/// which is a 422 rather than a 404.
pub async fn open_named_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    name: &str,
) -> Result<Vec<u8>, SecretError> {
    let row: Option<(Vec<u8>, String)> =
        sqlx::query_as("SELECT ciphertext, key_id FROM secret WHERE tenant_id = $1 AND name = $2")
            .bind(tenant.as_uuid())
            .bind(name)
            .fetch_optional(&mut *conn)
            .await?;
    let (ciphertext, key_id) = row.ok_or(SecretError::NotFound)?;
    sealer()?.open(tenant, &ciphertext, &key_id)
}
