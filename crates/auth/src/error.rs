//! Authentication failures. The HTTP layer maps each one to a status and a problem code;
//! the mapping lives in `conveyor-api`, not here, so this crate has no axum response code.

/// A refusal or failure while establishing who is calling.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// No credential, or a credential that is not a dev token in dev mode.
    #[error("authentication required")]
    Unauthenticated,
    /// The token's `iss` is not a trusted issuer. Refused before any JWKS fetch, so a caller
    /// cannot make this service fetch from an issuer of its choosing.
    #[error("untrusted token issuer")]
    UntrustedIssuer,
    /// A JWT-shaped token that failed verification. Never retried as a licence token.
    #[error("invalid token")]
    InvalidToken,
    /// The issuer's key set could not be fetched. Fail closed; a key rotation is not a
    /// reason to accept an unverified token.
    #[error("the issuer's key set is unavailable: {0}")]
    JwksUnavailable(String),
    /// A verified subject with no `membership` row.
    #[error("this principal has no tenant membership (found {count})")]
    NotProvisioned { count: usize },
    /// A person holding several memberships named none: the request cannot be scoped to one tenant.
    #[error("this principal holds {count} memberships; name one with X-Active-Tenant")]
    ActiveTenantRequired { count: usize },
    /// `X-Active-Tenant` names a tenant this principal has no membership in — or one the
    /// credential (a licence) does not name.
    #[error("this principal has no membership in the tenant named by X-Active-Tenant")]
    ActiveTenantNotPermitted,
    /// `X-Active-Tenant` is present but is not a single non-empty ASCII value.
    #[error("X-Active-Tenant is not a single non-empty value: {0}")]
    ActiveTenantInvalid(&'static str),
    /// `membership.role` holds a value this build does not know.
    #[error("unknown membership role {0:?}")]
    UnknownRole(String),
    /// `app_user.kind` holds a value this build does not know.
    #[error("unknown principal kind {0:?}")]
    UnknownPrincipalKind(String),
    /// The licences service could not answer an introspection. Fail closed: an outage is
    /// never an admission, and it is never reported as a refused credential.
    #[error("the licences service is unavailable: {0}")]
    LicenceUnavailable(String),
    /// The tenant exists but is not `active`.
    #[error("tenant is {status}")]
    TenantNotActive { status: String },
    /// The auth configuration cannot build a verifier.
    #[error("auth configuration: {0}")]
    Config(String),
    /// Resolving the identity failed.
    #[error("identity lookup: {0}")]
    Store(#[from] sqlx::Error),
}

impl AuthError {
    /// The stable problem `type` slug for this refusal.
    pub fn code(&self) -> &'static str {
        match self {
            AuthError::Unauthenticated
            | AuthError::UntrustedIssuer
            | AuthError::InvalidToken
            | AuthError::NotProvisioned { .. } => "unauthorized",
            AuthError::ActiveTenantRequired { .. } => "active_tenant_required",
            AuthError::ActiveTenantInvalid(_) => "active_tenant_invalid",
            AuthError::ActiveTenantNotPermitted => "active_tenant_not_permitted",
            AuthError::TenantNotActive { .. } => "forbidden",
            AuthError::JwksUnavailable(_)
            | AuthError::Config(_)
            | AuthError::LicenceUnavailable(_) => "service_unavailable",
            AuthError::UnknownRole(_)
            | AuthError::UnknownPrincipalKind(_)
            | AuthError::Store(_) => "internal_error",
        }
    }
}
