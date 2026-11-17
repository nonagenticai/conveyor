#![forbid(unsafe_code)]
//! `conveyor-host-agent` — the binary a hardened host's forced-command key runs.
//!
//! ```text
//! conveyor-host-agent /etc/conveyor/host-agent.json
//! ```
//!
//! It reads `SSH_ORIGINAL_COMMAND` (never its own argv beyond the config path), answers one of
//! `status`, `logs`, `restart`, and exits `0` on success, `2` on a refusal and `4` when the OS
//! surface failed. The JSON body goes to stdout in every case, so a client can map a refusal
//! without parsing stderr.

use std::path::Path;
use std::process::ExitCode;

use conveyor_host_agent::{Config, EXIT_BACKEND, EXIT_OK, EXIT_REFUSED, HostBackend, answer};

fn main() -> ExitCode {
    let path = match std::env::args().nth(1).or_else(|| {
        std::env::var("CONVEYOR_HOST_AGENT_CONFIG")
            .ok()
            .filter(|path| !path.trim().is_empty())
    }) {
        Some(path) => path,
        None => {
            let refusal = conveyor_host_agent::Refusal::new(
                "config_missing",
                "usage: conveyor-host-agent <config.json>",
            );
            eprintln!("conveyor-host-agent: {refusal}");
            println!("{}", refusal.to_json());
            return ExitCode::from(EXIT_REFUSED as u8);
        }
    };

    let config = match Config::from_path(Path::new(&path)) {
        Ok(config) => config,
        Err(refusal) => {
            eprintln!("conveyor-host-agent: {refusal}");
            println!("{}", refusal.to_json());
            return ExitCode::from(EXIT_REFUSED as u8);
        }
    };

    let command = std::env::var("SSH_ORIGINAL_COMMAND").unwrap_or_default();
    match answer(&config, &HostBackend, &command) {
        Ok(body) => {
            println!("{body}");
            ExitCode::from(EXIT_OK as u8)
        }
        Err(refusal) => {
            eprintln!("conveyor-host-agent refused: {refusal}");
            println!("{}", refusal.to_json());
            let backend = refusal.code.starts_with("backend_")
                || refusal.code == "log_unreadable"
                || refusal.code == "log_unconfigured";
            ExitCode::from(if backend {
                EXIT_BACKEND as u8
            } else {
                EXIT_REFUSED as u8
            })
        }
    }
}
