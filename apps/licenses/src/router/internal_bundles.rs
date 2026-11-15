//! `POST /internal/tenants/{tenant_id}/bundle-token` — the per-app token mint.
//!
//! Ported from `main.py:11063-11393` (`BundleTokenResponse` and
//! `internal_mint_tenant_bundle_token`), against the pinned `4e8a08d`.
//!
//! A bundle is the provisioning and billing object; runtime usage presents the SPECIFIC per-app
//! licence from the tenant's bundle. This route therefore:
//!
//! 1. guards on the `bundle-token-minter` REALM role (never the fleet-wide internal roles — they
//!    would let any internal service mint for any tenant);
//! 2. scopes the caller's owner to the tenant it OWNS or holds an active `tenant_app_grant` on,
//!    with the app-scoped `tenant_dependency_grant` fallback and the audited cross-owner
//!    break-glass; an inaccessible tenant is 404 exactly like an absent one;
//! 3. resolves `app` inside the bundles the tenant HOLDS — as a member (the per-app licence
//!    carrying the bundle id) or through the `app_requires` closure (the tenant's ONE
//!    tenant-level licence for the dependency app);
//! 4. mints a fresh short-lived `api` token on that licence, expiring at
//!    `min(now + ttl_minutes, licence.expires_at)`, and returns the raw token exactly once.
//!
//! Fail-closed is the whole point: every refusal below says which link is missing and mints
//! nothing. The error detail is the ROUTE envelope (`{"detail": {…}}`), never the middleware's
//! `{"error": …}` — see `error.rs`.
//!
//! ⚠️ Deliberate difference, not observable at a route: `_ensure_owner_resolved` has no analogue.
//! `auth::attach_and_authorize` is the only place `owner_key` is attached and it already ran in
//! the middleware (the same argument `db/issuance.rs` records); a `User` built outside that path
//! carries its own `owner_key`.

use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::auth::{User, require_bundle_token_minter};
use crate::db::issuance::require_known_owner;
use crate::db::tokens::{hash_token, py_iso};
use crate::db::{bundles, dependencies, owners};
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// `BUNDLE_TOKEN_MAX_TTL_MINUTES` (the reference reuses `OPERATOR_TOKEN_MAX_TTL_MINUTES`).
pub const BUNDLE_TOKEN_MAX_TTL_MINUTES: i64 = 60;
/// The reference's `ttl_minutes` default.
pub const BUNDLE_TOKEN_DEFAULT_TTL_MINUTES: i64 = 15;

/// `SERVICE_SUBJECT_APP_KEY_MAX_LEN` = `SUBJECT_SUB_MAX_LEN - len("svc:") - 36 - 1` = 214.
///
/// A per-app bundle licence is held by `svc:<tenant>:<app>` and `subject.keycloak_sub` is
/// `VARCHAR(255)`; an app key past this can never have one.
pub const SERVICE_SUBJECT_APP_KEY_MAX_LEN: usize = 255 - "svc:".len() - 36 - 1;

/// The captured `BundleTokenResponse`, field for field.
#[derive(Debug, Serialize, ToSchema)]
pub struct BundleTokenResponse {
    /// The RAW token — returned exactly once; only its SHA-256 is stored.
    pub token: String,
    /// ISO-8601 (Python `datetime.isoformat()`), never later than the licence's own expiry.
    pub expires_at: String,
    pub tenant_id: String,
    pub bundle: String,
    pub app: String,
    pub license_id: String,
    pub token_id: String,
}

/// The query parameters, documented exactly as FastAPI publishes them.
///
/// Extraction is done by hand (a `BTreeMap`) so the refusals are FastAPI's 422 body rather than
/// axum's own rejection; this struct exists for the published contract.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BundleTokenQuery {
    /// REQUIRED. The app key the token is for. The token is minted on the tenant's per-app licence
    /// for this app (the one carrying the bundle's id), so it authorises — and meters against —
    /// exactly this app.
    #[param(min_length = 1, max_length = 255)]
    pub app: String,
    /// Lifetime in minutes (1..60, default 15).
    #[serde(default = "default_ttl_minutes")]
    #[param(default = 15, minimum = 1, maximum = 60)]
    pub ttl_minutes: i64,
    /// Bundle key to mint under. Optional unless several held bundles contain `app`
    /// (409 `ambiguous_bundle`).
    #[param(nullable)]
    pub bundle: Option<String>,
}

