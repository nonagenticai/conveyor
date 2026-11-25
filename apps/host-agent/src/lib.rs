#![forbid(unsafe_code)]
//! `conveyor-host-agent` — the fixed-verb agent a bare-metal host answers with (T7, SPEC_V2
//! §10.4.1).
//!
//! The conveyor never holds a host credential. A pod reaches this agent over a key whose
//! `authorized_keys` line carries `restrict,command="/usr/local/bin/conveyor-host-agent …"`, so
//! the only thing the key can do is run THIS binary. The binary then answers three verbs and
//! nothing else:
//!
//! | verb | what it does |
//! |---|---|
//! | `status <unit>` | read the unit's state |
//! | `logs <unit> [--lines N]` | read a bounded tail of the unit's log |
//! | `restart <unit>` | restart the unit and read its state back |
//!
//! **The requested command is data, never a script.** [`parse_command`] splits ONE whitespace
//! separated line and matches it against the fixed set; there is no shell, no `-c`, no argument
//! that names a program. A forced-command key that asks for a shell passes an empty
//! `SSH_ORIGINAL_COMMAND`, and [`parse_command`] refuses it before any backend is consulted.
//!
//! **The unit allowlist is the host's own.** [`Config`] names the units this host may be asked
//! about; a unit outside it is refused here, independently of the conveyor's own registry check.
//! Two refusals, two authorities — neither trusts the other.
//!
//! **Two OS surfaces, one trait.** [`Backend`] with [`HostBackend`] dispatching on
//! [`Config::mechanism`]: `launchctl` on the macOS fleet, `systemctl` elsewhere. Tests drive a
//! recording backend, so the parsing and allowlist rules are proven without touching a host.

use std::fmt;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use serde_json::{Value, json};

/// The command succeeded.
pub const EXIT_OK: i32 = 0;
/// The agent refused the request; nothing was run.
pub const EXIT_REFUSED: i32 = 2;
/// The request was allowed but the OS surface failed.
pub const EXIT_BACKEND: i32 = 4;

/// The default log tail, in lines.
pub const DEFAULT_LINES: u32 = 200;
/// The longest tail a caller may ask for — a sample, never a download.
pub const MAX_LINES: u32 = 2_000;

/// One of the three verbs the agent answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    /// Read the unit's state.
    Status,
    /// Read a bounded tail of the unit's log.
    Logs,
    /// Restart the unit.
    Restart,
}

impl Verb {
    /// The spelling the client sends and the agent matches.
    ///
    /// ```
    /// use conveyor_host_agent::Verb;
    ///
    /// assert_eq!(Verb::Status.as_str(), "status");
    /// assert_eq!(Verb::Logs.as_str(), "logs");
    /// assert_eq!(Verb::Restart.as_str(), "restart");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Logs => "logs",
            Self::Restart => "restart",
        }
    }
}

/// A parsed, still-unvalidated request: the verb, the unit and the tail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    /// The verb asked for.
    pub verb: Verb,
    /// The unit named.
    pub unit: String,
    /// The tail, for [`Verb::Logs`] only.
    pub lines: u32,
}

/// Why the agent refused. Every variant is a refusal, never a partial run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The stable slug the client maps onto an action error.
    pub code: &'static str,
    /// The explanation.
    pub message: String,
}

