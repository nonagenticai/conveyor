#![forbid(unsafe_code)]
//! `conveyor admin upstream-token` — the per-tenant upstream bearer, sealed in the database.
//!
//! The conveyor calls two services with a **tenant's own** credential: `laya.judge` (the SOP
//! judge) and `inference.chat`/`completions.create` (the router). There is **no platform token**:
//! each tenant's token is a sealed `secret` row (`laya.token` / `inference.token`), which the
//! tenant may replace at any time with its own BYO value through `secrets.create`/`secrets.rotate`.
//!
//! This command fills that row. Two sources:
//!
//! * [`TokenSource::Mint`] — the licences API mints a short-lived token on the tenant's per-app
//!   licence (`POST /internal/tenants/{id}/bundle-token?app=…`). The row stores the minted
//!   envelope `{token, expires_at, source, licence_id}` so the engine can send `.token` and an
//!   operator can see when it expires.
//! * [`TokenSource::Byo`] — the tenant's own token, stored **raw**. The engine accepts both
//!   shapes; a mint never overwrites a raw row (only this command rewrites a row, explicitly).
//!
//! The value is never printed and never logged; the report names the secret, its source and (for
//! a mint) its expiry.

use conveyor_auth::LicenceSyncClient;
use conveyor_domain::Version;
use conveyor_store as store;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// The upstream services whose tenant credential the conveyor sends as the bearer.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum UpstreamService {
    /// The SOP judge (`laya.judge`).
    Laya,
    /// The inference router (`inference.chat`, `completions.create`).
    Inference,
}

impl UpstreamService {
    /// The sealed secret's name — the key the engine's `auth` names.
    pub fn secret(self) -> &'static str {
        match self {
            Self::Laya => conveyor_actions::process::judge::LAYA_TOKEN_SECRET,
            Self::Inference => conveyor_actions::integrations::inference::INFERENCE_TOKEN_SECRET,
        }
    }

    /// The licences `app` key the mint is metered against.
    pub fn licences_app(self) -> &'static str {
        match self {
            Self::Laya => "laya",
            Self::Inference => "inference-router",
        }
    }
}

/// Where the token comes from.
pub enum TokenSource {
    /// Mint it through the licences API on the tenant's per-app licence.
    Mint,
    /// The tenant's own token (BYO), already read from stdin by the CLI.
    Byo(Vec<u8>),
}

/// What was written — never the token.
#[derive(Debug)]
pub struct Filled {
    /// The secret's name.
    pub secret: &'static str,
    /// `byo` (raw) or `licences` (minted).
    pub source: &'static str,
    /// The minted token's expiry, when the licences service named one.
    pub expires_at: Option<String>,
}

/// Seal the tenant's token for `service`.
///
/// A row that already exists is rotated — the old value does not survive, by design.
pub async fn fill(
    pool: &PgPool,
    licences: Option<&LicenceSyncClient>,
    tenant_slug: &str,
    service: UpstreamService,
    source: TokenSource,
) -> Result<Filled, String> {
    let tenant_id = store::tenants::id_by_slug(pool, tenant_slug)
        .await
        .map_err(|error| format!("read tenant {tenant_slug:?}: {error}"))?
        .ok_or_else(|| format!("no tenant {tenant_slug:?}"))?;

    let (value, source_name, expires_at) = match source {
        TokenSource::Mint => {
            let client = licences.ok_or_else(|| {
                "LICENCES_BASE_URL is not set; the mint needs the licences API".to_owned()
            })?;
            let minted = client
                .mint_bundle_token(tenant_id, service.licences_app(), 60)
                .await
                .map_err(|error| format!("licences mint: {error}"))?;
            let expires_at = minted.expires_at.clone();
            let envelope = json!({
                "token": minted.token,
                "expires_at": expires_at,
                "source": "licences",
                "licence_id": minted.license_id,
            });
            let bytes = serde_json::to_vec(&envelope)
                .map_err(|error| format!("encode the minted envelope: {error}"))?;
            (bytes, "licences", Some(expires_at))
        }
        TokenSource::Byo(raw) => {
            if raw.is_empty() {
                return Err("the token read from stdin is empty".to_owned());
            }
            (raw, "byo", None)
        }
    };

    let name = service.secret();
    let mut tx = store::begin_scoped(pool, tenant_id)
        .await
        .map_err(|error| format!("begin the tenant transaction: {error}"))?;
    let existing: Option<(Uuid, i64)> =
        sqlx::query_as("SELECT id, version FROM secret WHERE tenant_id = $1 AND name = $2")
            .bind(tenant_id.as_uuid())
            .bind(name)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| format!("read the {name} secret: {error}"))?;
    match existing {
        Some((id, version)) => {
            let version = u64::try_from(version)
                .ok()
                .and_then(|value| Version::new(value).ok())
                .ok_or_else(|| format!("the {name} secret carries an unusable version"))?;
            store::secret::rotate_secret_tx(&mut tx, tenant_id, id, version, &value)
                .await
                .map_err(|error| format!("rotate {name}: {error}"))?;
        }
        None => {
            store::secret::create_secret_tx(&mut tx, tenant_id, name, &value, "tenant")
                .await
                .map_err(|error| format!("create {name}: {error}"))?;
        }
    }
    tx.commit()
        .await
        .map_err(|error| format!("commit {name}: {error}"))?;
    Ok(Filled {
        secret: name,
        source: source_name,
        expires_at,
    })
}
