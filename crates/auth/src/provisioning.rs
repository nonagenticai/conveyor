//! `POST /api/provision` — the ONE call that creates a tenant, its licence and its billing
//! quota.
//!
//! Contract (nonagenticai/provisioning, `src/schemas.py`):
//!
//! * `keycloak_user_id` is **required** and provisioning creates no Keycloak user — the
//!   caller owns the identity in its own realm and passes the id it minted. With that field
//!   present `workspace_role` must stay `member` and `send_password_reset_email` must be
//!   `false` (provisioning skips every Keycloak/email step), which this module hard-codes.
//! * `idempotency_key` is the caller's de-dup token: a re-POST with the same key returns the
//!   prior result (`replayed`) or resumes a partial run, never a duplicate tenant.
//! * `customer_ref` (`<source>:<id>`) is what tells two customers with the same lossy slug
//!   apart; a second customer colliding on a slug is refused 409, never merged.
//! * 201 fresh, 200 replay/resume; `status` is `success`, `succeeded_with_warnings` or
//!   `partial` (a MATERIAL best-effort step, e.g. the billing quota, failed).
//!
//! The key derives from `(email, slug)` rather than a random value, so a retry after a
//! crash IS the same operation — that is the whole reason there is no attempt ledger.

use conveyor_domain::{IdError, TenantId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::oauth::ClientCredentials;
use crate::service::{ServiceError, ServiceHttp};

/// The app key every provision mints a licence for.
pub const APP_KEY: &str = "conveyor";

/// The customer-ref source namespace (the pattern requires `<source>:<id>`).
pub const CUSTOMER_REF_SOURCE: &str = "conveyor";

/// What the Platform wizard sends.
#[derive(Debug, Clone, Serialize)]
pub struct ProvisionRequest {
    /// Deterministic de-dup key (see [`idempotency_key`]).
    pub idempotency_key: String,
    /// The first admin's verified email.
    pub email: String,
    /// The first admin's display name.
    pub name: String,
    /// Canonical tenant slug (`^[a-z0-9][a-z0-9-]*$`).
    pub tenant_slug: String,
    /// `conveyor:<slug>` — the non-lossy customer identity.
    pub customer_ref: String,
    /// Must be `member` when `keycloak_user_id` is present.
    pub workspace_role: &'static str,
    /// The app whose licence is minted.
    pub app_keys: Vec<&'static str>,
    /// Must be `false` when `keycloak_user_id` is present (provisioning sends no email).
    pub send_password_reset_email: bool,
    /// The Keycloak user the CALLER minted in its own realm.
    pub keycloak_user_id: Uuid,
}

impl ProvisionRequest {
    /// Build with the three constants the caller-owned-identity path requires.
    pub fn new(
        email: &str,
        name: &str,
        tenant_slug: &str,
        keycloak_user_id: Uuid,
    ) -> Result<Self, ServiceError> {
        let slug = tenant_slug.trim();
        if slug.is_empty() {
            return Err(ServiceError::Body {
                service: "provisioning",
                error: "tenant_slug is empty".into(),
            });
        }
        Ok(Self {
            idempotency_key: idempotency_key(email, slug),
            email: email.trim().to_owned(),
            name: name.trim().to_owned(),
            tenant_slug: slug.to_owned(),
            customer_ref: format!("{CUSTOMER_REF_SOURCE}:{slug}"),
            workspace_role: "member",
            app_keys: vec![APP_KEY],
            send_password_reset_email: false,
            keycloak_user_id,
        })
    }
}

/// One minted licence. `token` is returned ONCE and is `null` on an idempotent replay.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LicenseGrant {
    /// The app key.
    pub app_key: String,
    /// The licence id (always present).
    pub license_id: String,
    /// The raw licence token — present only on a fresh 201 / resume.
    #[serde(default)]
    pub token: Option<String>,
}

/// Provisioning's answer.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProvisionResult {
    /// The de-dup key echoed back.
    pub idempotency_key: String,
    /// `success` | `succeeded_with_warnings` | `partial`.
    pub status: String,
    /// The tenant id in the licences registry — adopted as the conveyor's own tenant id.
    pub tenant_id: Uuid,
    /// The Keycloak user id the caller supplied.
    pub keycloak_user_id: Uuid,
    /// True for a caller-owned identity. Informational.
    #[serde(default)]
    pub already_existed: bool,
    /// Never present on this path (provisioning sends no email).
    #[serde(default)]
    pub temporary_password: Option<String>,
    /// The licences minted.
    #[serde(default)]
    pub licenses: Vec<LicenseGrant>,
    /// Cosmetic best-effort failures.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// MATERIAL best-effort failures (e.g. the billing quota) — the tenant exists but is
    /// not fully wired; the UI must say so.
    #[serde(default)]
    pub partial_failures: Vec<String>,
    /// `true` when this was a replay of an already-successful run.
    #[serde(default)]
    pub replayed: bool,
}

