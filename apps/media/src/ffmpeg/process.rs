//! Running `ffmpeg`/`ffprobe` as a bounded child process.
//!
//! The image shells out; it does not link a decoder. What matters here is everything the shell-out
//! can get wrong: an argument list built from caller data must never reach a shell, a hung encode
//! must be killed rather than hold the job's deadline, and a failure must carry the tool's own last
//! words instead of a bare exit code.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::common::CapabilityError;

/// The wall-clock ceiling for one tool invocation. The job's own deadline is authoritative; this
/// bound exists so a wedged child cannot outlive it.
pub const TOOL_TIMEOUT: Duration = Duration::from_secs(300);

/// A finished invocation.
#[derive(Debug, Clone)]
pub struct Finished {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl Finished {
    pub fn ok(&self) -> bool {
        self.status.success()
    }

    /// The tool's last non-empty stderr lines — the part a human reads.
    pub fn tail(&self) -> String {
        let mut lines: Vec<&str> = self
            .stderr
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        if lines.len() > 6 {
            lines = lines.split_off(lines.len() - 6);
        }
        lines.join("\n")
    }
}

/// Run `tool` with `args`, no shell, stdin closed, output captured, bounded by [`TOOL_TIMEOUT`].
pub async fn run(tool: &'static str, args: &[String]) -> Result<Finished, CapabilityError> {
    let mut command = Command::new(tool);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let child = command.spawn().map_err(|source| CapabilityError::ToolMissing { tool, source })?;
    let output = match tokio::time::timeout(TOOL_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(CapabilityError::Failed(format!("{tool} could not be waited on: {error}")));
        }
        Err(_) => {
            return Err(CapabilityError::Failed(format!(
                "{tool} exceeded its {}s ceiling and was killed",
                TOOL_TIMEOUT.as_secs()
            )));
        }
    };
    Ok(Finished {
        status: output.status,
        stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Resolve a caller-named input inside `root`, refusing anything that escapes it.
///
/// Every path a job names is joined to the scratch directory and canonicalised, so a `..` segment
/// or an absolute path cannot reach a file the job was not given.
pub fn resolve_input(root: &Path, name: &str) -> Result<std::path::PathBuf, CapabilityError> {
    let joined = root.join(name);
    let canonical = joined.canonicalize().map_err(|error| {
        CapabilityError::Invalid(format!("input {name:?} is not readable: {error}"))
    })?;
    let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !canonical.starts_with(&root_canonical) {
        return Err(CapabilityError::Invalid(format!(
            "input {name:?} resolves outside the job's work directory"
        )));
    }
    Ok(canonical)
}
