//! The external services the Platform surface calls, built once per process.
//!
//! A missing or malformed setting does NOT fail startup: `migrate`, the worker and every
//! tenant-scoped action still work without provisioning or billing. The affected route
//! answers `503` naming the variable (see [`conveyor_auth::ServiceError::Unconfigured`]),
//! which is a visible refusal in the UI, never a silent degradation to zeros.

use conveyor_auth::{BillingClient, KeycloakAdmin, LicenceSyncClient, ProvisioningClient};

use crate::config::Config;

/// The clients a serving process may use. Each is `None` when its configuration is absent.
#[derive(Clone)]
pub struct Services {
    /// `POST /api/provision` — the Platform wizard.
    pub provisioning: Option<ProvisioningClient>,
    /// Realm `conveyor` user administration.
    pub keycloak: Option<KeycloakAdmin>,
    /// Cost/usage reads for the billing panel.
    pub billing: Option<BillingClient>,
    /// The entitlement sync, used once after provisioning and by the worker's job.
    pub licence_sync: Option<LicenceSyncClient>,
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Services")
            .field("provisioning", &self.provisioning.is_some())
            .field("keycloak", &self.keycloak.is_some())
            .field("billing", &self.billing.is_some())
            .field("licence_sync", &self.licence_sync.is_some())
            .finish()
    }
}

impl Services {
    /// Build from configuration. Returns the services plus one warning per absent
    /// capability, for the caller to log (the messages never contain a secret value).
    pub fn from_config(config: &Config) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();

        let backend = match config.service_credentials() {
            Ok(creds) => Some(creds),
            Err(error) => {
                warnings.push(format!("service credential: {error}"));
                None
            }
        };

        let provisioning =
            match ProvisioningClient::new(config.provisioning_base_url.as_deref(), backend.clone())
            {
                Ok(client) => Some(client),
                Err(error) => {
                    warnings.push(format!("provisioning: {error}"));
                    None
                }
            };

        let keycloak = match config.keycloak_admin_credentials() {
            Ok(creds) => match KeycloakAdmin::new(
                config.keycloak_admin_url.as_deref(),
                config.keycloak_admin_realm.as_deref().unwrap_or("conveyor"),
                Some(creds),
            ) {
                Ok(client) => Some(client),
                Err(error) => {
                    warnings.push(format!("keycloak admin: {error}"));
                    None
                }
            },
            Err(error) => {
                warnings.push(format!("keycloak admin credential: {error}"));
                None
            }
        };

        let billing = match BillingClient::new(config.billing_base_url.as_deref(), backend.clone())
        {
            Ok(client) => Some(client),
            Err(error) => {
                warnings.push(format!("billing: {error}"));
                None
            }
        };

        // `/licenses/active` is protected: with no credential the sync is built only for a
        // local licences instance that has auth off (the dev shape), and the warning says so.
        let licence_sync = match config.licences_base_url.as_deref() {
            None => {
                warnings.push("licence sync: LICENCES_BASE_URL is not configured".into());
                None
            }
            Some(base) => match backend.clone() {
                Some(creds) => match LicenceSyncClient::with_credentials(base, creds) {
                    Ok(client) => Some(client),
                    Err(error) => {
                        warnings.push(format!("licence sync: {error}"));
                        None
                    }
                },
                None => match LicenceSyncClient::new(base) {
                    Ok(client) => {
                        warnings.push(
                            "licence sync: no S2S credential; /licenses/active will 401".into(),
                        );
                        Some(client)
                    }
                    Err(error) => {
                        warnings.push(format!("licence sync: {error}"));
                        None
                    }
                },
            },
        };

        (
            Self {
                provisioning,
                keycloak,
                billing,
                licence_sync,
            },
            warnings,
        )
    }
}
