//! The three licence-token write paths, ported from the pinned Python service.
//!
//! | Rust | reference |
//! |---|---|
//! | [`token_expiry_refusal`] | `models.py:843` `token_expiry_refusal` |
//! | [`resolve_new_token_expiry`] | `models.py:874` `resolve_new_token_expiry` |
//! | [`implicit_long_lived_registration`] | `models.py:802` `implicit_long_lived_registration` |
//! | [`create_token`] | `models.py:8786` `DatabaseManager.create_token` |
//! | [`regenerate_token_for_license`] | `models.py:8609` `regenerate_token_for_license` |
//! | [`accepts_new_token`] | `models.py:3244` `License.accepts_new_token` |
//! | [`is_active`] | `models.py:3230` `License.is_active` |
//! | [`generate_token`] | `models.py:3564` `Token.generate_token` |
//!
//! The expiry policy is the reference's, not a re-derivation (licenses#244): an explicit expiry
//! always wins; an `api` mint with no expiry gets `TOKEN_API_DEFAULT_EXPIRY_DAYS` (365) UNLESS the
//! verified `(iss, azp)` is a registered implicit client, which gets a registered long-lived token;
//! a perpetual token otherwise needs `long_lived` plus owner and reason, and an `api` long-lived
//! token additionally needs the LICENCE's tenant (id or name) in `LONG_LIVED_API_TENANTS`.
//! `internal`/`node` with no expiry and no registration is REFUSED, never defaulted.
//!
//! Rotation never lengthens a credential: the replacement inherits the LATEST expiry of the live
//! tokens it replaces, any perpetual predecessor keeps it perpetual (and keeps its registration),
//! and an all-expired or all-revoked set is a refusal that writes nothing. The revoke and the mint
//! are ONE transaction, so a failure cannot leave the licence with zero tokens.

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::tokens::hash_token;

/// The reference's `OPERATOR_GRANTABLE_ROLES`: the only roles an operator token may carry.
pub const OPERATOR_GRANTABLE_ROLES: [&str; 1] = ["platform-admin"];
/// `OPERATOR_TOKEN_MAX_TTL_MINUTES` — the hard cap (the DB CHECK allows +5 min of clock skew).
pub const OPERATOR_TOKEN_MAX_TTL_MINUTES: i64 = 60;
/// The reference's `ttl_minutes` default.
pub const OPERATOR_TOKEN_DEFAULT_TTL_MINUTES: i64 = 15;

/// `TOKEN_API_DEFAULT_EXPIRY_DAYS`'s default (the env override is read per call).
const DEFAULT_TOKEN_API_EXPIRY_DAYS: i64 = 365;
/// `LONG_LIVED_API_TENANTS`'s default.
const DEFAULT_LONG_LIVED_API_TENANTS: &str = "n8n-dev";

/// `SECURE_APPS_ISSUER`, exactly as Keycloak emits it in `iss` (no trailing slash).
pub const SECURE_APPS_ISSUER: &str = "https://auth.nonagentic.ai/realms/secure-apps";

/// `LONG_LIVED_TOKEN_TYPES`: types that may be registered long-lived on any licence.
pub const LONG_LIVED_TOKEN_TYPES: [&str; 2] = ["internal", "node"];

/// `_positive_int_env("TOKEN_API_DEFAULT_EXPIRY_DAYS", 365)`.
pub fn token_api_default_expiry_days() -> i64 {
    match std::env::var("TOKEN_API_DEFAULT_EXPIRY_DAYS") {
        Ok(raw) if !raw.trim().is_empty() => match raw.trim().parse::<i64>() {
            Ok(value) if value >= 1 => value,
            _ => DEFAULT_TOKEN_API_EXPIRY_DAYS,
        },
        _ => DEFAULT_TOKEN_API_EXPIRY_DAYS,
    }
}

