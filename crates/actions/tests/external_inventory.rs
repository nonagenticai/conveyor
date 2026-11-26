//! Every External destination the registry declares, classified by runtime (issue #450).
//!
//! The owner's rule is that every tool the conveyor uses is a Rust service. The code change in
//! #450 moved the last Python-bound actions onto conveyor state; this test is the durable half —
//! a future External that names a service nobody has classified fails here, in CI, instead of
//! silently putting a non-Rust destination back in the tool surface.
//!
//! The classification is a checked-in fact, not an inference: each row names the runtime and the
//! date it was read. Re-measure and update the row when a destination changes.
//!
//! Out of reach of this test, and deliberately so: externals *minted at runtime* from an OpenAPI
//! document or a published toolkit. Those are validated at publish time (the handler must be
//! External and the service URL configured), and their destinations are the operator's choice.

use std::collections::BTreeSet;

use conveyor_actions::registry::registry;
use conveyor_actions::spec::ActionKind;
use conveyor_actions::{HandlerRef, registry::Registry};

/// One classified destination.
struct Destination {
    /// The `HandlerRef::External{service}` key, upper-cased in its env var.
    service: &'static str,
    /// What actually answers, in the owner's terms.
    runtime: &'static str,
    /// Where the claim was measured and when.
    evidence: &'static str,
}

/// The destinations the registry may dispatch to, read 2026-11-19.
const DESTINATIONS: &[Destination] = &[
    Destination {
        service: "coding",
        runtime: "Rust",
        evidence: "ghcr.io/nonagenticai/coding (Rust repo) — 2026-11-19",
    },
    Destination {
        service: "fixtures",
        runtime: "Rust",
        evidence: "in-process test transport (crates/actions/src/fixtures.rs) — 2026-11-19",
    },
    Destination {
        service: "image_gen",
        runtime: "Rust",
        evidence: "mac-mini mlx-inference-server on :9204 — 2026-11-19",
    },
    Destination {
        service: "inference",
        runtime: "Rust",
        evidence: "mac-mini mlx-inference-server / vLLM behind the router — 2026-11-19",
    },
    Destination {
        service: "laya",
        runtime: "third-party",
        evidence: "PyTorch engine ruled a model runtime, not a job tool (#76) — 2026-11-19",
    },
    Destination {
        service: "strata_rag",
        runtime: "Rust",
        evidence: "ghcr.io/nonagenticai/strata-rag, repo Cargo.toml — 2026-11-19",
    },
    Destination {
        service: "tts",
        runtime: "Rust",
        evidence: "mac-mini mlx-inference-server on :9202 — 2026-11-19",
    },
    Destination {
        service: "asr",
        runtime: "Rust",
        evidence: "mac-mini asr-backend-rs on :9201 — 2026-11-19",
    },
    Destination {
        service: "billing_shim",
        runtime: "Rust",
        evidence: "apps/billing-shim (Rust, SPEC §12 T11 shim) — 2026-11-24",
    },
    Destination {
        service: "scientific",
        runtime: "Rust",
        evidence: "apps/scientific (Rust, absorbed by #105); deployment at zero replicas — 2026-11-22",
    },
    Destination {
        service: "webscraper",
        runtime: "Rust",
        evidence: "apps/research (Rust, `conveyor-research`); public /api/scrape/* — 2026-11-24",
    },
    Destination {
        service: "lean4",
        runtime: "Rust",
        evidence: "apps/lean4-prover (Rust); licence-gated /api/* behind a sealed bearer — 2026-11-24",
    },
];

fn external_services(registry: &Registry) -> BTreeSet<&'static str> {
    registry
        .iter()
        .filter(|spec| spec.kind == ActionKind::External)
        .filter_map(|spec| match &spec.handler {
            HandlerRef::External { service, .. } => Some(service.as_ref()),
            _ => None,
        })
        .collect()
}

#[test]
fn every_external_destination_is_classified() {
    let registry = registry();
    let declared = external_services(registry);
    let classified: BTreeSet<&str> = DESTINATIONS.iter().map(|row| row.service).collect();

    let unclassified: Vec<&str> = declared.difference(&classified).copied().collect();
    assert!(
        unclassified.is_empty(),
        "unclassified External destination(s): {unclassified:?} — classify the runtime in \
         crates/actions/tests/external_inventory.rs before shipping"
    );

    let stale: Vec<&str> = classified.difference(&declared).copied().collect();
    assert!(
        stale.is_empty(),
        "classified destination(s) no action dispatches to any more: {stale:?} — drop the rows"
    );
}

#[test]
fn no_external_destination_is_python() {
    for row in DESTINATIONS {
        assert!(
            !row.runtime.eq_ignore_ascii_case("python"),
            "{} is Python; the owner's rule is that every conveyor tool is Rust ({})",
            row.service,
            row.evidence
        );
    }
}

/// `graphs.list` described a service that does not exist (SPEC_V2 §11.2). It must stay gone.
#[test]
fn graphs_is_not_registered() {
    let registry = registry();
    assert!(
        registry.get("graphs.list").is_none(),
        "graphs.list was retired with worker-management; do not re-register it"
    );
    for action in ["workers.list", "providers.list"] {
        let spec = registry
            .get(action)
            .unwrap_or_else(|| panic!("{action} must be registered"));
        assert_eq!(
            spec.kind,
            ActionKind::Internal,
            "{action} is served from conveyor state, not an external service"
        );
    }
    for action in ["artifacts.list", "billing.summary"] {
        let spec = registry
            .get(action)
            .unwrap_or_else(|| panic!("{action} must be registered"));
        assert_eq!(
            spec.kind,
            ActionKind::Internal,
            "{action} is served from conveyor state, not an external service"
        );
    }
}
