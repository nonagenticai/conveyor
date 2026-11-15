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
use crate::billing::BillingTeardown;
use crate::billing::metering::MeteringProvisioner;
use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: Option<PgPool>,
    pub environment: String,
    pub version: String,
    pub config: Arc<Config>,
    pub verifier: Arc<JwtVerifier>,
    /// billing's mirror teardown. `None` when `METERING_SERVICE_URL` is unset — the reference
    /// warns and skips in that case too, and a revoke must never fail because of it.
    pub billing: Option<Arc<BillingTeardown>>,
    /// billing's metering provisioner. `None` when `METERING_SERVICE_URL` is unset or unusable:
    /// a licence is still created (the user-licence route or a bundle-ensure insert), its
    /// `metering_provisioned_at` stays NULL / the app is reported metering-pending, and the
    /// reconcile sweep repairs it. A metering failure never delays or changes the response.
    pub metering: Option<Arc<MeteringProvisioner>>,
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
            billing: None,
            metering: None,
        }
    }

    /// Attach the billing teardown client (tests and the binary's serve path).
    pub fn with_billing(mut self, billing: Arc<BillingTeardown>) -> Self {
        self.billing = Some(billing);
        self
    }

    /// Attach the metering provisioner (tests and the binary's serve path).
    pub fn with_metering(mut self, metering: Arc<MeteringProvisioner>) -> Self {
        self.metering = Some(metering);
        self
    }
}
