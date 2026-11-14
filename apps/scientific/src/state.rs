//! The process state every handler shares.

use std::sync::Arc;
use std::time::Instant;

use crate::cache::Cache;
use crate::config::Config;

#[derive(Debug)]
pub struct AppState {
    pub config: Arc<Config>,
    pub cache: Arc<Cache>,
    pub started: Instant,
}

pub type Shared = Arc<AppState>;

impl AppState {
    pub fn new(config: Config) -> Self {
        let cache = Cache::new(config.cache_ttl, config.cache_max_entries);
        Self {
            config: Arc::new(config),
            cache: Arc::new(cache),
            started: Instant::now(),
        }
    }
}
