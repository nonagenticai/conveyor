//! SIGTERM (Kubernetes) or Ctrl-C — one implementation for `serve` and `worker`.
//!
//! The `worker` must also stop on SIGTERM: a rolling deploy that only implemented the
//! signal in `serve` would leave a worker polling until the grace period killed it, half-way
//! through a handler whose lease then had to expire instead of being finished.

/// Resolve when the process is asked to stop.
pub async fn signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = term => {},
    }
    tracing::info!("shutdown signal received");
}
