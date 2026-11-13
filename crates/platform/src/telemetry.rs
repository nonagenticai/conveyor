//! Tracing setup shared by every service.

/// Initialise the global subscriber from `RUST_LOG` (default `info`).
pub fn init(service_name: &'static str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
    tracing::info!(service = service_name, "service starting");
}
