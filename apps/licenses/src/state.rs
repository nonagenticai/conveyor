//! The process state the HTTP handlers see.
//!
//! The pool is optional so a unit test can build the router without a database. The binary always
//! supplies one: the service refuses to start without its database, as the Python service does when
//! its boot migrations fail.
//!
//! `config` and `verifier` are `Arc`s because the auth middleware is cloned into every request by
//! `axum::middleware::from_fn_with_state`; a per-request deep copy would rebuild a JWKS cache.

use std::sync::Arc;

use sqlx::PgPool;

use crate::auth::JwtVerifier;
use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: Option<PgPool>,
    pub environment: String,
    pub version: String,
    pub config: Arc<Config>,
    pub verifier: Arc<JwtVerifier>,
}

impl AppState {
    pub fn new(pool: PgPool, config: Config, verifier: JwtVerifier) -> Self {
        let environment = config.environment.clone();
        let version = config.version.clone();
        Self {
            pool: Some(pool),
            environment,
            version,
            config: Arc::new(config),
            verifier: Arc::new(verifier),
        }
    }
}
