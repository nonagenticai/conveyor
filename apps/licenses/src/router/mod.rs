//! The HTTP surface.
//!
//! `ROUTES` is the single declaration of what this service exposes: `router()` builds axum from it
//! and `--check-lock` compares it to the captured contract. A route that exists in one and not the
//! other is a failure, so a handler cannot be added or dropped silently.
//!
//! The auth middleware is attached to the whole router and decides by path: public endpoints and
//! paths outside the reference's protected prefixes pass through untouched; everything else needs
//! a verified credential. Applying it here (rather than per route) is what makes "a new route is
//! protected by default" true.

pub mod catalogue_apps;
pub mod health;
pub mod internal_bundles;
pub mod internal_ensure;
pub mod internal_reads;
pub mod internal_seat;
pub mod licence_admin;
pub mod licence_tokens;
pub mod licenses;
pub mod observability;
pub mod schemas;
pub mod tenant_offerings;
pub mod tenants_crud;
pub mod tokens;
pub mod user_licenses;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, patch, post};

use crate::auth::{JwtVerifier, UnavailableJwks};
use crate::commands::reconcile_metering::MeteringReconcileResult;
use crate::config::Config;
use crate::state::AppState;

/// `(METHOD, path)` for every operation this service registers.
pub const ROUTES: &[(&str, &str)] = &[
    ("DELETE", "/apps/{app_id}"),
    ("GET", "/apps"),
    ("GET", "/apps/{app_id}"),
    ("GET", "/apps/{app_id}/licenses"),
    ("GET", "/apps/{app_id}/plans"),
    ("POST", "/apps"),
    ("GET", "/health"),
    ("GET", "/health/live"),
    ("GET", "/health/ready"),
    ("GET", "/internal/apps"),
    ("GET", "/internal/licenses"),
    ("GET", "/internal/licenses/{license_id}"),
    ("GET", "/internal/subjects/{subject_id}"),
    ("GET", "/internal/tenants"),
    ("GET", "/internal/tenants/{tenant_id}/seat-count"),
    ("POST", "/internal/tenants/{tenant_id}/bundle-token"),
    (
        "POST",
        "/internal/tenants/{tenant_id}/bundles/{bundle_key}/ensure",
    ),
    ("GET", "/licenses"),
    ("GET", "/licenses/active"),
    ("GET", "/licenses/admin/all"),
    ("POST", "/licenses/admin/metering/reconcile"),
    ("GET", "/licenses/user/{subject_id}"),
    ("POST", "/licenses/user/{subject_id}"),
    ("DELETE", "/licenses/user/{subject_id}"),
    ("PATCH", "/licenses/{license_id}/budget"),
    ("POST", "/license/introspect"),
    ("POST", "/license/revoke"),
    ("POST", "/licenses/{license_id}/tokens"),
    ("POST", "/licenses/{license_id}/operator-token"),
    ("POST", "/licenses/{license_id}/regenerate-token"),
    ("GET", "/tenants"),
    ("POST", "/tenants"),
    ("GET", "/tenants/{name_or_id}"),
    ("PATCH", "/tenants/{name_or_id}"),
    ("PUT", "/tenants/{tenant_id}"),
    ("PATCH", "/tenants/{name_or_id}/archive"),
    ("PUT", "/tenants/{name_or_id}/budget-currency"),
    ("PATCH", "/tenants/{name_or_id}/cost-centre"),
    ("GET", "/tenants/{name_or_id}/grants"),
    ("PUT", "/tenants/{name_or_id}/grants/{owner_key}"),
    ("DELETE", "/tenants/{name_or_id}/grants/{owner_key}"),
    ("GET", "/tenants/{name_or_id}/offerings"),
    ("POST", "/tenants/{name_or_id}/offerings/{app_key}/{code}"),
    ("DELETE", "/tenants/{name_or_id}/offerings/{app_key}/{code}"),
];

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        catalogue_apps::create_app,
        catalogue_apps::delete_app,
        catalogue_apps::get_app,
        catalogue_apps::list_app_licenses,
        catalogue_apps::list_app_plans,
        catalogue_apps::list_apps,
        health::health,
        health::liveness,
        health::readiness,
        internal_bundles::internal_mint_tenant_bundle_token,
        internal_ensure::internal_ensure_tenant_bundle,
        internal_reads::internal_get_license,
        internal_reads::internal_get_subject,
        internal_reads::internal_list_apps,
        internal_reads::internal_list_licenses,
        internal_reads::internal_list_tenants,
        internal_seat::internal_tenant_seat_count,
        licence_admin::list_admin_licenses,
        licence_admin::list_licenses,
        licence_admin::reconcile_metering_provisioning,
        licence_admin::set_license_budget,
        licence_tokens::create_license_token,
        licence_tokens::create_operator_token,
        licence_tokens::regenerate_license_token,
        licenses::active,
        tokens::introspect,
        tokens::revoke,
        tenants_crud::create_tenant,
        tenants_crud::get_tenant,
        tenants_crud::list_tenants,
        tenants_crud::register_tenant,
        tenants_crud::rename_tenant,
        tenants_crud::set_tenant_archived,
        tenants_crud::set_tenant_budget_currency,
        tenants_crud::set_tenant_cost_centre,
        tenant_offerings::delete_tenant_grant,
        tenant_offerings::list_tenant_grants,
        tenant_offerings::list_tenant_offerings,
        tenant_offerings::provision_tenant_offering,
        tenant_offerings::put_tenant_grant,
        tenant_offerings::unprovision_tenant_offering,
        user_licenses::create_user_license,
        user_licenses::delete_user_licenses,
        user_licenses::list_user_licenses
    ),
    components(schemas(
        catalogue_apps::AppCreate,
        catalogue_apps::AppResponse,
        catalogue_apps::PlanResponse,
        health::ErrorResponse,
        health::HealthResponse,
        health::LivenessResponse,
        health::ReadinessResponse,
        internal_bundles::BundleTokenResponse,
        internal_ensure::BundleEnsureApp,
        internal_ensure::BundleEnsureRefusal,
        internal_ensure::BundleEnsureRequest,
        internal_ensure::BundleEnsureResponse,
        internal_reads::InternalAppResponse,
        internal_reads::InternalLicenseResponse,
        internal_reads::InternalSubjectResponse,
        internal_reads::InternalTenantResponse,
        internal_seat::InternalSeatCountResponse,
        licence_admin::LicenseBudgetUpdate,
        licence_tokens::OperatorTokenResponse,
        licence_tokens::RegenerateTokenResponse,
        licence_tokens::TokenCreateResponse,
        licence_tokens::TokenType,
        MeteringReconcileResult,
        schemas::HTTPValidationError,
        schemas::ValidationError,
        tenant_offerings::TenantGrantResponse,
        tenant_offerings::TenantOfferingProvisionResponse,
        tenant_offerings::TenantOfferingRecord,
        tenant_offerings::TenantOfferingUnprovisionResponse,
        tenants_crud::TenantArchiveUpdate,
        tenants_crud::TenantBudgetCurrencyUpdate,
        tenants_crud::TenantCostCentreUpdate,
        tenants_crud::TenantCreate,
        tenants_crud::TenantRegister,
        tenants_crud::TenantRename,
        tenants_crud::TenantResponse,
        tokens::TokenIntrospectRequest,
        tokens::TokenIntrospectResponse,
        tokens::TokenRevokeRequest,
        tokens::TokenRevokeResponse,
        user_licenses::AppInfo,
        user_licenses::DeleteLicensesResponse,
        user_licenses::LicenseCreate,
        user_licenses::LicenseResponse,
        user_licenses::PlanInfo
    )),
    tags(
        (name = "health", description = "Health and readiness probes"),
        (name = "licenses", description = "Licenses and entitlements"),
        (name = "tenants", description = "Tenant registry"),
        (name = "tokens", description = "API token introspection and revocation")
    )
)]
pub struct ApiDoc;

