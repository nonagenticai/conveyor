#![forbid(unsafe_code)]
//! `conveyor-host` — the bare-metal host client (T7, SPEC_V2 §10.4.1).
//!
//! One transport for the `ops.host_*` family: an SSH session to a host's forced-command agent.
//! The **client holds no host credential in the sense that matters** — the key it uses is pinned
//! in the host's `authorized_keys` to `restrict,command="/usr/local/bin/conveyor-host-agent"`, so
//! the only thing the key can do is ask the agent one of three verbs. It cannot open a shell, run
//! a program or forward a port.
//!
//! **The destination comes from the registry, never from a caller.** [`HostTarget`] is built from
//! the `host_registry` row the action looked up; no input ever carries an address, a user or a
//! command. The three methods take a unit name and nothing else.
//!
//! **The host key is pinned from the row.** Before every call the client writes the row's
//! `host_key` into a `known_hosts` file under the state directory and passes
//! `StrictHostKeyChecking=yes`, so the connection is verified against the registry rather than
//! against whatever answered on the network.
//!
//! **Fail closed, visibly.** No key path ⇒ [`HostError::Unconfigured`] and no process is spawned.
//! A refusal from the agent keeps the agent's own code (`unit_not_allowed`, `shell_refused`, …),
//! so a caller can tell "the host said no" from "the host was unreachable".

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;

/// Where the private key lives when the deployment configured nothing else.
pub const DEFAULT_KEY_PATH: &str = "/etc/conveyor/ssh/id";

/// Where the per-call `known_hosts` is written.
pub const DEFAULT_STATE_DIR: &str = "/etc/conveyor/ssh";

/// The ssh binary. `ssh` on PATH unless a deployment pins an absolute path.
pub const DEFAULT_SSH_BIN: &str = "ssh";

/// The per-call deadline. A host that does not answer in this budget is unreachable, not slow.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// The largest output the client will read from the agent — a log tail is a sample, and an
/// agent that answers more than this is not answering the question that was asked.
const OUTPUT_CAP: usize = 4 << 20;

/// Why a host call did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// No key path: the client refuses before any process is spawned.
    #[error("the host agent client is not configured")]
    Unconfigured,
    /// The configuration or the pinned key cannot be used.
    #[error("host agent configuration: {0}")]
    Config(String),
    /// The agent refused the request and ran nothing. The code is the agent's own.
    #[error("the host agent refused ({code}): {message}")]
    Refused {
        /// The agent's refusal slug.
        code: String,
        /// The agent's explanation.
        message: String,
    },
    /// The allowance held but the OS surface failed on the host.
    #[error("the host agent failed: {0}")]
    Backend(String),
    /// The session did not reach the host, or its output did not parse.
    #[error("ssh transport: {0}")]
    Transport(String),
}

impl HostError {
    /// The refusal code, when the host said no.
    pub fn refusal_code(&self) -> Option<&str> {
        match self {
            Self::Refused { code, .. } => Some(code),
            _ => None,
        }
    }
}

/// Everything the client needs to open a session. Built from the environment in a pod.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostConfig {
    /// The private key whose public half is pinned on each host.
    pub key_path: PathBuf,
    /// The writable directory the per-call `known_hosts` is written into (an emptyDir; the
    /// container's root filesystem is read-only).
    pub state_dir: PathBuf,
    /// The ssh binary.
    pub ssh_bin: String,
    /// The per-call deadline.
    pub timeout: Duration,
}