impl ProvisionResult {
    /// The conveyor's tenant id (provisioning's, adopted). A nil id is malformed, not a
    /// tenant — the caller refuses it rather than creating an unreachable row.
    pub fn tenant(&self) -> Result<TenantId, IdError> {
        TenantId::from_uuid(self.tenant_id)
    }

    /// The licence minted for this app, if any.
    pub fn app_license(&self) -> Option<&LicenseGrant> {
        self.licenses
            .iter()
            .find(|licence| licence.app_key == APP_KEY)
    }

    /// True when the tenant is fully wired (no material failure).
    pub fn is_complete(&self) -> bool {
        self.partial_failures.is_empty()
    }
}

/// The deterministic idempotency key: `conveyor-provision-v1-<32 hex of sha256>`.
///
/// `\0` separates the two fields so `("ab","c")` and `("a","bc")` cannot collide. The
/// pattern provisioning accepts is `^[A-Za-z0-9._:-]+$` — hex and hyphens satisfy it.
pub fn idempotency_key(email: &str, tenant_slug: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(email.trim().to_ascii_lowercase().as_bytes());
    hasher.update([0u8]);
    hasher.update(tenant_slug.trim().as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    format!("conveyor-provision-v1-{}", &hex[..32])
}

/// The provisioning service.
#[derive(Clone)]
pub struct ProvisioningClient {
    http: ServiceHttp,
}

impl std::fmt::Debug for ProvisioningClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisioningClient")
            .field("http", &self.http)
            .finish()
    }
}

impl ProvisioningClient {
    /// Build. `base_url` is `PROVISIONING_BASE_URL`; `creds` is the `provision-caller`
    /// credential and is REQUIRED for this call.
    pub fn new(
        base_url: Option<&str>,
        creds: Option<ClientCredentials>,
    ) -> Result<Self, ServiceError> {
        if creds.is_none() {
            return Err(ServiceError::Unconfigured("SERVICE_CLIENT_SECRET"));
        }
        Ok(Self {
            http: ServiceHttp::new("provisioning", base_url, creds)?,
        })
    }

    /// `POST /api/provision`. 201 and 200 both deserialize; a 4xx is a refusal with the
    /// upstream's short detail (e.g. `tenant_name_taken`).
    pub async fn provision(
        &self,
        request: &ProvisionRequest,
    ) -> Result<ProvisionResult, ServiceError> {
        let body = serde_json::to_value(request).map_err(|error| ServiceError::Body {
            service: "provisioning",
            error: error.to_string(),
        })?;
        let (_status, value) = self.http.post("/api/provision", &body).await?;
        serde_json::from_value(value).map_err(|error| ServiceError::Body {
            service: "provisioning",
            error: error.to_string(),
        })
    }
}

/// A JSON object for tests and for the route's problem detail.
pub fn as_value(result: &ProvisionResult) -> Value {
    serde_json::to_value(result).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_deterministic_and_field_separated() {
        let a = idempotency_key("alice@example.com", "acme");
        let b = idempotency_key("Alice@Example.com ", "acme ");
        assert_eq!(a, b, "case and surrounding space must not fork the key");
        assert_ne!(a, idempotency_key("alice@example.com", "acme-2"));
        assert!(a.starts_with("conveyor-provision-v1-"));
        assert_eq!(a.len(), "conveyor-provision-v1-".len() + 32);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        );
        // The separator matters: these two pairs concatenate identically.
        assert_ne!(idempotency_key("ab", "c"), idempotency_key("a", "bc"));
    }

    #[test]
    fn the_request_carries_the_caller_owned_identity_constants() {
        let id = Uuid::now_v7();
        let request = ProvisionRequest::new("a@b.co", "Alice", "acme", id).expect("request");
        assert_eq!(request.workspace_role, "member");
        assert!(!request.send_password_reset_email);
        assert_eq!(request.app_keys, vec!["conveyor"]);
        assert_eq!(request.customer_ref, "conveyor:acme");
        assert_eq!(request.keycloak_user_id, id);
        assert_eq!(request.idempotency_key, idempotency_key("a@b.co", "acme"));
    }

    #[test]
    fn an_empty_slug_is_refused() {
        assert!(ProvisionRequest::new("a@b.co", "Alice", "  ", Uuid::now_v7()).is_err());
    }

    #[test]
    fn a_partial_result_is_not_complete() {
        let result = ProvisionResult {
            idempotency_key: "k".into(),
            status: "partial".into(),
            tenant_id: Uuid::now_v7(),
            keycloak_user_id: Uuid::now_v7(),
            already_existed: true,
            temporary_password: None,
            licenses: vec![LicenseGrant {
                app_key: "conveyor".into(),
                license_id: "l".into(),
                token: None,
            }],
            warnings: vec![],
            partial_failures: vec!["billing quota".into()],
            replayed: false,
        };
        assert!(!result.is_complete());
        assert!(result.app_license().is_some());
    }
}
