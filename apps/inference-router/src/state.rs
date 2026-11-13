//! Shared application state.

use std::sync::Arc;
use std::time::Duration;

use axum::http::HeaderMap;
use sqlx::PgPool;

use crate::auth::{self, CallerIdentity, Introspector};
use crate::billing::{AllowAllGate, BudgetGate};
use crate::config::Config;
use crate::metering::{CostSink, NoopCostSink};
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
}

impl Default for AppDeps {
    fn default() -> Self {
        Self {
            ledger: None,
            budget: Arc::new(AllowAllGate),
            cost: Arc::new(NoopCostSink),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    router: Arc<RouterService>,
    introspector: Arc<Introspector>,
    deps: Arc<AppDeps>,
}

impl AppState {
    /// Test and health-test defaults: no pool, allow-all budget, no-op cost sink.
    pub fn new(config: Config, router: RouterService) -> Self {
        Self::with_deps(config, router, AppDeps::default())
    }

    /// The real constructor: every collaborator supplied by the caller.
    pub fn with_deps(config: Config, router: RouterService, deps: AppDeps) -> Self {
        let introspector = Introspector::new(
            config.introspect_url.clone(),
            Duration::from_secs_f64(config.introspect_timeout_seconds),
        );
        Self {
            config: Arc::new(config),
            router: Arc::new(router),
            introspector: Arc::new(introspector),
            deps: Arc::new(deps),
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

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn router(&self) -> &RouterService {
        &self.router
    }

    pub fn deps(&self) -> &AppDeps {
        &self.deps
    }
}