/// `LONG_LIVED_API_TENANTS` — tenant ids OR names, matched against the LICENCE's row.
pub fn long_lived_api_tenants() -> BTreeSet<String> {
    std::env::var("LONG_LIVED_API_TENANTS")
        .unwrap_or_else(|_| DEFAULT_LONG_LIVED_API_TENANTS.to_owned())
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `LongLivedImplicitClient`: one registered `(iss, azp)` pair and the registration its
/// expiry-less `api` mints receive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongLivedImplicitClient {
    pub iss: String,
    pub azp: String,
    pub owner: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct ImplicitClientJson {
    iss: String,
    azp: String,
    owner: String,
    reason: String,
}

/// `_DEFAULT_LONG_LIVED_IMPLICIT_CLIENTS`: commonpractice's provisioning client, the caller that
/// cannot pass an expiry yet.
fn default_implicit_clients() -> Vec<LongLivedImplicitClient> {
    vec![LongLivedImplicitClient {
        iss: SECURE_APPS_ISSUER.to_owned(),
        azp: "commonpractice-licenses-admin".to_owned(),
        owner: "m4 / commonpractice".to_owned(),
        reason: "caller cannot pass expiry; migrate before removal".to_owned(),
    }]
}

/// `LONG_LIVED_IMPLICIT_CLIENTS`, keyed by the verified `(iss, azp)` pair.
///
/// The reference raises at import on a malformed override (the service does not start). This port
/// reads the environment on each call and fails CLOSED on a malformed value: the class is empty,
/// so no caller receives an implicit perpetual token. That is a visible misconfiguration, never
/// a widening.
pub fn long_lived_implicit_clients() -> BTreeMap<(String, String), LongLivedImplicitClient> {
    let clients = match std::env::var("LONG_LIVED_IMPLICIT_CLIENTS") {
        Ok(raw) if !raw.trim().is_empty() => {
            match serde_json::from_str::<Vec<ImplicitClientJson>>(raw.trim()) {
                Ok(entries) => entries
                    .into_iter()
                    .filter(|entry| {
                        let complete = !entry.iss.trim().is_empty()
                            && !entry.azp.trim().is_empty()
                            && !entry.owner.trim().is_empty()
                            && !entry.reason.trim().is_empty();
                        if !complete {
                            tracing::error!(
                                "LONG_LIVED_IMPLICIT_CLIENTS needs a non-empty iss, azp, owner \
                                 and reason on every entry; dropping the incomplete entry"
                            );
                        }
                        complete
                    })
                    .map(|entry| LongLivedImplicitClient {
                        iss: entry.iss.trim().to_owned(),
                        azp: entry.azp.trim().to_owned(),
                        owner: entry.owner.trim().to_owned(),
                        reason: entry.reason.trim().to_owned(),
                    })
                    .collect(),
                Err(error) => {
                    tracing::error!(
                        %error,
                        "LONG_LIVED_IMPLICIT_CLIENTS is not a JSON array; no implicit client \
                         will receive a long-lived token"
                    );
                    Vec::new()
                }
            }
        }
        _ => default_implicit_clients(),
    };
    clients
        .into_iter()
        .map(|client| ((client.iss.clone(), client.azp.clone()), client))
        .collect()
}

/// `LongLivedRegistration`: who owns a perpetual token, why it may be perpetual, and (only for
/// the route's implicit path) the verified client that qualified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongLivedRegistration {
    pub owner: String,
    pub reason: String,
    pub implicit_client: Option<(String, String)>,
}

/// `implicit_long_lived_registration`: the registration an allowlisted caller's expiry-less
/// `api` mint gets, else `None`. Both claims must equal a registered entry exactly.
pub fn implicit_long_lived_registration(
    iss: Option<&str>,
    azp: Option<&str>,
    token_type: &str,
    has_expiry: bool,
    explicit_long_lived: bool,
) -> Option<LongLivedRegistration> {
    let (iss, azp) = (iss?, azp?);
    if token_type != "api" || has_expiry || explicit_long_lived {
        return None;
    }
    let client = long_lived_implicit_clients()
        .get(&(iss.to_owned(), azp.to_owned()))
        .cloned()?;
    Some(LongLivedRegistration {
        owner: client.owner,
        reason: client.reason,
        implicit_client: Some((client.iss, client.azp)),
    })
}

