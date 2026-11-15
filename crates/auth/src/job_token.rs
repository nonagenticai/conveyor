//! The runner's job token: a short-lived HMAC credential a dispatched pod presents instead of a
//! session (ADR 0007 §Decision 4, issue #144 P3).
//!
//! Why not Keycloak: the token must be **revocable per job** — it dies when the job finishes, is
//! superseded or is cancelled — and minting one must not need an external issuer or a network
//! round trip inside the dispatch path. A local HMAC over claims does exactly that, and the claims
//! are what the byte routes fence on.
//!
//! Shape: `v1.<base64url-nopad(payload)>.<base64url-nopad(hmac-sha256(key, "v1.<payload>"))>`,
//! payload `{"j":job,"t":tenant,"a":attempt,"e":unix_secs}`. The version prefix is inside the MAC
//! so a v2 token cannot be read as a v1 one.
//!
//! The key is `CONVEYOR_JOB_TOKEN_KEY`: 32 bytes, hex/base64 accepted, **fail-closed** — an unset
//! key means no token can be minted or verified, never a default. [`JobTokenKey::from_env`] is the
//! only reader; the deployment unit supplies the value.
//!
//! This module deliberately does NOT touch the database. The row fence (status, live lease, matching
//! tenant and attempt) lives where the row is read — `crates/api/src/job_routes.rs` — so this crate
//! stays a pure credential primitive and the runner crate stays free of key material.

use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, TimeZone, Utc};
use conveyor_domain::TenantId;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

/// The environment variable the deployment supplies.
pub const JOB_TOKEN_ENV: &str = "CONVEYOR_JOB_TOKEN_KEY";

/// The only token version this code mints or accepts.
pub const JOB_TOKEN_VERSION: &str = "v1";

/// The longest lifetime [`mint`] will write. A media job's wall clock is minutes; a day is already
/// an order of magnitude beyond it and keeps a forgotten dispatch from being a week-long key.
pub const MAX_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// The key's required length, in bytes.
pub const KEY_BYTES: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// What a verified token asserts. The row must still agree with every field — the token proves the
/// claims were minted here, not that the job is still in that state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobClaims {
    pub job_id: Uuid,
    pub tenant: TenantId,
    pub attempt: i32,
    pub expires_at: DateTime<Utc>,
}

/// Why a token was refused. Every variant is deliberately indistinguishable to the caller: the
/// routes answer one 403 body for all of them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum JobTokenError {
    /// `CONVEYOR_JOB_TOKEN_KEY` is unset or unreadable: nothing can be minted or verified.
    #[error("the job token key is not configured")]
    KeyMissing,
    /// The key is not 32 bytes of usable material.
    #[error("the job token key is not {KEY_BYTES} bytes")]
    KeyMalformed,
    /// Not three dot-separated parts, or not decodable.
    #[error("the job token is malformed")]
    Malformed,
    /// A version this code does not mint.
    #[error("the job token version is not supported")]
    UnsupportedVersion,
    /// The MAC did not match — a forged or truncated token.
    #[error("the job token does not verify")]
    BadMac,
    /// The claims is not the shape this code writes.
    #[error("the job token claims are not readable")]
    BadClaims,
    /// The token was minted correctly and has expired.
    #[error("the job token has expired")]
    Expired,
}

/// The HMAC key, held only as bytes and never logged.
#[derive(Clone)]
pub struct JobTokenKey([u8; KEY_BYTES]);

impl std::fmt::Debug for JobTokenKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JobTokenKey(<redacted>)")
    }
}

impl JobTokenKey {
    /// Read the key from the environment. `Ok(None)` means "runner dispatch is not configured" —
    /// the caller decides whether that is a 503 (routes) or a permanent job failure (dispatch).
    /// An unset or empty variable is never a zero key.
    pub fn from_env() -> Result<Option<Self>, JobTokenError> {
        match std::env::var(JOB_TOKEN_ENV) {
            Ok(raw) if raw.trim().is_empty() => Ok(None),
            Ok(raw) => Self::parse(&raw).map(Some),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(_) => Err(JobTokenError::KeyMissing),
        }
    }

