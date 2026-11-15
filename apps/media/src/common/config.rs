//! Per-binary configuration. Every value comes from the environment; nothing is defaulted that
//! would make a misconfigured deploy look healthy.

/// The bind address, the port default and the service name a binary answers `/health` with.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind_host: String,
    pub bind_port: u16,
    /// The reference name this binary reports. Health scaffolding is re-expressed across the group
    /// (see each contract README), so the binary — not the captured service — is what it names.
    pub service: String,
    /// Postgres, when the binary needs one. `None` is a binary that ships without a database.
    pub database_url: Option<String>,
}

impl Config {
    pub fn from_env(service: &str, default_port: u16) -> Self {
        Self {
            bind_host: std::env::var("BIND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            bind_port: std::env::var("BIND_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(default_port),
            service: service.to_string(),
            database_url: std::env::var("DATABASE_URL").ok(),
        }
    }
}
