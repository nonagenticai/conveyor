//! `conveyor-health` — a minimal liveness server built from the platform kit.
//!
//! It exists so the service-image template has a binary that needs no database: CI builds it
//! through `docker/Dockerfile.service` and asserts `/health` (see `.github/workflows/ci-public.yml`).
use conveyor_platform::{config::ServiceConfig, health};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServiceConfig::from_env("conveyor-health", 8080)?;
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, health::router(config.service_name)).await?;
    Ok(())
}
