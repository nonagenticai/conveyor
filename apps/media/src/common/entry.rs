//! What the three binaries do identically: parse the flags, check the lock, print the document, or
//! serve. Only the router, the locks and the service name differ.

use std::process::ExitCode;

use axum::Router;
use clap::Parser;
use serde_json::{Value, json};

use crate::common::Config;
use crate::contract::{self, Lock};

/// The flags every `conveyor-media-*` binary accepts.
#[derive(Parser, Debug, Clone)]
pub struct Cli {
    /// Bind address (default 0.0.0.0).
    #[arg(long)]
    pub host: Option<String>,

    /// Bind port.
    #[arg(long)]
    pub port: Option<u16>,

    /// Compare the routed operations with the captured contracts and exit.
    #[arg(long)]
    pub check_lock: bool,

    /// Print the served OpenAPI document and exit.
    #[arg(long)]
    pub print_openapi: bool,
}

/// `--check-lock`: every captured operation routed, and no route the capture does not name.
pub fn check(locks: &[Lock], served: &[(&str, &str)]) -> ExitCode {
    match contract::check(locks, served) {
        Ok(summary) => {
            println!(
                "lock: OK ({} operations served across {} captured services)",
                summary.operations, summary.services
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("lock: DRIFTED — {error}");
            ExitCode::FAILURE
        }
    }
}

/// A minimal OpenAPI document describing exactly what this binary serves.
///
/// It is deliberately generated from `served`, not copied from a capture: this document answers
/// "what does the binary answer", and the capture answers "what must it answer".
pub fn openapi(service: &str, served: &[(&str, &str)]) -> Value {
    let mut paths = serde_json::Map::new();
    for (method, path) in served {
        let entry = paths
            .entry((*path).to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Some(map) = entry.as_object_mut() {
            map.insert(method.to_ascii_lowercase(), json!({"responses": {"200": {"description": "OK"}}}));
        }
    }
    json!({
        "openapi": "3.1.0",
        "info": {"title": service, "version": env!("CARGO_PKG_VERSION")},
        "paths": Value::Object(paths),
    })
}

/// Serve `router` on the configured address until the process is stopped.
pub async fn serve(config: &Config, router: Router) -> ExitCode {
    let address = format!("{}:{}", config.bind_host, config.bind_port);
    let listener = match tokio::net::TcpListener::bind(&address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("{}: cannot bind {address}: {error}", config.service);
            return ExitCode::FAILURE;
        }
    };
    eprintln!("{} listening on {address}", config.service);
    match axum::serve(listener, router).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}: {error}", config.service);
            ExitCode::FAILURE
        }
    }
}

/// Apply the `--host`/`--port` overrides to a config.
pub fn apply(config: &mut Config, cli: &Cli) {
    if let Some(host) = &cli.host {
        config.bind_host = host.clone();
    }
    if let Some(port) = cli.port {
        config.bind_port = port;
    }
}
