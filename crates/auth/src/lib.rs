#![forbid(unsafe_code)]
//! Authentication: OIDC/JWKS verification, the development principal, licence-bearer
//! (machine) authentication, and the resolution of a verified subject to the ONE
//! (app_user, tenant) pair a request acts in.
//!
//! The tenant never comes from a header (CONTRACT §9). It comes from `membership`, through
//! [`conveyor_store::resolve_identity`]. `TenantId` lives in `conveyor-domain` and is
//! re-exported here (S2-D1), so callers can spell it `conveyor_auth::TenantId`.

pub mod authn;
pub mod billing;
pub mod config;
pub mod dev;
pub mod error;
pub mod jwks;
pub mod keycloak_admin;
pub mod licence;
pub mod machine;
pub mod oauth;
pub mod principal;
pub mod provisioning;
pub mod service;

pub use authn::{IdentitySession, authenticate, bearer, identity_session};
pub use billing::BillingClient;
pub use config::{AuthConfig, AuthConfigError, DEFAULT_AUDIENCE, is_development_env};
pub use error::AuthError;
pub use jwks::{JWKS_MIN_REFRESH, JWKS_TTL, JwksCache, JwksSource, Verifier};
pub use keycloak_admin::{KeycloakAdmin, KeycloakUser};
pub use licence::{LICENCE_MAX_AGE_SECS, LicenceRefusal, LicenceSyncClient, SyncOutcome, Verdict};
pub use machine::{CACHE_TTL, MachineAuthenticator, MachineIdentity, is_jwt_shaped};
pub use oauth::{ClientCredentials, TokenError};
pub use principal::{
    ChannelAuthority, Membership, Principal, PrincipalKind, Role, ScopePolicy, Session,
};
pub use provisioning::{ProvisionRequest, ProvisionResult, ProvisioningClient};
pub use service::{ServiceError, ServiceHttp};

pub use conveyor_domain::TenantId;