/// `TOKEN_EXPIRY_REFUSALS`, keyed by the discriminator the mint route returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalCode {
    /// Route-level: owner/reason passed without `long_lived=true`.
    LongLivedFieldsWithoutLongLived,
    LongLivedRequiresOwnerAndReason,
    LongLivedWithExpiry,
    LongLivedNotPermitted,
    PerpetualTokenNotRegistered,
}

impl RefusalCode {
    pub fn code(self) -> &'static str {
        match self {
            Self::LongLivedFieldsWithoutLongLived => "long_lived_fields_without_long_lived",
            Self::LongLivedRequiresOwnerAndReason => "long_lived_requires_owner_and_reason",
            Self::LongLivedWithExpiry => "long_lived_with_expiry",
            Self::LongLivedNotPermitted => "long_lived_not_permitted",
            Self::PerpetualTokenNotRegistered => "perpetual_token_not_registered",
        }
    }

    /// The literal `remedy` each refusal raises with in `main.py:6589-6684`.
    pub fn remedy(self) -> &'static str {
        match self {
            Self::LongLivedFieldsWithoutLongLived => {
                "long_lived_owner/long_lived_reason require long_lived=true"
            }
            Self::LongLivedRequiresOwnerAndReason => {
                "pass non-empty long_lived_owner and long_lived_reason"
            }
            Self::LongLivedWithExpiry => {
                "a long-lived token is perpetual: drop expires_days or long_lived"
            }
            Self::LongLivedNotPermitted => {
                "long-lived is allowed for internal/node, and for api only on the n8n \
                 licence-token tenants; pass expires_days instead"
            }
            Self::PerpetualTokenNotRegistered => {
                "pass expires_days, or long_lived=true with long_lived_owner and long_lived_reason"
            }
        }
    }
}

/// `token_expiry_refusal`: the policy's refusal for a NEW mint, or `None` when it may proceed.
///
/// `tenant_keys` is the licence's tenant id and name; it only matters for a long-lived `api`
/// token.
pub fn token_expiry_refusal(
    token_type: &str,
    has_expiry: bool,
    long_lived: Option<&LongLivedRegistration>,
    tenant_keys: &BTreeSet<String>,
) -> Option<RefusalCode> {
    if let Some(registration) = long_lived {
        if registration.owner.trim().is_empty() || registration.reason.trim().is_empty() {
            return Some(RefusalCode::LongLivedRequiresOwnerAndReason);
        }
        if has_expiry {
            return Some(RefusalCode::LongLivedWithExpiry);
        }
        if LONG_LIVED_TOKEN_TYPES.contains(&token_type) {
            return None;
        }
        if token_type == "api" {
            let allowlist = long_lived_api_tenants();
            if tenant_keys.iter().any(|key| allowlist.contains(key)) {
                return None;
            }
            if let Some((iss, azp)) = registration.implicit_client.as_ref()
                && long_lived_implicit_clients().contains_key(&(iss.clone(), azp.clone()))
            {
                return None;
            }
        }
        return Some(RefusalCode::LongLivedNotPermitted);
    }
    if has_expiry || token_type == "api" {
        return None;
    }
    // internal/node, AND any unknown type: refused, never defaulted (fail closed).
    Some(RefusalCode::PerpetualTokenNotRegistered)
}

