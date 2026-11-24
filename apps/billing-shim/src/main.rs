#![forbid(unsafe_code)]
//! Process entry point: read the environment, bind `0.0.0.0:$PORT` (default 8158), serve.

use std::net::SocketAddr;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use conveyor_billing_shim::config::Config;
use conveyor_billing_shim::router::{self, AppState};
use conveyor_billing_shim::upstream::BillingService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    let config = Config::from_env()?;
    let billing = BillingService::new(
        config.billing_service_url.as_deref(),
        config.billing_service_token.clone(),
    )?;
    if config.billing_service_url.is_none() {
        tracing::warn!(
            "BILLING_SERVICE_URL is not configured; POST /api/usage/rollup will answer 503"
        );
    }

    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "conveyor-billing-shim ready");

    axum::serve(listener, router::router(AppState { billing })).await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}
