#![cfg(unix)]
//! #172 P3 — the client's rules: no key means no session, the argv is a fixed verb and never a
//! shell, the host key is pinned from the registry row, and the agent's own refusal code survives
//! the trip back into an action error.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use conveyor_host::{Client, HostConfig, HostError, HostTarget};
use serde_json::Value;

/// A stand-in for `ssh` that prints its own argv as JSON and exits as the script says.
fn fake_ssh(directory: &Path, body: &str) -> String {
    let path = directory.join("fake-ssh");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write the script");
    let mut permissions = std::fs::metadata(&path).expect("stat").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("chmod");
    path.display().to_string()
}

/// Print `{"argv": [...]}` for the arguments the client passed.
const ECHO_ARGV: &str = r#"printf '{"argv":['; prev=""; for a in "$@"; do printf '%s"%s"' "$prev" "$a"; prev=","; done; printf ']}'"#;

fn target() -> HostTarget {
    HostTarget {
        name: "example".to_owned(),
        address: "host.example.test".to_owned(),
        agent_user: "agent".to_owned(),
        host_key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPINNED".to_owned(),
    }
}

fn client(directory: &Path) -> Client {
    let key = directory.join("id");
    std::fs::write(&key, "not a real key").expect("write the key");
    Client::new(HostConfig::from_parts(
        &key,
        directory.join("state"),
        fake_ssh(directory, ECHO_ARGV),
    ))
}

/// P3.0 — no key path is `Unconfigured`, and nothing is spawned.
#[test]
fn unconfigured_without_a_key() {
    let error = HostConfig::from_lookup(|_| None).expect_err("no key is unconfigured");
    assert!(matches!(error, HostError::Unconfigured), "{error:?}");
}

/// P3.1 — the session is one fixed verb over a pinned key, never a shell.
#[tokio::test]
async fn the_argv_is_a_fixed_verb_and_never_a_shell() {
    let directory = tempfile::tempdir().expect("tempdir");
    let client = client(directory.path());
    let value = client
        .status(&target(), "com.example.unit")
        .await
        .expect("the fake ssh answers");
    let argv: Vec<String> = value["argv"]
        .as_array()
        .expect("argv array")
        .iter()
        .map(|item| item.as_str().expect("string").to_owned())
        .collect();

    // The remote command is the last two words, and it is the fixed verb — not a shell.
    assert_eq!(&argv[argv.len() - 2..], ["status", "com.example.unit"]);
    assert!(
        !argv
            .iter()
            .any(|arg| arg == "-c" || arg == "sh" || arg == "bash"),
        "no argument names a shell: {argv:?}"
    );
    assert!(
        argv.iter().any(|arg| arg == "agent@host.example.test"),
        "the destination comes from the registry: {argv:?}"
    );
    assert!(
        argv.iter().any(|arg| arg == "BatchMode=yes"),
        "a session never prompts: {argv:?}"
    );
    assert!(
        argv.iter().any(|arg| arg == "StrictHostKeyChecking=yes"),
        "the host key is always checked: {argv:?}"
    );
}

/// P3.2 — the registry's host key is what the connection is checked against.
#[tokio::test]
async fn the_host_key_is_pinned_from_the_row() {
    let directory = tempfile::tempdir().expect("tempdir");
    let client = client(directory.path());
    let _ = client.status(&target(), "com.example.unit").await;
    let known_hosts = std::fs::read_to_string(directory.path().join("state").join("known_hosts"))
        .expect("the known_hosts was written");
    assert!(
        known_hosts.contains("host.example.test ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPINNED"),
        "{known_hosts:?}"
    );
}

/// P3.3 — the agent's refusal code reaches the caller intact.
#[tokio::test]
async fn a_refusal_keeps_the_agents_own_code() {
    let directory = tempfile::tempdir().expect("tempdir");
    let ssh = fake_ssh(
        directory.path(),
        r#"printf '{"error":{"code":"unit_not_allowed","message":"not on the allowlist"}}'; exit 2"#,
    );
    let key = directory.path().join("id");
    std::fs::write(&key, "not a real key").expect("write the key");
    let client = Client::new(HostConfig::from_parts(
        &key,
        directory.path().join("state"),
        ssh,
    ));
    let error = client
        .restart(&target(), "sshd.service")
        .await
        .expect_err("the agent refused");
    assert_eq!(error.refusal_code(), Some("unit_not_allowed"), "{error:?}");
    assert!(
        error.to_string().contains("not on the allowlist"),
        "{error}"
    );
}

/// P3.4 — an unreachable host is a transport failure, never a refusal or a success.
#[tokio::test]
async fn an_unreachable_host_is_a_transport_failure() {
    let directory = tempfile::tempdir().expect("tempdir");
    let key = directory.path().join("id");
    std::fs::write(&key, "not a real key").expect("write the key");
    let client = Client::new(HostConfig::from_parts(
        &key,
        directory.path().join("state"),
        fake_ssh(directory.path(), "exit 255"),
    ));
    let error = client
        .status(&target(), "com.example.unit")
        .await
        .expect_err("255 is a transport failure");
    assert!(matches!(error, HostError::Transport(_)), "{error:?}");
}

/// The answer body is the agent's JSON, parsed — the action builds its output from it.
#[tokio::test]
async fn a_successful_answer_is_the_agents_json() {
    let directory = tempfile::tempdir().expect("tempdir");
    let ssh = fake_ssh(
        directory.path(),
        r#"printf '{"verb":"status","unit":"com.example.unit","mechanism":"launchctl","state":"running","detail":"pid 1"}'"#,
    );
    let key = directory.path().join("id");
    std::fs::write(&key, "not a real key").expect("write the key");
    let client = Client::new(HostConfig::from_parts(
        &key,
        directory.path().join("state"),
        ssh,
    ));
    let value: Value = client
        .status(&target(), "com.example.unit")
        .await
        .expect("the agent answered");
    assert_eq!(value["state"], "running");
    assert_eq!(value["mechanism"], "launchctl");
}