/// `resolve_new_token_expiry`: the expiry a NEW token is stored with, or the refusal code.
///
/// `None` (perpetual) is returned only for a registered long-lived mint; an `api` mint with no
/// expiry gets the bounded default.
pub fn resolve_new_token_expiry(
    token_type: &str,
    expires_at: Option<DateTime<Utc>>,
    long_lived: Option<&LongLivedRegistration>,
    tenant_keys: &BTreeSet<String>,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, RefusalCode> {
    if let Some(refusal) = token_expiry_refusal(token_type, expires_at.is_some(), long_lived, tenant_keys)
    {
        return Err(refusal);
    }
    if long_lived.is_some() || expires_at.is_some() {
        return Ok(expires_at);
    }
    Ok(Some(now + Duration::days(token_api_default_expiry_days())))
}

/// `Token.generate_token`: `secrets.token_urlsafe(32)` — 32 random bytes, URL-safe base64, no
/// padding (43 characters).
pub fn generate_token() -> Result<String, &'static str> {
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|_| "the OS random source failed")?;
    Ok(URL_SAFE_NO_PAD.encode(random))
}

/// A token row as the write paths return it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TokenRow {
    pub id: Uuid,
    pub license_id: Uuid,
    pub token_type: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub long_lived_owner: Option<String>,
}

/// The licence fields the three write paths decide on. `get_license` has no `deleted_at` filter,
/// so neither does [`licence`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MintLicence {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub app_id: Uuid,
    pub subject_id: Option<Uuid>,
    pub status: String,
    pub starts_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub is_dependency: bool,
}

/// `DatabaseManager.get_license`, restricted to the fields these routes read.
pub async fn licence(pool: &PgPool, license_id: Uuid) -> Result<Option<MintLicence>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, tenant_id, app_id, subject_id, status, starts_at, expires_at, deleted_at,
                is_dependency
           FROM license
          WHERE id = $1",
    )
    .bind(license_id)
    .fetch_optional(pool)
    .await
}

