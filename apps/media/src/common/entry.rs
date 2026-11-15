//! The command line both images share.
//!
//! A media image is **not an HTTP service**: it executes one capability and exits. This binary is
//! therefore the local, testable form of what the runner contract will invoke —
//! `conveyor_domain::runner::Instruction` (issue #144) carries the same four things: a `kind`, its
//! `params`, named `inputs` and a closed set of `outputs`. Binding the real instruction is a
//! one-function change once that contract lands; nothing here assumes a route.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use serde_json::Value;

use crate::common::{CapabilityError, Config};
use crate::contract::{self, Capture};

/// The flags a media image accepts.
#[derive(Parser, Debug, Clone)]
pub struct Cli {
    /// Compare the kinds this image implements with the captured contracts and exit.
    #[arg(long)]
    pub check_lock: bool,

    /// Print the capability kinds this image implements and exit.
    #[arg(long)]
    pub list: bool,

    /// Execute one capability kind.
    #[arg(long)]
    pub run: Option<String>,

    /// The capability's parameters, as a JSON file (or `-` for stdin).
    #[arg(long)]
    pub params: Option<String>,

    /// Directory the capability writes its outputs into.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// A capability implementation: `(kind, params, config) -> result document`.
///
/// Boxed rather than an `async fn` pointer: the two images dispatch from a table, and a trait
/// object of `async fn` is not stable. The future owns its inputs so the table stays trivially
/// callable from both the CLI and, later, the runner loop.
pub type Dispatch =
    fn(String, Value, Config) -> futures_util::future::BoxFuture<'static, Result<Value, CapabilityError>>;

/// `--check-lock`: every captured kind implemented, and nothing implemented that no capture names.
pub fn check(captures: &[Capture], kinds: &[&str]) -> ExitCode {
    match contract::check(captures, kinds) {
        Ok(summary) => {
            println!(
                "lock: OK ({} capability kind(s) covered across {} captured services)",
                summary.kinds, summary.services
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("lock: DRIFTED — {error}");
            ExitCode::FAILURE
        }
    }
}

/// `--list`, one kind per line.
pub fn list(kinds: &[&str]) {
    for kind in kinds {
        println!("{kind}");
    }
}

/// `--run`: read the parameters, dispatch, print the result document.
pub async fn run(kind: &str, cli: &Cli, config: &Config, dispatch: Dispatch) -> ExitCode {
    let params = match read_params(cli.params.as_deref()) {
        Ok(params) => params,
        Err(error) => {
            eprintln!("{}: {error}", config.service);
            return ExitCode::FAILURE;
        }
    };
    let mut config = config.clone();
    if let Some(out) = &cli.out {
        config.out_dir = out.clone();
    }
    if let Err(error) = std::fs::create_dir_all(&config.out_dir) {
        eprintln!("{}: cannot create {:?}: {error}", config.service, config.out_dir);
        return ExitCode::FAILURE;
    }
    let service = config.service.clone();
    match dispatch(kind.to_string(), params, config).await {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{service}: {error}");
            ExitCode::FAILURE
        }
    }
}

fn read_params(path: Option<&str>) -> Result<Value, CapabilityError> {
    match path {
        None | Some("-") => {
            let mut raw = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw)
                .map_err(|e| CapabilityError::Invalid(format!("cannot read stdin: {e}")))?;
            serde_json::from_str(&raw)
                .map_err(|e| CapabilityError::Invalid(format!("params are not JSON: {e}")))
        }
        Some(path) => {
            let raw = std::fs::read_to_string(path)
                .map_err(|e| CapabilityError::Invalid(format!("cannot read {path}: {e}")))?;
            serde_json::from_str(&raw)
                .map_err(|e| CapabilityError::Invalid(format!("{path} is not JSON: {e}")))
        }
    }
}