impl Refusal {
    /// Build a refusal.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The JSON body the binary prints for this refusal.
    ///
    /// ```
    /// use conveyor_host_agent::Refusal;
    ///
    /// let body = Refusal::new("shell_refused", "a shell was requested").to_json();
    /// assert_eq!(body["error"]["code"], "shell_refused");
    /// assert_eq!(body["error"]["message"], "a shell was requested");
    /// ```
    pub fn to_json(&self) -> Value {
        json!({"error": {"code": self.code, "message": self.message}})
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Refusal {}

/// Parse `SSH_ORIGINAL_COMMAND` into one invocation.
///
/// The grammar is the whole surface:
///
/// ```text
/// status  <unit>
/// logs    <unit> [--lines <1..=2000>]
/// restart <unit>
/// ```
///
/// An empty command is a shell request (the forced-command key passes no command when a person
/// types `ssh host`), and it is refused: the agent runs nothing and exits non-zero.
///
/// ```
/// use conveyor_host_agent::{Verb, parse_command};
///
/// let call = parse_command("logs api --lines 50").expect("a well-formed call");
/// assert_eq!(call.verb, Verb::Logs);
/// assert_eq!(call.unit, "api");
/// assert_eq!(call.lines, 50);
///
/// // A bare shell request is a refusal, never a run.
/// assert!(parse_command("").is_err());
/// ```
pub fn parse_command(command: &str) -> Result<Invocation, Refusal> {
    let mut words = command.split_whitespace();
    let verb = match words.next() {
        Some("status") => Verb::Status,
        Some("logs") => Verb::Logs,
        Some("restart") => Verb::Restart,
        Some(other) => {
            return Err(Refusal::new(
                "unknown_verb",
                format!("{other:?} is not one of status, logs, restart"),
            ));
        }
        None => {
            return Err(Refusal::new(
                "shell_refused",
                "a shell was requested: the forced-command key answers status, logs and restart only",
            ));
        }
    };
    let unit = match words.next() {
        Some("") | None => {
            return Err(Refusal::new(
                "missing_unit",
                format!("{} needs a unit name", verb.as_str()),
            ));
        }
        Some(unit) => unit.to_owned(),
    };
    let mut lines = DEFAULT_LINES;
    let mut rest = words.peekable();
    while let Some(word) = rest.next() {
        match word {
            "--lines" if verb == Verb::Logs => {
                let value = rest
                    .next()
                    .ok_or_else(|| Refusal::new("missing_lines", "--lines needs a count"))?;
                let count: u32 = value.parse().map_err(|_| {
                    Refusal::new("invalid_lines", format!("{value:?} is not a line count"))
                })?;
                lines = count.clamp(1, MAX_LINES);
            }
            other => {
                return Err(Refusal::new(
                    "unknown_argument",
                    format!("{other:?} is not part of the fixed verb set"),
                ));
            }
        }
    }
    Ok(Invocation { verb, unit, lines })
}

/// Which OS surface answers.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mechanism {
    /// macOS, `launchctl`.
    Launchctl,
    /// Linux, `systemctl`.
    Systemctl,
}

impl Mechanism {
    /// The spelling the registry row and the agent's config share.
    ///
    /// ```
    /// use conveyor_host_agent::Mechanism;
    ///
    /// assert_eq!(Mechanism::Launchctl.as_str(), "launchctl");
    /// assert_eq!(Mechanism::Systemctl.as_str(), "systemctl");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Launchctl => "launchctl",
            Self::Systemctl => "systemctl",
        }
    }
}

/// One unit the host may be asked about.
#[derive(Clone, Debug, Deserialize)]
pub struct UnitConfig {
    /// The unit name the action sends (a launchd label or a systemd unit).
    pub name: String,
    /// Where the unit writes its log, when it is a file (`launchctl` only; `journalctl` reads
    /// systemd units without one).
    #[serde(default)]
    pub log_path: Option<String>,
}

/// The agent's own view of the host: which surface, which domain, which units.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// The OS surface.
    pub mechanism: Mechanism,
    /// The launchd domain (`gui/501`, `system`). Required for `launchctl`, ignored otherwise.
    #[serde(default)]
    pub domain: Option<String>,
    /// The allowlist. A unit outside it is refused before any backend call.
    pub units: Vec<UnitConfig>,
}

impl Config {
    /// Read the config a deployment placed on the host.
    pub fn from_path(path: &Path) -> Result<Self, Refusal> {
        let text = std::fs::read_to_string(path).map_err(|error| {
            Refusal::new("config_unreadable", format!("{}: {error}", path.display()))
        })?;
        let config: Self = serde_json::from_str(&text).map_err(|error| {
            Refusal::new("config_invalid", format!("{}: {error}", path.display()))
        })?;
        if config.mechanism == Mechanism::Launchctl && config.domain.is_none() {
            return Err(Refusal::new(
                "config_invalid",
                "a launchctl config must name a domain (e.g. gui/501)",
            ));
        }
        Ok(config)
    }

    /// The unit, when the host's allowlist carries it.
    pub fn unit(&self, name: &str) -> Result<&UnitConfig, Refusal> {
        self.units
            .iter()
            .find(|unit| unit.name == name)
            .ok_or_else(|| {
                Refusal::new(
                    "unit_not_allowed",
                    format!("{name:?} is not on this host's allowlist"),
                )
            })
    }
}