fn default_ttl_minutes() -> i64 {
    BUNDLE_TOKEN_DEFAULT_TTL_MINUTES
}

/// The validated query, after FastAPI-equivalent coercion.
struct MintQuery {
    app_key: String,
    ttl_minutes: i64,
    bundle: Option<String>,
}

fn validation_error(
    kind: &'static str,
    field: &'static str,
    message: &'static str,
    input: Value,
) -> Refusal {
    Refusal::detail(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!([{
            "type": kind,
            "loc": ["query", field],
            "msg": message,
            "input": input,
        }]),
    )
}

/// The reference's query validation (`Query(..., min_length=1, max_length=255, ge=1, le=60)`).
///
/// Unknown parameters are ignored, exactly as FastAPI ignores them.
fn validate_query(raw: &BTreeMap<String, String>) -> Result<MintQuery, Refusal> {
    let app_key = match raw.get("app") {
        None => {
            return Err(validation_error(
                "missing",
                "app",
                "Field required",
                Value::Null,
            ));
        }
        Some(value) if value.is_empty() => {
            return Err(validation_error(
                "string_too_short",
                "app",
                "String should have at least 1 character",
                json!(value),
            ));
        }
        Some(value) if value.chars().count() > 255 => {
            return Err(validation_error(
                "string_too_long",
                "app",
                "String should have at most 255 characters",
                json!(value),
            ));
        }
        Some(value) => value.clone(),
    };

    let ttl_minutes = match raw.get("ttl_minutes") {
        None => BUNDLE_TOKEN_DEFAULT_TTL_MINUTES,
        Some(value) => match value.parse::<i64>() {
            Ok(number) if number < 1 => {
                return Err(validation_error(
                    "greater_than_equal",
                    "ttl_minutes",
                    "Input should be greater than or equal to 1",
                    json!(number),
                ));
            }
            Ok(number) if number > BUNDLE_TOKEN_MAX_TTL_MINUTES => {
                return Err(validation_error(
                    "less_than_equal",
                    "ttl_minutes",
                    "Input should be less than or equal to 60",
                    json!(number),
                ));
            }
            Ok(number) => number,
            Err(_) => {
                return Err(validation_error(
                    "int_parsing",
                    "ttl_minutes",
                    "Input should be a valid integer, unable to parse string as an integer",
                    json!(value),
                ));
            }
        },
    };

    Ok(MintQuery {
        app_key,
        ttl_minutes,
        bundle: raw.get("bundle").cloned(),
    })
}

/// `service_subject_app_key_problem` — why `app_key` cannot name a tenant-level service subject.
fn service_subject_app_key_problem(app_key: &str) -> Option<String> {
    let length = app_key.chars().count();
    (length > SERVICE_SUBJECT_APP_KEY_MAX_LEN).then(|| {
        format!(
            "app key is {length} characters; a tenant-level licence subject \
             svc:<tenant-uuid>:<app> allows at most {SERVICE_SUBJECT_APP_KEY_MAX_LEN} \
             (subject.keycloak_sub is VARCHAR(255))"
        )
    })
}

