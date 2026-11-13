//! Shared application state.

use std::sync::Arc;

use crate::config::Config;
use crate::routing::RouterService;

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    router: Arc<RouterService>,
}

impl AppState {
    pub fn new(config: Config, router: RouterService) -> Self {
        Self {
            config: Arc::new(config),
            router: Arc::new(router),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn router(&self) -> &RouterService {
        &self.router
    }
}
