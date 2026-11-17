//! The command line both images share.
//!
//! A media image is **not an HTTP service**: it executes one capability and exits. Two ways in:
//!
//! * `--instruction <path>` — the pod mode. The file is one `conveyor_domain::runner::Instruction`
//!   (ADR 0007 §3–4); the capability's inputs, outputs and deadline come from it and the result is
//!   posted back through conveyor's job routes ([`crate::common::runner`]). `RUNNER_INSTRUCTION`
//!   is honoured as well, because that is the variable the Job spec sets, and the job token is
//!   read from `RUNNER_TOKEN` — never from argv.
//! * `--run <kind> --params <file>` — the local, testable form: read the parameters, dispatch, and
//!   print the result document. No network, no token.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use conveyor_domain::runner::{Instruction, Status};
use futures_util::future::BoxFuture;
use serde_json::Value;

use crate::common::{CapabilityError, Config, runner};
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

    /// Execute one `runner/v1` instruction read from this file. Also `RUNNER_INSTRUCTION`.
    #[arg(long, env = "RUNNER_INSTRUCTION", value_name = "PATH")]
    pub instruction: Option<PathBuf>,
}

/// A capability implementation: `(kind, params, config) -> result document`.
///
/// Boxed rather than an `async fn` pointer: the two images dispatch from a table, and a trait
/// object of `async fn` is not stable. The future owns its inputs so the table stays trivially
/// callable from both the CLI and the runner loop.
pub type DispatchFn =
    fn(String, Value, Config) -> BoxFuture<'static, Result<Value, CapabilityError>>;

/// One image's capability table: the kinds it implements and the function that runs them.
///
/// The kinds are part of the table because the runner must refuse a kind this image does not
/// implement **before** it fetches an input or starts a child process; a bare function pointer
/// cannot answer that question.
#[derive(Clone, Copy)]
pub struct Dispatch {
    pub kinds: &'static [&'static str],
    pub call: DispatchFn,
}

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

/// `--instruction` / `RUNNER_INSTRUCTION`: execute one runner/v1 instruction.
///
/// Exit 0 when the posted result is `succeeded`, 1 when it is `failed` — the pod's status is the
/// dispatcher's first signal; the posted result is the record.
pub async fn run_instruction(cli: &Cli, config: &Config, dispatch: Dispatch) -> ExitCode {
    let Some(path) = cli.instruction.as_deref() else {
        eprintln!("{}: --instruction needs a path", config.service);
        return ExitCode::FAILURE;
    };
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!(
                "{}: cannot read instruction {:?}: {error}",
                config.service, path
            );
            return ExitCode::FAILURE;
        }
    };
    let instruction: Instruction = match serde_json::from_str(&raw) {
        Ok(instruction) => instruction,
        Err(error) => {
            eprintln!(
                "{}: {:?} is not a runner/v1 instruction: {error}",
                config.service, path
            );
            return ExitCode::FAILURE;
        }
    };

    let result = runner::execute(instruction, config, dispatch).await;
    match result.status {
        Status::Succeeded => ExitCode::SUCCESS,
        Status::Failed => {
            if let Some(error) = &result.error {
                eprintln!(
                    "{}: {:?}/{}: {}",
                    config.service, error.class, error.code, error.message
                );
            }
            ExitCode::FAILURE
        }
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
        eprintln!(
            "{}: cannot create {:?}: {error}",
            config.service, config.out_dir
        );
        return ExitCode::FAILURE;
    }
    let service = config.service.clone();
    match (dispatch.call)(kind.to_string(), params, config).await {
        Ok(result) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).unwrap_or_default()
            );
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
