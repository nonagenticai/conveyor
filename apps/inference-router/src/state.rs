//! Shared application state.

use std::sync::Arc;
use std::time::Duration;

use axum::http::HeaderMap;

use crate::auth::{self, CallerIdentity, Introspector};
use crate::config::Config;
use crate::routing::{Refusal, RouterService};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    router: Arc<RouterService>,
    introspector: Arc<Introspector>,
}

impl AppState {
    pub fn new(config: Config, router: RouterService) -> Self {
        let introspector = Introspector::new(
            config.introspect_url.clone(),
            Duration::from_secs_f64(config.introspect_timeout_seconds),
        );
        Self {
            config: Arc::new(config),
            router: Arc::new(router),
            introspector: Arc::new(introspector),
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
}