/// The tenant's name and archive state; `None` means no tenant row.
pub async fn tenant_name_and_archive(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Option<(String, Option<DateTime<Utc>>)>, sqlx::Error> {
    sqlx::query_as("SELECT name, archived_at FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
}

/// `DatabaseManager.get_subject`'s identity field, as the operator route compares it.
pub async fn subject_keycloak_sub(
    pool: &PgPool,
    subject_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT keycloak_sub FROM subject WHERE id = $1")
        .bind(subject_id)
        .fetch_optional(pool)
        .await
}

/// `License.accepts_new_token`: may a NEW token be minted on this licence?
///
/// Wider than [`is_active`] by exactly one case: `pending` only because `starts_at` is in the
/// future. Everything else `is_active` refuses is refused here too.
pub fn accepts_new_token(licence: &MintLicence, now: DateTime<Utc>) -> bool {
    licence.deleted_at.is_none()
        && licence.status == "active"
        && !licence.expires_at.is_some_and(|expiry| expiry < now)
}

/// `License.is_active()` — `effective_status() == "active"`, reused rather than re-derived.
pub fn is_active(licence: &MintLicence, now: DateTime<Utc>) -> bool {
    licence.status == "active"
        && !licence.starts_at.is_some_and(|start| start > now)
        && !licence.expires_at.is_some_and(|expiry| expiry < now)
}

/// `_inherited_rotation_expiry`: any perpetual predecessor keeps the replacement perpetual;
/// otherwise the LATEST replaced expiry; nothing replaced is `None` (a new mint, not perpetual).
pub fn inherited_rotation_expiry(
    replaced: &[Option<DateTime<Utc>>],
) -> Option<DateTime<Utc>> {
    if replaced.is_empty() || replaced.iter().any(Option::is_none) {
        return None;
    }
    replaced.iter().flatten().max().copied()
}

#[derive(Debug, thiserror::Error)]
pub enum CreateTokenError {
    #[error("the new-token expiry policy refused the mint: {}", .0.code())]
    Policy(RefusalCode),
    #[error("token generation failed: {0}")]
    Random(&'static str),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum RegenerateError {
    #[error("every unrevoked token has expired (most recent {newest_expiry:?})")]
    AllTokensExpired {
        newest_expiry: Option<DateTime<Utc>>,
    },
    #[error("all {revoked_count} token(s) on the licence are revoked")]
    AllTokensRevoked { revoked_count: i64 },
    #[error("the new-token expiry policy refused the mint: {}", .0.code())]
    Policy(RefusalCode),
    #[error("token generation failed: {0}")]
    Random(&'static str),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

const INSERT_TOKEN: &str = "INSERT INTO token
       (license_id, token_hash, token_type, expires_at, granted_roles, long_lived_owner,
        long_lived_reason)
 VALUES ($1, $2, $3, $4, $5, $6, $7)
 RETURNING id, license_id, token_type, created_at, expires_at, long_lived_owner";

/// `DatabaseManager.create_token`: apply the expiry policy, then insert the row.
///
/// The raw token never reaches this layer — only its hash. `granted_roles` is for the operator
/// route ONLY; the DB CHECKs refuse any role but `platform-admin` and any role-bearing token
/// without a short expiry.
pub async fn create_token(
    pool: &PgPool,
    license_id: Uuid,
    token_type: &str,
    expires_at: Option<DateTime<Utc>>,
    granted_roles: &[String],
    long_lived: Option<&LongLivedRegistration>,
) -> Result<(TokenRow, String), CreateTokenError> {
    let tenant_keys = if long_lived.is_some() && token_type == "api" {
        tenant_keys_for_licence(pool, license_id).await?
    } else {
        BTreeSet::new()
    };
    let expires_at = resolve_new_token_expiry(
        token_type,
        expires_at,
        long_lived,
        &tenant_keys,
        Utc::now(),
    )
    .map_err(CreateTokenError::Policy)?;

    let raw_token = generate_token().map_err(CreateTokenError::Random)?;
    let row: TokenRow = sqlx::query_as(INSERT_TOKEN)
        .bind(license_id)
        .bind(hash_token(&raw_token))
        .bind(token_type)
        .bind(expires_at)
        .bind(granted_roles)
        .bind(long_lived.map(|registration| registration.owner.trim().to_owned()))
        .bind(long_lived.map(|registration| registration.reason.trim().to_owned()))
        .fetch_one(pool)
        .await?;
    Ok((row, raw_token))
}

/// The n8n path is decided by the LICENCE's tenant row, never a header: `{id, name}`.
async fn tenant_keys_for_licence(
    pool: &PgPool,
    license_id: Uuid,
) -> Result<BTreeSet<String>, sqlx::Error> {
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT t.id, t.name
           FROM tenants t
           JOIN license l ON l.tenant_id = t.id
          WHERE l.id = $1",
    )
    .bind(license_id)
    .fetch_optional(pool)
    .await?;
    Ok(row
        .map(|(id, name)| BTreeSet::from([id.to_string(), name]))
        .unwrap_or_default())
}

/// `regenerate_token_for_license`: revoke EVERY live token and mint the replacement in ONE
/// transaction. The replacement inherits the expiry (and any long-lived registration) of the
/// tokens it replaces; an all-expired or all-revoked set refuses without writing.
pub async fn regenerate_token_for_license(
    pool: &PgPool,
    license_id: Uuid,
) -> Result<(i64, TokenRow, String), RegenerateError> {
    let now = Utc::now();
    let mut tx = pool.begin().await?;

    #[derive(sqlx::FromRow)]
    struct LiveToken {
        expires_at: Option<DateTime<Utc>>,
        long_lived_owner: Option<String>,
        long_lived_reason: Option<String>,
    }

    let live: Vec<LiveToken> = sqlx::query_as(
        "SELECT expires_at, long_lived_owner, long_lived_reason
           FROM token
          WHERE license_id = $1
            AND NOT revoked
          ORDER BY created_at, id",
    )
    .bind(license_id)
    .fetch_all(&mut *tx)
    .await?;

    let expiries: Vec<Option<DateTime<Utc>>> = live.iter().map(|token| token.expires_at).collect();
    let mut expires_at = inherited_rotation_expiry(&expiries);

    // A REGISTERED long-lived predecessor stays registered across rotation: the replacement is
    // the same owned credential.
    let mut registration: Option<LongLivedRegistration> = None;
    if expires_at.is_none()
        && let Some(token) = live
            .iter()
            .find(|token| token.expires_at.is_none() && token.long_lived_owner.is_some())
    {
        registration = Some(LongLivedRegistration {
            owner: token.long_lived_owner.clone().unwrap_or_default(),
            reason: token.long_lived_reason.clone().unwrap_or_default(),
            implicit_client: None,
        });
    }

    // An EXPIRED token is not a credential the licence still holds either. `<`, not `<=`:
    // `Token.is_valid` treats `expires_at == now` as still valid.
    if expires_at.is_some_and(|expiry| expiry < now) {
        tx.rollback().await?;
        return Err(RegenerateError::AllTokensExpired {
            newest_expiry: expires_at,
        });
    }

    if live.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM token WHERE license_id = $1")
            .bind(license_id)
            .fetch_one(&mut *tx)
            .await?;
        if total > 0 {
            tx.rollback().await?;
            return Err(RegenerateError::AllTokensRevoked {
                revoked_count: total,
            });
        }
        // NEVER had a token: a NEW mint, so the new-token policy applies (an `api` token gets
        // the bounded default, never perpetuity).
        expires_at = resolve_new_token_expiry("api", None, None, &BTreeSet::new(), now)
            .map_err(RegenerateError::Policy)?;
    }

    let revoked_count = sqlx::query(
        "UPDATE token SET revoked = true, revoked_at = $2
          WHERE license_id = $1 AND revoked = false",
    )
    .bind(license_id)
    .bind(now)
    .execute(&mut *tx)
    .await?
    .rows_affected() as i64;

    let raw_token = generate_token().map_err(RegenerateError::Random)?;
    let row: TokenRow = sqlx::query_as(INSERT_TOKEN)
        .bind(license_id)
        .bind(hash_token(&raw_token))
        .bind("api")
        .bind(expires_at)
        .bind(Vec::<String>::new())
        .bind(registration.as_ref().map(|value| value.owner.clone()))
        .bind(registration.as_ref().map(|value| value.reason.clone()))
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((revoked_count, row, raw_token))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(owner: &str, reason: &str) -> LongLivedRegistration {
        LongLivedRegistration {
            owner: owner.to_owned(),
            reason: reason.to_owned(),
            implicit_client: None,
        }
    }

    #[test]
    fn an_api_token_without_expiry_is_allowed_but_internal_is_not() {
        let keys = BTreeSet::new();
        assert_eq!(token_expiry_refusal("api", false, None, &keys), None);
        assert_eq!(
            token_expiry_refusal("internal", false, None, &keys),
            Some(RefusalCode::PerpetualTokenNotRegistered)
        );
        assert_eq!(token_expiry_refusal("node", true, None, &keys), None);
    }

    #[test]
    fn a_long_lived_token_needs_owner_reason_and_no_expiry() {
        let keys = BTreeSet::new();
        let empty = registration("  ", "why");
        assert_eq!(
            token_expiry_refusal("internal", false, Some(&empty), &keys),
            Some(RefusalCode::LongLivedRequiresOwnerAndReason)
        );
        let ok = registration("ops", "permanent");
        assert_eq!(
            token_expiry_refusal("internal", true, Some(&ok), &keys),
            Some(RefusalCode::LongLivedWithExpiry)
        );
        assert_eq!(token_expiry_refusal("internal", false, Some(&ok), &keys), None);
    }

    #[test]
    fn a_long_lived_api_token_needs_an_allowlisted_tenant_or_client() {
        let keys = BTreeSet::from(["tenant-a".to_owned()]);
        let ok = registration("ops", "permanent");
        assert_eq!(
            token_expiry_refusal("api", false, Some(&ok), &keys),
            Some(RefusalCode::LongLivedNotPermitted)
        );

        let allowed = BTreeSet::from(["n8n-dev".to_owned()]);
        assert_eq!(token_expiry_refusal("api", false, Some(&ok), &allowed), None);

        let implicit = LongLivedRegistration {
            owner: "m4 / commonpractice".to_owned(),
            reason: "caller cannot pass expiry; migrate before removal".to_owned(),
            implicit_client: Some((
                SECURE_APPS_ISSUER.to_owned(),
                "commonpractice-licenses-admin".to_owned(),
            )),
        };
        assert_eq!(
            token_expiry_refusal("api", false, Some(&implicit), &keys),
            None
        );
    }

    #[test]
    fn the_default_expiry_is_only_for_an_api_mint_without_one() {
        let now = Utc::now();
        let keys = BTreeSet::new();
        let resolved = resolve_new_token_expiry("api", None, None, &keys, now).expect("allowed");
        let days = resolved.expect("defaulted").signed_duration_since(now).num_days();
        assert!((364..=365).contains(&days), "{days}");
        assert_eq!(
            resolve_new_token_expiry("api", Some(now), None, &keys, now).expect("explicit"),
            Some(now)
        );
    }

    #[test]
    fn the_implicit_client_qualifies_only_for_an_expiry_less_api_mint() {
        let issuer = Some(SECURE_APPS_ISSUER);
        let client = Some("commonpractice-licenses-admin");
        assert!(
            implicit_long_lived_registration(issuer, client, "api", false, false).is_some()
        );
        assert!(implicit_long_lived_registration(issuer, client, "api", true, false).is_none());
        assert!(implicit_long_lived_registration(issuer, client, "api", false, true).is_none());
        assert!(
            implicit_long_lived_registration(issuer, Some("another-client"), "api", false, false)
                .is_none()
        );
        assert!(
            implicit_long_lived_registration(Some("https://auth.example/realms/portal"), client, "api", false, false)
                .is_none()
        );
    }

    #[test]
    fn rotation_inherits_the_latest_expiry_and_never_perpetuity_by_accident() {
        let now = Utc::now();
        let later = now + Duration::days(30);
        assert_eq!(
            inherited_rotation_expiry(&[Some(now), Some(later)]),
            Some(later)
        );
        assert_eq!(inherited_rotation_expiry(&[Some(now), None]), None);
        assert_eq!(inherited_rotation_expiry(&[]), None);
    }

    #[test]
    fn accepts_new_token_is_wider_than_active_by_one_pending_case() {
        let now = Utc::now();
        let mut licence = MintLicence {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            app_id: Uuid::nil(),
            subject_id: None,
            status: "active".to_owned(),
            starts_at: Some(now + Duration::days(1)),
            expires_at: None,
            deleted_at: None,
            is_dependency: false,
        };
        assert!(!is_active(&licence, now));
        assert!(accepts_new_token(&licence, now));
        licence.starts_at = Some(now);
        licence.expires_at = Some(now - Duration::seconds(1));
        assert!(!accepts_new_token(&licence, now));
        licence.expires_at = None;
        licence.status = "revoked".to_owned();
        assert!(!accepts_new_token(&licence, now));
        licence.status = "active".to_owned();
        licence.deleted_at = Some(now);
        assert!(!accepts_new_token(&licence, now));
    }

    #[test]
    fn a_generated_token_is_url_safe_base64_of_32_bytes() {
        let token = generate_token().expect("random");
        assert_eq!(token.len(), 43);
        assert!(!token.contains('+') && !token.contains('/') && !token.contains('='));
    }
}
