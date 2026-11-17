//! #172 P1 — the agent's three rules: an unknown verb runs nothing, a unit outside the host's
//! row runs nothing, and a shell request runs nothing. The backend in the refusal tests panics
//! if it is ever consulted, which is what makes "runs nothing" an assertion rather than a claim.

use std::cell::RefCell;

use conveyor_host_agent::{
    Backend, Config, Mechanism, Refusal, Status, UnitConfig, answer, parse_command,
};

fn config() -> Config {
    Config {
        mechanism: Mechanism::Systemctl,
        domain: None,
        units: vec![UnitConfig {
            name: "vllm.service".to_owned(),
            log_path: None,
        }],
    }
}

/// A backend that fails the test if the agent touches it — the proof that a refusal is decided
/// before any OS command exists.
struct PanicBackend;

impl Backend for PanicBackend {
    fn status(&self, _: &Config, _: &UnitConfig) -> Result<Status, Refusal> {
        panic!("the backend was consulted for a request that must be refused");
    }
    fn logs(&self, _: &Config, _: &UnitConfig, _: u32) -> Result<String, Refusal> {
        panic!("the backend was consulted for a request that must be refused");
    }
    fn restart(&self, _: &Config, _: &UnitConfig) -> Result<Status, Refusal> {
        panic!("the backend was consulted for a request that must be refused");
    }
}

/// A backend that records what was asked and answers a fixed state.
#[derive(Default)]
struct Recording {
    calls: RefCell<Vec<String>>,
}

impl Backend for Recording {
    fn status(&self, _: &Config, unit: &UnitConfig) -> Result<Status, Refusal> {
        self.calls
            .borrow_mut()
            .push(format!("status {}", unit.name));
        Ok(Status {
            state: "active".to_owned(),
            detail: "pid 42".to_owned(),
        })
    }
    fn logs(&self, _: &Config, unit: &UnitConfig, lines: u32) -> Result<String, Refusal> {
        self.calls
            .borrow_mut()
            .push(format!("logs {} {lines}", unit.name));
        Ok("one\ntwo\n".to_owned())
    }
    fn restart(&self, _: &Config, unit: &UnitConfig) -> Result<Status, Refusal> {
        self.calls
            .borrow_mut()
            .push(format!("restart {}", unit.name));
        Ok(Status {
            state: "active".to_owned(),
            detail: "pid 43".to_owned(),
        })
    }
}

/// P1.1 — a verb outside the set is refused.
#[test]
fn an_unknown_verb_is_refused() {
    let error = parse_command("systemctl restart vllm.service")
        .expect_err("only status, logs and restart exist");
    assert_eq!(error.code, "unknown_verb", "{error}");
    assert!(
        error.message.contains("systemctl"),
        "the refusal names what was asked for: {error}"
    );
}

/// P1.2 — a unit outside the host's row is refused, and the OS is never touched.
#[test]
fn an_unknown_unit_is_refused() {
    let error = answer(&config(), &PanicBackend, "restart sshd.service")
        .expect_err("sshd.service is not on this host's allowlist");
    assert_eq!(error.code, "unit_not_allowed", "{error}");
    assert!(error.message.contains("sshd.service"), "{error}");
}

/// P1.3 — the forced-command key cannot ask for a command.
///
/// An empty `SSH_ORIGINAL_COMMAND` is what a person typing `ssh host` produces: the forced
/// command runs with nothing to answer, so it refuses instead of opening a shell. A command
/// wrapped in `bash -c` and a bare `-c` are refused as unknown verbs, and the backend is never
/// consulted in any of the four shapes.
#[test]
fn the_key_cannot_run_a_command() {
    for (command, code) in [
        ("", "shell_refused"),
        ("   ", "shell_refused"),
        ("-c whoami", "unknown_verb"),
        ("bash -c id", "unknown_verb"),
    ] {
        let error = answer(&config(), &PanicBackend, command)
            .expect_err("a shell request must be refused, never run");
        assert_eq!(error.code, code, "{command:?} → {error}");
    }
}

/// The allowlisted path answers, so the refusals above are not vacuously true.
#[test]
fn an_allowed_unit_answers_status_and_logs() {
    let backend = Recording::default();
    let status = answer(&config(), &backend, "status vllm.service").expect("allowed");
    assert_eq!(status["state"], "active");
    assert_eq!(status["verb"], "status");

    let logs = answer(&config(), &backend, "logs vllm.service --lines 2").expect("allowed");
    assert_eq!(logs["log"], "one\ntwo\n");
    assert_eq!(logs["lines"], 2);

    assert_eq!(
        backend.calls.borrow().as_slice(),
        ["status vllm.service", "logs vllm.service 2"]
    );
}

/// A config carrying a launchctl mechanism without a domain is refused at load, so a mistyped
/// deployment fails loudly instead of answering "unknown" for every unit.
#[test]
fn a_launchctl_config_without_a_domain_is_refused() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("host-agent.json");
    std::fs::write(
        &path,
        r#"{"mechanism":"launchctl","units":[{"name":"com.example.unit"}]}"#,
    )
    .expect("write the config");
    let error = Config::from_path(&path).expect_err("no domain");
    assert_eq!(error.code, "config_invalid", "{error}");
}
