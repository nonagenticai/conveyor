//! Shared application state and the authenticator it holds.

use std::sync::Arc;

use axum::http::HeaderMap;
use conveyor_auth::{AuthConfig, AuthError, Session, Verifier};
use sqlx::PgPool;

use crate::config::Config;
use crate::metrics::Metrics;

/// Everything a handler may read. Cloned per request (all members are cheap handles).
#[derive(Clone)]
pub struct AppState {
    /// The runtime pool: `conveyor_runtime` through pgbouncer.
    pub pool: PgPool,
    /// The SSE doorbell's dedicated DIRECT connection (`DATABASE_LISTEN_URL`) — never the
    /// pooler. The pooler is transaction-pooled and cannot hold a session-scoped `LISTEN`;
    /// startup PROVES that with a two-connection NOTIFY round-trip, because a pooled URL
    /// connects successfully and only then fails to deliver.
    pub listen: conveyor_store::EventListen,
    pub config: Arc<Config>,
    pub auth: Arc<Authenticator>,
    /// The Platform/billing clients. `None` per service when unconfigured.
    pub services: crate::services::Services,
    pub metrics: Metrics,
}

impl AppState {
    /// Build the state for a serving process. The pool connects now, so a broken database
    /// is a startup failure rather than a first-request failure.
    pub async fn connect(config: Config) -> Result<Self, AppStateError> {
        let pool = conveyor_store::connect(&config.database_url).await?;
        // `connect` first round-trips a NOTIFY between two connections taken from this URL.
        // A pooled URL opens fine and then delivers nothing, so the probe — not the connect —
        // is what refuses a misconfigured `DATABASE_LISTEN_URL` at startup.
        let listen = conveyor_store::EventListen::connect(&config.database_listen_url)
            .await
            .map_err(AppStateError::Listen)?;
        let auth = Authenticator::new(config.auth.clone())?
            .with_licences(config.licences_base_url.as_deref(), &config.app_key)?;
        let (services, warnings) = crate::services::Services::from_config(&config);
        for warning in warnings {
            tracing::warn!(%warning, "external service not configured");
        }
        Ok(Self {
            pool,
            listen,
            config: Arc::new(config),
            auth: Arc::new(auth),
            services,
            metrics: Metrics::install(),
        })
    }
}

/// Startup failures while building [`AppState`].
#[derive(Debug, thiserror::Error)]
pub enum AppStateError {
    /// The database or pooler is unreachable.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// The LISTEN connection's startup probe failed — most often because
    /// `DATABASE_LISTEN_URL` points at the transaction-pooled PgBouncer, which cannot hold
    /// a session-scoped `LISTEN` and therefore delivers no notifications.
    #[error("listen: {0}")]
    Listen(#[from] conveyor_store::ListenError),
    /// The auth configuration cannot build a verifier.
    #[error("auth: {0}")]
    Auth(#[from] AuthError),
}

/// The dev/OIDC dispatcher. The verifier is built once at startup, so a misconfigured
/// issuer fails the process rather than every request.
pub struct Authenticator {
    cfg: AuthConfig,
    verifier: Option<Verifier>,
    machine: Option<conveyor_auth::MachineAuthenticator>,
}

impl Authenticator {
    /// Build. In dev mode no verifier exists, so a JWT can never be accepted.
    pub fn new(cfg: AuthConfig) -> Result<Self, AuthError> {
        let verifier = if cfg.dev_auth {
            None
        } else {
            Some(Verifier::from_config(&cfg)?)
        };
        Ok(Self {
            cfg,
            verifier,
            machine: None,
        })
    }

    /// Enable licence-bearer authentication (S06b). Absent a base URL, a non-JWT bearer is
    /// simply unauthenticated — it is never admitted by default. Dev auth keeps precedence:
    /// a licence is not a dev credential.
    pub fn with_licences(
        mut self,
        base_url: Option<&str>,
        app_key: &str,
    ) -> Result<Self, AuthError> {
        self.machine = match base_url {
            Some(base) if !self.cfg.dev_auth => {
                Some(conveyor_auth::MachineAuthenticator::new(base, app_key)?)
            }
            _ => None,
        };
        Ok(self)
    }

    /// The session for a request, or the refusal.
    pub async fn session(&self, pool: &PgPool, headers: &HeaderMap) -> Result<Session, AuthError> {
        conveyor_auth::authenticate(
            &self.cfg,
            self.verifier.as_ref(),
            self.machine.as_ref(),
            pool,
            headers,
        )
        .await
    }

    /// The caller's identity and memberships, with NO tenant selected.
    ///
    /// `/api/me` is the only route that needs this: the shell must draw the tenant switcher
    /// before a tenant is chosen, so a multi-membership caller naming no `X-Active-Tenant` gets
    /// a 200 here while every other endpoint still refuses.
    pub async fn identity_session(
        &self,
        pool: &PgPool,
        headers: &HeaderMap,
    ) -> Result<conveyor_auth::IdentitySession, AuthError> {
        conveyor_auth::identity_session(
            &self.cfg,
            self.verifier.as_ref(),
            self.machine.as_ref(),
            pool,
            headers,
        )
        .await
    }

    /// The caller's identity, keeping the SELECTED tenant when one was selected.
    ///
    /// `/api/me` needs the memberships always and the active tenant when there is one. The ONE
    /// case with no selection is a caller holding several memberships who names none — the
    /// bootstrap the switcher exists for. Every other request still refuses that caller.
    pub async fn identity_or_active(
        &self,
        pool: &PgPool,
        headers: &HeaderMap,
    ) -> Result<(conveyor_auth::IdentitySession, Option<Session>), AuthError> {
        match self.session(pool, headers).await {
            Ok(session) => {
                let identity = conveyor_auth::IdentitySession::from(&session);
                Ok((identity, Some(session)))
            }
            Err(AuthError::ActiveTenantRequired { .. }) => {
                let identity = self.identity_session(pool, headers).await?;
                Ok((identity, None))
            }
            Err(error) => Err(error),
        }
    }

    /// Whether dev auth is on (for startup logging and the S12 dev sign-in).
    pub fn dev_auth(&self) -> bool {
        self.cfg.dev_auth
    }
}
