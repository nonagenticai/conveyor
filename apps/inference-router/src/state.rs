//! Shared application state.

use std::sync::Arc;
use std::time::Duration;

use axum::http::HeaderMap;
use sqlx::PgPool;

use crate::admin_auth::{AdminVerifier, production_source};
use crate::auth::{self, CallerIdentity, Introspector};
use crate::billing::{AllowAllGate, BudgetGate};
use crate::config::Config;
use crate::metering::{CostSink, NoopCostSink};
use crate::policy::PolicyStore;
use crate::routing::{Refusal, RouterService};

/// The request-path collaborators that talk to other services.
///
/// One field, not four: `AppState` is cloned per request by axum, so the whole bundle sits behind
/// a single `Arc`. `ledger` is `None` only in unit and health tests — `serve` always supplies the
/// pool it already refuses to boot without.
pub struct AppDeps {
    pub ledger: Option<PgPool>,
    pub budget: Arc<dyn BudgetGate>,
    pub cost: Arc<dyn CostSink>,
    /// No pool is `None` only in unit and health tests, where no tenant can have a policy.
    pub policy: Arc<PolicyStore>,
}

impl Default for AppDeps {
    fn default() -> Self {
        Self {
            ledger: None,
            budget: Arc::new(AllowAllGate),
            cost: Arc::new(NoopCostSink),
            policy: Arc::new(PolicyStore::new(None, std::time::Duration::from_secs(15))),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    router: Arc<RouterService>,
    introspector: Arc<Introspector>,
    deps: Arc<AppDeps>,
    admin: Arc<AdminVerifier>,
}

impl AppState {
    /// Test and health-test defaults: no pool, allow-all budget, no-op cost sink.
    pub fn new(config: Config, router: RouterService) -> Self {
        Self::with_deps(config, router, AppDeps::default())
    }

    /// The real constructor: every collaborator supplied by the caller.
    ///
    /// The admin verifier is built from the same config; tests that need a fixed key set use
    /// [`AppState::with_admin`] instead of mutating process-global environment.
    pub fn with_deps(config: Config, router: RouterService, deps: AppDeps) -> Self {
        let admin = Arc::new(AdminVerifier::from_config(&config, production_source()));
        Self::assemble(config, router, deps, admin)
    }

    /// A constructor with an injected admin verifier (integration tests).
    pub fn with_admin(
        config: Config,
        router: RouterService,
        deps: AppDeps,
        admin: Arc<AdminVerifier>,
    ) -> Self {
        Self::assemble(config, router, deps, admin)
    }

    fn assemble(
        config: Config,
        router: RouterService,
        deps: AppDeps,
        admin: Arc<AdminVerifier>,
    ) -> Self {
        let introspector = Introspector::new(
            config.introspect_url.clone(),
            Duration::from_secs_f64(config.introspect_timeout_seconds),
        );
        Self {
            config: Arc::new(config),
            router: Arc::new(router),
            introspector: Arc::new(introspector),
            deps: Arc::new(deps),
            admin,
        }
    }

    /// The completions gate: verify the caller against the licence server.
    pub async fn authorize(&self, headers: &HeaderMap) -> Result<CallerIdentity, Refusal> {
        auth::authorize(
            self.config.introspect_mode,
            &self.introspector,
            self.config.introspect_fail_open,
            headers,
        )
        .await
    }

    /// The caller AND the tenant the request is FOR. Every token-spending and tenant-scoped
    /// route resolves both here, so completions and the listings can never disagree.
    pub async fn caller(
        &self,
        headers: &HeaderMap,
    ) -> Result<(CallerIdentity, auth::EffectiveTenant), Refusal> {
        let identity = self.authorize(headers).await?;
        let effective = auth::effective_tenant(
            headers
                .get("x-effective-tenant-id")
                .and_then(|value| value.to_str().ok()),
            identity.subject_id.as_deref(),
            identity.tenant_id.as_deref(),
            &self.config.auth_delegating_subjects,
        );
        Ok((identity, effective))
    }

    /// `/api/whoami` asks the licence server directly: fresh, uncached, and without the app
    /// check (it REPORTS the app, and lets the caller judge it).
    pub fn introspector(&self) -> &Introspector {
        &self.introspector
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn router(&self) -> &RouterService {
        &self.router
    }

    pub fn deps(&self) -> &AppDeps {
        &self.deps
    }

    /// The `/api/admin/*` verifier. Every admin handler calls it before doing anything else.
    pub fn admin(&self) -> &AdminVerifier {
        &self.admin
    }
}
