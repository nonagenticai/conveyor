//! Deployment configuration, read once at startup.
//!
//! `BILLING_SERVICE_URL` absent keeps the in-cluster default; present-but-blank is
//! "explicitly unconfigured" and every rollup answers `503 billing_unconfigured` — an
//! operator who blanked the variable meant to turn the read off, not to call `""`.
//! `BILLING_SERVICE_TOKEN` absent or blank means no `Authorization` header at all.

/// The in-cluster billing service.
pub const DEFAULT_BILLING_SERVICE_URL: &str = "http://billing-service:8157";

/// The shim's default listen port. Port 8000 is never used on this estate.
pub const DEFAULT_PORT: u16 = 8158;

#[derive(Debug, Clone)]
pub struct Config {
    /// `None` means explicitly unconfigured; rollups answer `503`.
    pub billing_service_url: Option<String>,
    /// Bearer for the billing service; `None` sends no `Authorization` header.
    pub billing_service_token: Option<String>,
    pub port: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("PORT is not a port number: {0}")]
    Port(String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let billing_service_url = match std::env::var("BILLING_SERVICE_URL") {
            Ok(raw) => {
                let raw = raw.trim();
                if raw.is_empty() {
                    None
                } else {
                    Some(raw.to_owned())
                }
            }
            Err(_) => Some(DEFAULT_BILLING_SERVICE_URL.to_owned()),
        };
        let billing_service_token = std::env::var("BILLING_SERVICE_TOKEN")
            .ok()
            .map(|raw| raw.trim().to_owned())
            .filter(|raw| !raw.is_empty());
        let port = match std::env::var("PORT") {
            Ok(raw) => raw
                .trim()
                .parse::<u16>()
                .map_err(|_| ConfigError::Port(raw))?,
            Err(_) => DEFAULT_PORT,
        };
        Ok(Self {
            billing_service_url,
            billing_service_token,
            port,
        })
    }
}