/// The document the service publishes (and `--check-lock` compares to the capture).
pub fn openapi() -> utoipa::openapi::OpenApi {
    <ApiDoc as utoipa::OpenApi>::openapi()
}

/// The HTTP application the binary serves.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route(
            "/apps",
            get(catalogue_apps::list_apps).post(catalogue_apps::create_app),
        )
        .route(
            "/apps/{app_id}",
            get(catalogue_apps::get_app).delete(catalogue_apps::delete_app),
        )
        .route(
            "/apps/{app_id}/licenses",
            get(catalogue_apps::list_app_licenses),
        )
        .route("/apps/{app_id}/plans", get(catalogue_apps::list_app_plans))
        .route("/health", get(health::health))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .route("/internal/apps", get(internal_reads::internal_list_apps))
        .route(
            "/internal/licenses",
            get(internal_reads::internal_list_licenses),
        )
        .route(
            "/internal/licenses/{license_id}",
            get(internal_reads::internal_get_license),
        )
        .route(
            "/internal/subjects/{subject_id}",
            get(internal_reads::internal_get_subject),
        )
        .route(
            "/internal/tenants",
            get(internal_reads::internal_list_tenants),
        )
        .route(
            "/internal/tenants/{tenant_id}/seat-count",
            get(internal_seat::internal_tenant_seat_count),
        )
        .route(
            "/internal/tenants/{tenant_id}/bundle-token",
            post(internal_bundles::internal_mint_tenant_bundle_token),
        )
        .route(
            "/internal/tenants/{tenant_id}/bundles/{bundle_key}/ensure",
            post(internal_ensure::internal_ensure_tenant_bundle),
        )
        .route("/licenses", get(licence_admin::list_licenses))
        .route(
            "/licenses/admin/all",
            get(licence_admin::list_admin_licenses),
        )
        .route(
            "/licenses/admin/metering/reconcile",
            post(licence_admin::reconcile_metering_provisioning),
        )
        .route("/licenses/active", get(licenses::active))
        .route(
            "/licenses/user/{subject_id}",
            get(user_licenses::list_user_licenses)
                .post(user_licenses::create_user_license)
                .delete(user_licenses::delete_user_licenses),
        )
        .route(
            "/licenses/{license_id}/budget",
            patch(licence_admin::set_license_budget),
        )
        .route("/license/introspect", post(tokens::introspect))
        .route("/license/revoke", post(tokens::revoke))
        .route(
            "/licenses/{license_id}/tokens",
            post(licence_tokens::create_license_token),
        )
        .route(
            "/licenses/{license_id}/operator-token",
            post(licence_tokens::create_operator_token),
        )
        .route(
            "/licenses/{license_id}/regenerate-token",
            post(licence_tokens::regenerate_license_token),
        )
        .route(
            "/tenants",
            get(tenants_crud::list_tenants).post(tenants_crud::create_tenant),
        )
        // `/tenants/{name_or_id}` and `/tenants/{tenant_id}` are the SAME actual selector:
        // the capture spells the PUT's parameter `tenant_id`, and axum refuses two wildcard
        // NAMES at one path position, so the three methods share the one route. The
        // documented path stays the capture's, in `ROUTES` and in the utoipa annotation.
        .route(
            "/tenants/{name_or_id}",
            get(tenants_crud::get_tenant)
                .patch(tenants_crud::rename_tenant)
                .put(tenants_crud::register_tenant),
        )
        .route(
            "/tenants/{name_or_id}/archive",
            patch(tenants_crud::set_tenant_archived),
        )
        .route(
            "/tenants/{name_or_id}/budget-currency",
            axum::routing::put(tenants_crud::set_tenant_budget_currency),
        )
        .route(
            "/tenants/{name_or_id}/cost-centre",
            patch(tenants_crud::set_tenant_cost_centre),
        )
        .route(
            "/tenants/{name_or_id}/grants",
            get(tenant_offerings::list_tenant_grants),
        )
        .route(
            "/tenants/{name_or_id}/grants/{owner_key}",
            axum::routing::put(tenant_offerings::put_tenant_grant)
                .delete(tenant_offerings::delete_tenant_grant),
        )
        .route(
            "/tenants/{name_or_id}/offerings",
            get(tenant_offerings::list_tenant_offerings),
        )
        .route(
            "/tenants/{name_or_id}/offerings/{app_key}/{code}",
            post(tenant_offerings::provision_tenant_offering)
                .delete(tenant_offerings::unprovision_tenant_offering),
        )
        // The live service also answers `/licenses/health` (a gateway-prefix alias it registers
        // with `include_in_schema=False`), so it is registered here but is not in `ROUTES`.
        .route("/licenses/health", get(health::health))
        // `/metrics` is a Prometheus scrape endpoint, not a documented JSON operation: no
        // `ROUTES` entry, no `OpenApi` path. `--check-lock` must not see it.
        .route("/metrics", get(observability::metrics))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::auth::authenticate,
        ))
}

/// A state with no database and no trusted issuer, for tests that exercise routing/health.
pub fn test_state() -> AppState {
    AppState {
        pool: None,
        environment: "test".to_owned(),
        version: "0.0.0".to_owned(),
        config: Arc::new(Config {
            port: 0,
            environment: "test".to_owned(),
            version: "0.0.0".to_owned(),
            trusted_issuers: Vec::new(),
            introspect_require_auth: false,
            service_actor_id: None,
        }),
        verifier: Arc::new(JwtVerifier::new(Vec::new(), Arc::new(UnavailableJwks))),
        billing: None,
        metering: None,
    }
}