    /// Parse a configured value: base64url/base64/hex when it decodes to [`KEY_BYTES`], else the raw
    /// bytes when the value is exactly that long.
    pub fn parse(raw: &str) -> Result<Self, JobTokenError> {
        use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
        let raw = raw.trim();
        for decoded in [
            URL_SAFE_NO_PAD.decode(raw),
            STANDARD.decode(raw),
            URL_SAFE.decode(raw),
        ]
        .into_iter()
        .flatten()
        {
            if decoded.len() == KEY_BYTES {
                return Ok(Self(decoded.try_into().expect("length checked")));
            }
        }
        if let Some(bytes) = decode_hex(raw) {
            if bytes.len() == KEY_BYTES {
                return Ok(Self(bytes.try_into().expect("length checked")));
            }
        }
        if raw.as_bytes().len() == KEY_BYTES {
            return Ok(Self(raw.as_bytes().try_into().expect("length checked")));
        }
        Err(JobTokenError::KeyMalformed)
    }

    fn mac(&self, signed: &str) -> HmacSha256 {
        let mut mac = HmacSha256::new_from_slice(&self.0).expect("HMAC accepts any key length");
        mac.update(signed.as_bytes());
        mac
    }
}

/// The wire form. Field names are short because the token travels in an environment variable and a
/// ConfigMap size limit is a real one.
#[derive(Deserialize, Serialize)]
struct Wire {
    j: Uuid,
    t: Uuid,
    a: i32,
    e: i64,
}

impl Wire {
    fn from_claims(claims: &JobClaims) -> Self {
        Self {
            j: claims.job_id,
            t: claims.tenant.as_uuid(),
            a: claims.attempt,
            e: claims.expires_at.timestamp(),
        }
    }

    fn into_claims(self) -> Result<JobClaims, JobTokenError> {
        if self.a <= 0 {
            return Err(JobTokenError::BadClaims);
        }
        Ok(JobClaims {
            job_id: self.j,
            tenant: TenantId::from_uuid(self.t).map_err(|_| JobTokenError::BadClaims)?,
            attempt: self.a,
            expires_at: Utc
                .timestamp_opt(self.e, 0)
                .single()
                .ok_or(JobTokenError::BadClaims)?,
        })
    }
}

/// Mint a token for `claims`. The expiry is whatever the caller stamped — use [`expires_at`] so the
/// [`MAX_TTL`] clamp applies.
pub fn mint(key: &JobTokenKey, claims: &JobClaims) -> String {
    let wire = serde_json::to_vec(&Wire::from_claims(claims)).expect("the wire form serialises");
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(wire);
    let signed = format!("{JOB_TOKEN_VERSION}.{payload}");
    let mac = key.mac(&signed).finalize().into_bytes();
    format!("{signed}.{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac))
}

/// The expiry a dispatch should stamp: `now + ttl`, clamped to [`MAX_TTL`].
/// The wire carries whole seconds, so this returns a whole second: the claims a caller mints and
/// the claims a verifier reads are then the same value, not merely the same instant.
pub fn expires_at(now: DateTime<Utc>, ttl: Duration) -> DateTime<Utc> {
    let ttl = ttl.min(MAX_TTL);
    let at = now + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::hours(1));
    Utc.timestamp_opt(at.timestamp(), 0)
        .single()
        .unwrap_or(at)
}

/// Verify `token` at `now`. Every failure — bad shape, bad MAC, unsupported version, expired —
/// collapses to one error variant per cause; the caller must not tell them apart.
pub fn verify(key: &JobTokenKey, token: &str, now: DateTime<Utc>) -> Result<JobClaims, JobTokenError> {
    let token = token.trim();
    let mut parts = token.split('.');
    let (Some(version), Some(payload), Some(mac), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(JobTokenError::Malformed);
    };
    if version != JOB_TOKEN_VERSION {
        return Err(JobTokenError::UnsupportedVersion);
    }
    let signed = format!("{version}.{payload}");
    let presented = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(mac)
        .map_err(|_| JobTokenError::Malformed)?;
    key.mac(&signed)
        .verify_slice(&presented)
        .map_err(|_| JobTokenError::BadMac)?;

    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| JobTokenError::Malformed)?;
    let wire: Wire = serde_json::from_slice(&bytes).map_err(|_| JobTokenError::BadClaims)?;
    let claims = wire.into_claims()?;
    if claims.expires_at <= now {
        return Err(JobTokenError::Expired);
    }
    Ok(claims)
}

fn decode_hex(raw: &str) -> Option<Vec<u8>> {
    if raw.len() % 2 != 0 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = Vec::with_capacity(raw.len() / 2);
    let bytes = raw.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}