/// A unit's observed state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// The state word the surface reports (`running`, `active`, …).
    pub state: String,
    /// One line of evidence (a pid, a timestamp).
    pub detail: String,
}

/// The OS surface, behind one trait so the agent's rules are testable without a host.
pub trait Backend {
    /// Read the unit's state.
    fn status(&self, config: &Config, unit: &UnitConfig) -> Result<Status, Refusal>;
    /// Read a bounded tail of the unit's log.
    fn logs(&self, config: &Config, unit: &UnitConfig, lines: u32) -> Result<String, Refusal>;
    /// Restart the unit and read its state back.
    fn restart(&self, config: &Config, unit: &UnitConfig) -> Result<Status, Refusal>;
}

/// The real backend: `launchctl` or `systemctl`, chosen by the config.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostBackend;

impl HostBackend {
    fn run(program: &str, args: &[&str]) -> Result<String, Refusal> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|error| Refusal::new("backend_unavailable", format!("{program}: {error}")))?;
        if !output.status.success() {
            return Err(Refusal::new(
                "backend_failed",
                format!(
                    "{program} {} exited {}: {}",
                    args.join(" "),
                    output.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// The last `lines` lines of a file, bounded: a log read is a sample, never a download.
    fn tail_file(path: &str, lines: u32) -> Result<String, Refusal> {
        const CAP: u64 = 1 << 20;
        let text = std::fs::read(path)
            .map_err(|error| Refusal::new("log_unreadable", format!("{path}: {error}")))?;
        let text = if text.len() as u64 > CAP {
            String::from_utf8_lossy(&text[text.len() - CAP as usize..]).into_owned()
        } else {
            String::from_utf8_lossy(&text).into_owned()
        };
        let kept: Vec<&str> = text.lines().rev().take(lines as usize).collect();
        let mut ordered: Vec<&str> = kept.into_iter().rev().collect();
        let mut out = ordered.join("\n");
        ordered.clear();
        if !out.is_empty() {
            out.push('\n');
        }
        Ok(out)
    }

    fn launchctl_domain(config: &Config) -> Result<&str, Refusal> {
        config
            .domain
            .as_deref()
            .ok_or_else(|| Refusal::new("config_invalid", "a launchctl config must name a domain"))
    }

    /// `state = running` inside `launchctl print`'s output, plus the first `pid = N`.
    fn parse_launchctl(text: &str) -> Status {
        let mut state = None;
        let mut pid = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("state = ") {
                state = Some(value.trim().to_owned());
            } else if let Some(value) = line.strip_prefix("pid = ") {
                pid = Some(value.trim().to_owned());
            }
        }
        Status {
            state: state.unwrap_or_else(|| "unknown".to_owned()),
            detail: pid.map(|pid| format!("pid {pid}")).unwrap_or_default(),
        }
    }

    /// `systemctl show` emits `Key=Value` lines.
    fn parse_systemctl(text: &str) -> Status {
        let mut active = None;
        let mut sub = None;
        let mut pid = None;
        for line in text.lines() {
            let (key, value) = match line.split_once('=') {
                Some(pair) => pair,
                None => continue,
            };
            match key {
                "ActiveState" => active = Some(value.trim().to_owned()),
                "SubState" => sub = Some(value.trim().to_owned()),
                "MainPID" => pid = Some(value.trim().to_owned()),
                _ => {}
            }
        }
        let state = match (active, sub) {
            (Some(active), Some(sub)) if !sub.is_empty() && sub != active => {
                format!("{active} ({sub})")
            }
            (Some(active), _) => active,
            _ => "unknown".to_owned(),
        };
        Status {
            state,
            detail: pid
                .filter(|pid| pid != "0")
                .map(|pid| format!("pid {pid}"))
                .unwrap_or_default(),
        }
    }
}

impl Backend for HostBackend {
    fn status(&self, config: &Config, unit: &UnitConfig) -> Result<Status, Refusal> {
        match config.mechanism {
            Mechanism::Launchctl => {
                let domain = Self::launchctl_domain(config)?;
                let text = Self::run("launchctl", &["print", &format!("{domain}/{}", unit.name)])?;
                Ok(Self::parse_launchctl(&text))
            }
            Mechanism::Systemctl => {
                let text = Self::run(
                    "systemctl",
                    &[
                        "show",
                        &unit.name,
                        "--property=ActiveState,SubState,MainPID",
                    ],
                )?;
                Ok(Self::parse_systemctl(&text))
            }
        }
    }

    fn logs(&self, config: &Config, unit: &UnitConfig, lines: u32) -> Result<String, Refusal> {
        match config.mechanism {
            Mechanism::Launchctl => {
                let path = unit.log_path.as_deref().ok_or_else(|| {
                    Refusal::new(
                        "log_unconfigured",
                        format!("{} has no log_path in this host's config", unit.name),
                    )
                })?;
                Self::tail_file(path, lines)
            }
            Mechanism::Systemctl => {
                let count = lines.to_string();
                Self::run(
                    "journalctl",
                    &["-u", &unit.name, "-n", &count, "--no-pager"],
                )
            }
        }
    }

    fn restart(&self, config: &Config, unit: &UnitConfig) -> Result<Status, Refusal> {
        match config.mechanism {
            Mechanism::Launchctl => {
                let domain = Self::launchctl_domain(config)?;
                Self::run(
                    "launchctl",
                    &["kickstart", "-k", &format!("{domain}/{}", unit.name)],
                )?;
                self.status(config, unit)
            }
            Mechanism::Systemctl => {
                Self::run("systemctl", &["restart", &unit.name])?;
                self.status(config, unit)
            }
        }
    }
}

/// Answer one invocation: validate the allowlist, then hand it to the backend.
///
/// The JSON body is the client's contract; the exit code is the operator's.
pub fn run(
    config: &Config,
    backend: &dyn Backend,
    invocation: &Invocation,
) -> Result<Value, Refusal> {
    let unit = config.unit(&invocation.unit)?;
    match invocation.verb {
        Verb::Status => {
            let status = backend.status(config, unit)?;
            Ok(json!({
                "verb": Verb::Status.as_str(),
                "unit": unit.name,
                "mechanism": config.mechanism.as_str(),
                "state": status.state,
                "detail": status.detail,
            }))
        }
        Verb::Logs => {
            let log = backend.logs(config, unit, invocation.lines)?;
            Ok(json!({
                "verb": Verb::Logs.as_str(),
                "unit": unit.name,
                "mechanism": config.mechanism.as_str(),
                "lines": invocation.lines,
                "log": log,
            }))
        }
        Verb::Restart => {
            let status = backend.restart(config, unit)?;
            Ok(json!({
                "verb": Verb::Restart.as_str(),
                "unit": unit.name,
                "mechanism": config.mechanism.as_str(),
                "state": status.state,
                "detail": status.detail,
            }))
        }
    }
}

/// The whole flow the binary runs: parse the requested command, then answer it.
///
/// Split out so the "a shell request runs nothing" property is provable: the parse fails first,
/// so no backend is ever consulted.
pub fn answer(config: &Config, backend: &dyn Backend, command: &str) -> Result<Value, Refusal> {
    let invocation = parse_command(command)?;
    run(config, backend, &invocation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grammar_is_the_whole_surface() {
        let status = parse_command("status com.example.unit").expect("status parses");
        assert_eq!(status.verb, Verb::Status);
        assert_eq!(status.unit, "com.example.unit");
        assert_eq!(status.lines, DEFAULT_LINES);

        let logs = parse_command("logs com.example.unit --lines 50").expect("logs parses");
        assert_eq!(logs.verb, Verb::Logs);
        assert_eq!(logs.lines, 50);

        let restart = parse_command("restart com.example.unit").expect("restart parses");
        assert_eq!(restart.verb, Verb::Restart);
    }

    #[test]
    fn a_tail_is_clamped_into_the_bounded_range() {
        assert_eq!(
            parse_command("logs u --lines 999999")
                .expect("parses")
                .lines,
            MAX_LINES
        );
        assert_eq!(parse_command("logs u --lines 0").expect("parses").lines, 1);
    }

    #[test]
    fn extra_words_are_refused_rather_than_ignored() {
        let error = parse_command("status u extra").expect_err("refused");
        assert_eq!(error.code, "unknown_argument");
    }
}