impl HostConfig {
    /// Read the configuration from the process environment.
    pub fn from_env() -> Result<Self, HostError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Build from a lookup so the fail-closed path is testable without a pod.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, HostError> {
        let trimmed = |key: &str| {
            get(key)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let key_path = trimmed("CONVEYOR_HOST_AGENT_KEY").ok_or(HostError::Unconfigured)?;
        let state_dir = trimmed("CONVEYOR_HOST_AGENT_STATE_DIR")
            .unwrap_or_else(|| DEFAULT_STATE_DIR.to_owned());
        let ssh_bin =
            trimmed("CONVEYOR_HOST_AGENT_SSH_BIN").unwrap_or_else(|| DEFAULT_SSH_BIN.to_owned());
        let timeout = match trimmed("CONVEYOR_HOST_AGENT_TIMEOUT_MS") {
            None => DEFAULT_TIMEOUT,
            Some(text) => Duration::from_millis(
                text.parse()
                    .map_err(|_| HostError::Config(format!("timeout {text:?} is not a number")))?,
            ),
        };
        Ok(Self {
            key_path: PathBuf::from(key_path),
            state_dir: PathBuf::from(state_dir),
            ssh_bin,
            timeout,
        })
    }

    /// Build from explicit parts, for a caller that already parsed its environment or a test.
    pub fn from_parts(
        key_path: impl Into<PathBuf>,
        state_dir: impl Into<PathBuf>,
        ssh_bin: impl Into<String>,
    ) -> Self {
        Self {
            key_path: key_path.into(),
            state_dir: state_dir.into(),
            ssh_bin: ssh_bin.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

/// One registered host, as the action's registry lookup returns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTarget {
    /// The row's name, for the output.
    pub name: String,
    /// The address the session connects to.
    pub address: String,
    /// The unprivileged account the forced-command key logs in as.
    pub agent_user: String,
    /// The pinned host key, in `known_hosts` form after the address.
    pub host_key: String,
}

/// The host agent client.
#[derive(Clone, Debug)]
pub struct Client {
    config: HostConfig,
}

impl Client {
    /// Build from an explicit configuration.
    pub fn new(config: HostConfig) -> Self {
        Self { config }
    }

    /// Build from the process environment (the in-pod path).
    pub fn from_env() -> Result<Self, HostError> {
        Ok(Self::new(HostConfig::from_env()?))
    }

    /// The state directory the client writes its `known_hosts` into.
    pub fn state_dir(&self) -> &std::path::Path {
        &self.config.state_dir
    }

    /// `status <unit>` — the unit's state.
    pub async fn status(&self, target: &HostTarget, unit: &str) -> Result<Value, HostError> {
        self.call(target, &["status", unit]).await
    }

    /// `logs <unit> --lines N` — a bounded tail.
    pub async fn logs(
        &self,
        target: &HostTarget,
        unit: &str,
        lines: u32,
    ) -> Result<Value, HostError> {
        let lines = lines.to_string();
        self.call(target, &["logs", unit, "--lines", &lines]).await
    }

    /// `restart <unit>` — restart and read the state back, in one agent call.
    pub async fn restart(&self, target: &HostTarget, unit: &str) -> Result<Value, HostError> {
        self.call(target, &["restart", unit]).await
    }

    /// The argv the session runs, exposed so a test can assert the shape without a host.
    fn argv(&self, target: &HostTarget, args: &[&str], known_hosts: &str) -> Vec<String> {
        let connect_timeout = self.config.timeout.as_secs().max(1).to_string();
        let mut argv: Vec<String> = vec![
            "-F".to_owned(),
            "/dev/null".to_owned(),
            "-i".to_owned(),
            self.config.key_path.display().to_string(),
            "-o".to_owned(),
            "IdentitiesOnly=yes".to_owned(),
            "-o".to_owned(),
            "BatchMode=yes".to_owned(),
            "-o".to_owned(),
            "StrictHostKeyChecking=yes".to_owned(),
            "-o".to_owned(),
            format!("UserKnownHostsFile={known_hosts}"),
            "-o".to_owned(),
            format!("ConnectTimeout={connect_timeout}"),
            "-o".to_owned(),
            "LogLevel=ERROR".to_owned(),
            format!("{}@{}", target.agent_user, target.address),
        ];
        argv.extend(args.iter().map(|arg| (*arg).to_owned()));
        argv
    }

    /// The `known_hosts` line the registry row pins. One address, one key.
    fn known_hosts_line(target: &HostTarget) -> String {
        format!("{} {}\n", target.address, target.host_key)
    }

    async fn call(&self, target: &HostTarget, args: &[&str]) -> Result<Value, HostError> {
        // The registry's key is the authority; write it where ssh will read it. An unwritable
        // state directory is a configuration failure, never a silently unverified connection.
        std::fs::create_dir_all(&self.config.state_dir).map_err(|error| {
            HostError::Config(format!("{}: {error}", self.config.state_dir.display()))
        })?;
        let known_hosts = self.config.state_dir.join("known_hosts");
        std::fs::write(&known_hosts, Self::known_hosts_line(target))
            .map_err(|error| HostError::Config(format!("{}: {error}", known_hosts.display())))?;

        let argv = self.argv(target, args, &known_hosts.display().to_string());
        let mut command = tokio::process::Command::new(&self.config.ssh_bin);
        command
            .args(&argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = match tokio::time::timeout(self.config.timeout, command.output()).await {
            Err(_) => {
                return Err(HostError::Transport(format!(
                    "{} did not answer within {:?}",
                    target.name, self.config.timeout
                )));
            }
            Ok(Err(error)) => {
                return Err(HostError::Transport(format!(
                    "{}: {error}",
                    self.config.ssh_bin
                )));
            }
            Ok(Ok(output)) => output,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stdout = if stdout.len() > OUTPUT_CAP {
            &stdout[..OUTPUT_CAP]
        } else {
            &stdout
        };
        let parsed: Option<Value> = serde_json::from_str(stdout.trim()).ok();
        if output.status.success() {
            return parsed.ok_or_else(|| {
                HostError::Transport(format!(
                    "{} answered non-JSON: {}",
                    target.name,
                    stdout.trim()
                ))
            });
        }
        // A refusal or a backend failure: the agent prints its own code, and that code is what
        // the action surface must show — never a generic transport error.
        if let Some(error) = parsed.as_ref().and_then(|body| body.get("error")) {
            let code = error
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("agent_error")
                .to_owned();
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if code.starts_with("backend_") || code.starts_with("log_") {
                return Err(HostError::Backend(format!("{code}: {message}")));
            }
            return Err(HostError::Refused { code, message });
        }
        Err(HostError::Transport(format!(
            "{} exited {:?}: {}",
            target.name,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}
