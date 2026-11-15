//! Running an external tool (`ffmpeg`, `ffprobe`, a headless browser) as a bounded child process.
//!
//! Both media images shell out rather than link a decoder or a browser engine. What matters here is everything the shell-out
//! can get wrong: an argument list built from caller data must never reach a shell, a hung encode
//! must be killed rather than hold the job's deadline, and a failure must carry the tool's own last
//! words instead of a bare exit code.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::common::CapabilityError;

/// The wall-clock ceiling for one tool invocation. The job's own deadline is authoritative; this
/// bound exists so a wedged child cannot outlive it.
pub const TOOL_TIMEOUT: Duration = Duration::from_secs(300);

/// The image's own binary directories, tried when `PATH` is too narrow to reach a tool that is
/// installed on the host.
const TOOL_DIRS: [&str; 2] = ["/usr/bin", "/usr/local/bin"];

/// Where `tool` lives: the bare name when `PATH` can reach it, otherwise one of the image's own
/// binary directories. `None` is the "not installed" answer.
///
/// [`run`] and [`available`] share this resolution, so "the tool is there" and "the tool can be
/// spawned" never disagree when `PATH` is narrower than the host's layout.
fn resolve(tool: &str) -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(tool).is_file()))
        .unwrap_or(false);
    if on_path {
        return Some(PathBuf::from(tool));
    }
    TOOL_DIRS
        .into_iter()
        .map(|dir| Path::new(dir).join(tool))
        .find(|candidate| candidate.is_file())
}

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
pub async fn run(tool: &str, args: &[String]) -> Result<Finished, CapabilityError> {
    let program = resolve(tool).unwrap_or_else(|| PathBuf::from(tool));
    let mut command = Command::new(&program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let child = command
        .spawn()
        .map_err(|source| CapabilityError::ToolMissing {
            tool: tool.to_string(),
            source,
        })?;
    let output = match tokio::time::timeout(TOOL_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(CapabilityError::Failed(format!(
                "{tool} could not be waited on: {error}"
            )));
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

/// Whether `tool` is installed on this host: resolved on `PATH`, or present in one of the
/// image's own binary directories.
///
/// A case that reaches a child process uses this to skip when the host does not carry its tool,
/// rather than fail the suite over a missing third-party binary.
pub fn available(tool: &str) -> bool {
    resolve(tool).is_some()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The suite's own shell exists wherever the suite can run at all.
    #[test]
    fn a_tool_on_this_host_is_available() {
        assert!(
            available("sh"),
            "sh must resolve on any host that runs tests"
        );
    }

    #[test]
    fn a_name_that_is_not_installed_is_not_available() {
        assert!(!available("conveyor-no-such-tool"));
    }
}
