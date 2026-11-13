//! Service configuration from the environment.

use std::net::SocketAddr;

/// The minimum every service needs to start.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub service_name: &'static str,
    pub bind: SocketAddr,
    /// `None` only for a service that has no database.
    pub database_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("BIND_ADDR is not a socket address: {0}")]
    InvalidBind(String),
    #[error("DATABASE_URL is required")]
    MissingDatabaseUrl,
}

impl ServiceConfig {
    /// Read `BIND_ADDR` (default `0.0.0.0:<default_port>`) and `DATABASE_URL` (optional).
    pub fn from_env(service_name: &'static str, default_port: u16) -> Result<Self, ConfigError> {
        let raw = std::env::var("BIND_ADDR").unwrap_or_else(|_| format!("0.0.0.0:{default_port}"));
        let bind = raw
            .parse()
            .map_err(|_| ConfigError::InvalidBind(raw.clone()))?;
        Ok(Self {
            service_name,
            bind,
            database_url: std::env::var("DATABASE_URL").ok(),
        })
    }

    pub fn database_url(&self) -> Result<&str, ConfigError> {
        self.database_url
            .as_deref()
            .ok_or(ConfigError::MissingDatabaseUrl)
    }
}