/// The reference's `_refuse`: one detail object carrying `error`, `tenant_id`, `app`, `reason`
/// and any extra keys, plus the WARNING audit line.
#[allow(clippy::too_many_arguments)]
fn refuse(
    user: &User,
    status: StatusCode,
    error: &'static str,
    tenant_id: &str,
    app_key: &str,
    bundle: Option<&str>,
    reason: &str,
    extra: Vec<(&'static str, Value)>,
) -> Refusal {
    tracing::warn!(
        actor_sub = %user.sub,
        actor_azp = ?user.azp,
        tenant_id = %tenant_id,
        bundle = ?bundle,
        app = %app_key,
        error = %error,
        reason = %reason,
        "bundle_token_refused"
    );
    let mut detail = serde_json::Map::new();
    detail.insert("error".to_owned(), json!(error));
    detail.insert("tenant_id".to_owned(), json!(tenant_id));
    detail.insert("app".to_owned(), json!(app_key));
    detail.insert("reason".to_owned(), json!(reason));
    for (key, value) in extra {
        detail.insert(key.to_owned(), value);
    }
    Refusal::detail(status, Value::Object(detail))
}

fn mint_failed(error: impl std::fmt::Display) -> Refusal {
    tracing::error!(error = %error, "Failed to mint bundle token");
    Refusal::detail(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Failed to mint bundle token",
    )
}

/// Owner-scoped tenant resolution plus the app-scoped dependency-grant fallback.
///
/// Mirrors `_resolve_tenant_for_caller` (owner or active `tenant_app_grant`; cross-owner reaches
/// any tenant, audited) and its 404 branch in the handler (a `tenant_dependency_grant` admits the
/// apps it names; otherwise the tenant is indistinguishable from absent).
async fn resolve_tenant(
    pool: &sqlx::PgPool,
    user: &User,
    owner: &str,
    tenant_uuid: Uuid,
    tenant_key: &str,
    app_key: &str,
    bundle: Option<&str>,
) -> Result<bundles::TenantRow, Refusal> {
    let accessible = if user.cross_owner {
        true
    } else {
        owners::owner_can_access_tenant(pool, owner, tenant_key)
            .await
            .map_err(mint_failed)?
    };

    let tenant = if accessible {
        bundles::tenant(pool, tenant_uuid)
            .await
            .map_err(mint_failed)?
    } else {
        let scope = bundles::dependency_grant_apps(pool, tenant_uuid, owner)
            .await
            .map_err(mint_failed)?;
        if scope.is_empty() {
            return Err(refuse(
                user,
                StatusCode::NOT_FOUND,
                "no_bundle",
                tenant_key,
                app_key,
                bundle,
                "tenant_not_found",
                Vec::new(),
            ));
        }
        if !scope.iter().any(|granted| granted == app_key) {
            let mut granted_apps = scope;
            granted_apps.sort();
            return Err(refuse(
                user,
                StatusCode::FORBIDDEN,
                "dependency_grant_scope",
                tenant_key,
                app_key,
                bundle,
                "app_outside_dependency_grant",
                vec![("granted_apps", json!(granted_apps))],
            ));
        }
        bundles::tenant(pool, tenant_uuid)
            .await
            .map_err(mint_failed)?
    };

    let Some(tenant) = tenant else {
        return Err(refuse(
            user,
            StatusCode::NOT_FOUND,
            "no_bundle",
            tenant_key,
            app_key,
            bundle,
            "tenant_not_found",
            Vec::new(),
        ));
    };

    if user.cross_owner && tenant.owner_key != owner {
        tracing::warn!(
            action = "bundle_token_mint",
            tenant = %tenant_key,
            tenant_owner = %tenant.owner_key,
            caller_owner = %owner,
            azp = ?user.azp,
            realm = ?user.realm(),
            sub = %user.sub,
            at = %Utc::now().to_rfc3339(),
            "cross_owner_action"
        );
    }
    Ok(tenant)
}

/// One held bundle's resolution of `app`: `None` when the bundle does not contain it at all,
/// otherwise the (possibly empty) licence candidates.
async fn resolve_in_bundle(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    bundle: &bundles::HeldBundle,
    app_key: &str,
    edges: &[(String, String, String)],
    dependency_app: &mut Option<Option<bundles::AppRow>>,
    now: DateTime<Utc>,
) -> Result<Option<Vec<bundles::LicenceRow>>, Refusal> {
    let members = bundles::bundle_member_keys(pool, bundle.id)
        .await
        .map_err(mint_failed)?;

    if members.iter().any(|key| key == app_key) {
        return bundles::bundle_app_licences(pool, tenant_id, bundle.id, app_key, now)
            .await
            .map(Some)
            .map_err(mint_failed);
    }

    let deps = match dependencies::app_requires_deps(&members, edges) {
        Ok(deps) => deps,
        Err(path) => {
            // A cycle backs nothing — refused, never walked.
            tracing::warn!(
                tenant_id = %tenant_id,
                bundle = %bundle.key,
                app = %app_key,
                cycle = %path.join(" -> "),
                "bundle_token_dependency_cycle"
            );
            return Ok(None);
        }
    };
    if !deps.iter().any(|key| key == app_key) {
        return Ok(None);
    }

    if dependency_app.is_none() {
        *dependency_app = Some(
            bundles::app_by_key(pool, app_key)
                .await
                .map_err(mint_failed)?,
        );
    }
    match dependency_app.as_ref().expect("resolved above") {
        None => Ok(Some(Vec::new())),
        Some(app) if app.revoked => Ok(Some(Vec::new())),
        Some(app) => bundles::tenant_level_app_licences(pool, tenant_id, app.id, now)
            .await
            .map(Some)
            .map_err(mint_failed),
    }
}

/// `POST /internal/tenants/{tenant_id}/bundle-token`.
#[utoipa::path(
    post,
    path = "/internal/tenants/{tenant_id}/bundle-token",
    tag = "internal",
    operation_id = "internal_mint_tenant_bundle_token",
    summary = "Internal Mint Tenant Bundle Token",
    description = "Resolve (tenant, app) to a SHORT-LIVED token on that app's licence in a bundle the tenant holds. The raw token is returned once; only its SHA-256 is stored.",
    params(
        BundleTokenQuery,
        ("tenant_id" = String, Path, description = "Tenant identifier"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 201, body = BundleTokenResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn internal_mint_tenant_bundle_token(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(tenant_id): Path<String>,
    Query(raw): Query<BTreeMap<String, String>>,
) -> Result<(StatusCode, Json<BundleTokenResponse>), Refusal> {
    // FastAPI solves the role dependency before request validation, so a wrong role wins over a
    // malformed query.
    require_bundle_token_minter(&user)?;
    let query = validate_query(&raw)?;

    let tenant_uuid = Uuid::try_parse(&tenant_id)
        .map_err(|_| Refusal::detail(StatusCode::BAD_REQUEST, "Invalid tenant_id"))?;
    let tenant_key = tenant_uuid.to_string();
    let app_key = query.app_key.as_str();
    let requested_bundle = query.bundle.as_deref();

    // ⛔ Say "too long" rather than a misleading `no_app_licence` 404.
    if let Some(problem) = service_subject_app_key_problem(app_key) {
        return Err(refuse(
            &user,
            StatusCode::BAD_REQUEST,
            "app_key_too_long",
            &tenant_key,
            app_key,
            requested_bundle,
            &problem,
            Vec::new(),
        ));
    }

    let owner = require_known_owner(&user)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(mint_failed("no database connection"));
    };

    let now = Utc::now();
    let tenant = resolve_tenant(
        pool,
        &user,
        &owner,
        tenant_uuid,
        &tenant_key,
        app_key,
        requested_bundle,
    )
    .await?;
    if tenant.archived_at.is_some() {
        return Err(refuse(
            &user,
            StatusCode::NOT_FOUND,
            "no_bundle",
            &tenant_key,
            app_key,
            requested_bundle,
            "tenant_archived",
            Vec::new(),
        ));
    }

    let mut held = bundles::held_bundles(pool, tenant_uuid)
        .await
        .map_err(mint_failed)?;
    if let Some(requested) = requested_bundle {
        held.retain(|bundle| bundle.key == requested);
        if held.is_empty() {
            return Err(refuse(
                &user,
                StatusCode::NOT_FOUND,
                "no_bundle",
                &tenant_key,
                app_key,
                requested_bundle,
                "bundle_not_held",
                Vec::new(),
            ));
        }
    }
    if held.is_empty() {
        return Err(refuse(
            &user,
            StatusCode::NOT_FOUND,
            "no_bundle",
            &tenant_key,
            app_key,
            requested_bundle,
            "no_bundle_held",
            Vec::new(),
        ));
    }

    let edges = bundles::app_requirements(pool).await.map_err(mint_failed)?;
    let mut dependency_app: Option<Option<bundles::AppRow>> = None;
    let mut containing: Vec<(&bundles::HeldBundle, Vec<bundles::LicenceRow>)> = Vec::new();
    for held_bundle in &held {
        let licences = resolve_in_bundle(
            pool,
            tenant_uuid,
            held_bundle,
            app_key,
            &edges,
            &mut dependency_app,
            now,
        )
        .await?;
        if let Some(licences) = licences {
            containing.push((held_bundle, licences));
        }
    }

    if containing.is_empty() {
        let bundle_keys: Vec<String> = held.iter().map(|bundle| bundle.key.clone()).collect();
        return Err(refuse(
            &user,
            StatusCode::NOT_FOUND,
            "app_not_in_bundle",
            &tenant_key,
            app_key,
            requested_bundle,
            "app_not_in_any_held_bundle",
            vec![("bundles", json!(bundle_keys))],
        ));
    }

    if containing.len() > 1 {
        // Several held bundles reach `app`. When they all resolve to the SAME single licence that
        // licence is the answer; only genuinely different licences are ambiguous.
        let distinct: BTreeSet<Uuid> = containing
            .iter()
            .flat_map(|(_bundle, licences)| licences.iter().map(|licence| licence.id))
            .collect();
        if distinct.len() != 1 {
            let bundle_keys: Vec<String> = containing
                .iter()
                .map(|(bundle, _licences)| bundle.key.clone())
                .collect();
            return Err(refuse(
                &user,
                StatusCode::CONFLICT,
                "ambiguous_bundle",
                &tenant_key,
                app_key,
                requested_bundle,
                "app_in_several_held_bundles",
                vec![("bundles", json!(bundle_keys))],
            ));
        }
        containing = vec![
            containing
                .into_iter()
                .find(|(_bundle, licences)| !licences.is_empty())
                .expect("a distinct licence was found"),
        ];
    }
    let (held_bundle, licences) = containing
        .into_iter()
        .next()
        .expect("checked non-empty above");

    if licences.is_empty() {
        return Err(refuse(
            &user,
            StatusCode::NOT_FOUND,
            "no_app_licence",
            &tenant_key,
            app_key,
            requested_bundle,
            "no_active_per_app_licence_in_bundle",
            vec![("bundle", json!(held_bundle.key))],
        ));
    }
    if licences.len() > 1 {
        let license_ids: Vec<String> = licences
            .iter()
            .map(|licence| licence.id.to_string())
            .collect();
        return Err(refuse(
            &user,
            StatusCode::CONFLICT,
            "ambiguous_app_licence",
            &tenant_key,
            app_key,
            requested_bundle,
            "several_per_app_licences_in_bundle",
            vec![
                ("bundle", json!(held_bundle.key)),
                ("license_ids", json!(license_ids)),
            ],
        ));
    }
    let licence = &licences[0];

    // SHORT-LIVED: never past the licence's own expiry. `min(now + ttl, licence.expires_at)`.
    let mut expires_at = now + Duration::minutes(query.ttl_minutes);
    if let Some(licence_expiry) = licence.expires_at
        && licence_expiry < expires_at
    {
        expires_at = licence_expiry;
    }

    // `secrets.token_urlsafe(32)`: 32 random bytes, URL-safe base64, no padding (43 chars).
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(mint_failed)?;
    let raw_token = URL_SAFE_NO_PAD.encode(random);
    let token_id = bundles::create_api_token(pool, licence.id, &hash_token(&raw_token), expires_at)
        .await
        .map_err(mint_failed)?;

    tracing::warn!(
        actor_sub = %user.sub,
        actor = %user.preferred_username,
        actor_azp = ?user.azp,
        tenant_id = %tenant_key,
        bundle = %held_bundle.key,
        app = %app_key,
        license_id = %licence.id,
        token_id = %token_id,
        expires_at = %py_iso(expires_at),
        "bundle_token_minted"
    );

    Ok((
        StatusCode::CREATED,
        Json(BundleTokenResponse {
            token: raw_token,
            expires_at: py_iso(expires_at),
            tenant_id: tenant_key,
            bundle: held_bundle.key.clone(),
            app: query.app_key,
            license_id: licence.id.to_string(),
            token_id: token_id.to_string(),
        }),
    ))
}
