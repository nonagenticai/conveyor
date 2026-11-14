//! Shared application state, cloned into every handler.

use std::sync::Arc;

use crate::admission::Gate;
use crate::auth::LicenceClient;
use crate::config::Config;
use crate::metrics::Metrics;
use crate::runner::LeanRunner;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub runner: LeanRunner,
    pub gate: Arc<Gate>,
    pub metrics: Arc<Metrics>,
    /// `None` only when `ENABLE_AUTH=false`; boot logs a warning in that case.
    pub licence: Option<Arc<LicenceClient>>,
}
