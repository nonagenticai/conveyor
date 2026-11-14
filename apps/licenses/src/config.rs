//! Deployment configuration, read once at startup.

use crate::{DEFAULT_ENVIRONMENT, DEFAULT_PORT, DEFAULT_VERSION};

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub environment: String,
    pub version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("PORT is not a port number: {0}")]
    Port(String),
    #[error("DATABASE_URL is not set")]
    DatabaseUrl,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let port = match std::env::var("PORT") {
            Ok(value) => value.parse::<u16>().map_err(|_| ConfigError::Port(value))?,
            Err(_) => DEFAULT_PORT,
        };
        Ok(Self {
            port,
            environment: std::env::var("ENVIRONMENT")
                .unwrap_or_else(|_| DEFAULT_ENVIRONMENT.to_owned()),
            version: std::env::var("VERSION").unwrap_or_else(|_| DEFAULT_VERSION.to_owned()),
        })
    }

    pub fn database_url(&self) -> Result<String, ConfigError> {
        std::env::var("DATABASE_URL").map_err(|_| ConfigError::DatabaseUrl)
    }

    pub fn bind(&self) -> String {
        format!("0.0.0.0:{}", self.port)
    }
}
