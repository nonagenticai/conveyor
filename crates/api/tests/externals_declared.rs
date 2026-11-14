//! S09 §7 (d, f): every external call is a DECLARED action, and the worker surface is read-only.
//!
//! Two properties, both checked against the registry rather than against a service (which may be down, and whose
//! unavailability must never be the reason a rule is not tested):
//!
//! - **No `reqwest::` outside the executor.** A handler that calls a service directly bypasses the timeout, the
//!   breaker, the idempotency claim and the audit row — so the source is scanned for one, and the only permitted
//!   file is `invoke.rs`, which owns the engine's call.
//! - **A display never writes.** `workers.*`, `providers.*`, `graphs.*`, `artifacts.*`, `billing.*` and
//!   `retrieval.*` must all be `effect = read`. Absorption by accretion (S09 trap 4) starts with one write.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Every `.rs` under `crates/actions/src`, recursively.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_handler_calls_a_service_directly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .join("actions/src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(
        !files.is_empty(),
        "the scan found no sources under {root:?}"
    );

    // The complete allowlist of files that may speak HTTP. Each is a TRANSPORT with its own delivery machinery,
    // not an external action call:
    //   * `invoke.rs`    — the engine's external executor (timeout + breaker + idempotency + audit).
    //   * `webhooks.rs`  — S05's outbound webhook delivery: the retry/backoff lives in `delivery`, not here.
    //   * `channels.rs`  — S06's Telegram Bot API send: the same delivery drainer owns its retries.
    //   * `ingest.rs`    — S09's `strata_ingest` handoff behind `documents.ingest`. It is a transport and not a
    //     `HandlerRef::External` action because the job id it returns becomes a `document` column, which no
    //     passthrough could write. Added 2026-11-08.
    //   * `strata.rs`    — S15's `strata_rag` retrieval transport behind `knowledge.query` (and the capability
    //     readiness probe). Same shape as `ingest.rs`: the handler resolves the tenant's sealed token and the
    //     transport owns its own timeout, error mapping and degradation (`knowledge_degraded`). It was missing
    //     from this exactly-compared list, which made the workspace suite red until 2026-11-09.
    //   * `webhook.rs`   — S06d's generic connector transport. The delivery worker owns its retries,
    //     backoff and terminal state; the same shape as `channels.rs`, and the adapter never runs as
    //     a `HandlerRef::External` action.
    //   * `github.rs`   — #70's publish transport behind the `publish.design` worker job. The job owns
    //     the attempt/dead-letter and records the PR result; no action handler calls it, and the token
    //     is opened only inside the job. Added 2026-11-13.
    // A new name here is a decision, not an accident: the list is compared exactly.
    const ALLOWED: [&str; 7] = [
        "invoke.rs",
        "webhooks.rs",
        "channels.rs",
        "ingest.rs",
        "strata.rs",
        "webhook.rs",
        "github.rs",
    ];

    let mut offenders = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if ALLOWED.contains(&name) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            let code = line.trim_start();
            // Comments that MENTION `reqwest::` are documentation (this crate explains the rule in prose).
            if code.starts_with("//") {
                continue;
            }
            if code.contains("reqwest::") || code.contains("reqwest::Client") {
                offenders.push(format!("{}:{}: {}", file.display(), number + 1, code));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "an external call outside the engine bypasses timeout, breaker, idempotency and audit:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_displays_are_read_only_and_carry_a_timeout() {
    let registry = conveyor_actions::registry::registry();
    let displays = [
        "workers.list",
        "providers.list",
        "graphs.list",
        "artifacts.list",
        "billing.summary",
        "coding_jobs.get",
        "retrieval.query",
    ];
    for name in displays {
        let spec = registry
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"));
        assert_eq!(
            spec.effect,
            conveyor_actions::Effect::Read,
            "{name} is a display and must not be able to write"
        );
        assert!(
            spec.timeout_ms > 0 && spec.timeout_ms <= 30_000,
            "{name} needs a bounded timeout, saw {}ms",
            spec.timeout_ms
        );
        assert_eq!(
            spec.kind,
            conveyor_actions::ActionKind::External,
            "{name} is an external service's data, never a local copy"
        );
    }

    // The one external write is the dispatch, and it is bounded at the write timeout.
    let dispatch = registry
        .get("coding_jobs.create")
        .expect("coding_jobs.create is registered");
    assert_eq!(dispatch.effect, conveyor_actions::Effect::Write);
    assert_eq!(dispatch.timeout_ms, 30_000, "a write gets 30 s, a read 5 s");
}

/// S09 §5.5: every external action declares the path its service actually serves.
///
/// The base URL is configuration and names the service **root** (`CONVEYOR_EXTERNAL_<SERVICE>_URL`), so
/// a declared path missing the service's mount prefix is a 404 that no test could see: the stub accepts
/// any path, and a real service answers 404, not an assertion. This table is the measured ground truth,
/// not an assumption — the prefix is **per service**.
///
/// It would have caught `retrieval.query` declaring `/query` against a service that serves `/api/query`.
#[test]
fn every_external_action_declares_its_services_path_prefix() {
    //   service              prefix   measured from
    //   strata_rag           /api     src/api/mod.rs:71 (`/api/query`)
    //   artifacts            /api     /api/artifacts
    //   billing_service      /api     /api/billing/overview
    //   coding               /api     /api/jobs, /api/jobs/status
    //   inference            /api     /api/chat/completions
    //   laya                 /v1      the Jev System-1 bridge: /v1/systemone (care-bus' measured client)
    //   fixtures             ""       tests/fixtures/external/openapi.json: /echo, /fail, /slow, /timeout
    //   worker_management    ""       the Python services mount bare: /workers, /providers, /graphs
    const PREFIXES: [(&str, &str); 8] = [
        ("worker_management", ""),
        ("fixtures", ""),
        ("artifacts", "/api"),
        ("billing_service", "/api"),
        ("coding", "/api"),
        ("strata_rag", "/api"),
        ("inference", "/api"),
        ("laya", "/v1"),
    ];

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for spec in conveyor_actions::registry::registry().iter() {
        let conveyor_actions::spec::HandlerRef::External { service, path, .. } = &spec.handler
        else {
            continue;
        };
        let (_, prefix) = PREFIXES
            .iter()
            .find(|(key, _)| key == service)
            .unwrap_or_else(|| {
                panic!(
                    "{service} has no declared path prefix: read the service's own router, add a \
                     row to PREFIXES, then re-run — a guessed base URL is a 404 no test can see"
                )
            });
        let prefix = *prefix;
        seen.insert(*service);
        assert!(
            prefix.is_empty() || path.starts_with(&format!("{prefix}/")),
            "{} declares {path}, but {service} serves under {prefix:?}: the base URL names the \
             service root, so this path is a 404 in production and the stub accepts any path",
            spec.name
        );
    }
    assert_eq!(
        seen.len(),
        PREFIXES.len(),
        "a PREFIXES row names a service no external action uses; the table has drifted: {seen:?}"
    );
}
