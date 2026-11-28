//! T14's live-gate receipts (finding `v1-w3c-2`; SPEC_V2 §12 T14).
//!
//! The gate's four clauses are live-estate measurements, not unit tests. What this test can do —
//! and what makes the receipts durable — is refuse the repository when a published toolkit lacks
//! its probe or its one recorded live call, or when the scale, zero-use or outage receipts stop
//! travelling with the code. The evidence is checked in under `docs/evidence/t14/live/`; the
//! 2026-11-26 directory holds this gate run, the 2026-11-25 files beside it hold the scale and
//! outage drill.
//!
//! A toolkit is "published" exactly as the manifest declares it; a draft toolkit is outside the
//! clause (the gate names published toolkits).

use std::path::{Path, PathBuf};

use conveyor_actions::toolkits::manifests;

fn live_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/evidence/t14/live")
}

fn day_dir() -> PathBuf {
    live_dir().join("2026-11-26")
}

fn read_json(path: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} is not readable: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
}

/// Clause (a): for every published toolkit, one live probe and one live-call receipt. The probe
/// is one `tools.probe` run over the catalogue; the call is one file per toolkit so its absence
/// names the toolkit that is unproved.
#[test]
fn every_published_toolkit_carries_a_live_probe_and_one_live_call() {
    let probe = read_json(&day_dir().join("01-probe.json"));
    let probe_rows = probe
        .get("toolkits")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("01-probe.json has no toolkits array: {probe}"));
    let mut published = 0;
    for manifest in manifests().expect("the embedded toolkit manifests parse") {
        if manifest.status != "published" {
            continue;
        }
        published += 1;
        let live = probe_rows.iter().any(|row| {
            row.get("key").and_then(serde_json::Value::as_str) == Some(manifest.key.as_str())
                && row.get("probe").and_then(serde_json::Value::as_str) == Some("live")
        });
        assert!(
            live,
            "published toolkit {:?} has no live probe in 01-probe.json",
            manifest.key
        );

        let call_path = day_dir().join(format!("14-call-{}.json", manifest.key));
        let call = read_json(&call_path);
        assert!(
            call.as_object().is_some_and(|object| !object.is_empty()),
            "published toolkit {:?} has an empty live-call receipt at {}",
            manifest.key,
            call_path.display()
        );
    }
    assert!(
        published > 0,
        "the manifests declare no published toolkit; the gate would be vacuous"
    );
}

/// Clauses (b), (c) and (d): the scientific scale receipt, the zero-use report, and the outage
/// unbind that refuses a call before any network dispatch.
#[test]
fn the_scale_zero_use_and_outage_receipts_exist() {
    let live = live_dir();
    for name in [
        "23-scale-zero.log",
        "24-scale-up.log",
        "29-scale-restore.log",
        "34-scale-restore-2.log",
        "32-probe-outage-2.json",
        "33-call-outage-2.json",
    ] {
        let path = live.join(name);
        let bytes = std::fs::metadata(&path)
            .unwrap_or_else(|error| panic!("missing receipt {}: {error}", path.display()))
            .len();
        assert!(
            bytes > 0,
            "receipt {} is empty; an absence is not a pass",
            path.display()
        );
    }

    let usage = read_json(&day_dir().join("02b-zero-use-after.json"));
    let rows = usage
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("the zero-use report has no rows: {usage}"));
    assert!(
        !rows.is_empty(),
        "the zero-use report must name every published tool version"
    );
}
