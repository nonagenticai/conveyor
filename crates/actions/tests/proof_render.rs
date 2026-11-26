//! #746 — the facts renderer: one pure function from canonical facts to Lean source.
//!
//! These are the issue's P3 scenarios. They are deliberately about *bytes*: a certificate is
//! re-checkable because the source is reproducible, so "the same facts render the same bytes" and
//! "a bad snapshot is refused by name" are the whole contract.
//!
//! `rbuild -- test -p conveyor-actions --test proof_render`.

use conveyor_actions::proof::render::{self, RenderError};
use serde_json::{Value, json};

/// A slice that is closed and carries its evidence.
fn slice_facts() -> Value {
    json!({
        "kind": "slice",
        "spec_sha256": "a".repeat(64),
        "model_sha256": render::model_sha256(),
        "state": "closed",
        "required": ["gate", "audit"],
        "present": ["audit", "gate"],
    })
}

fn programme_facts() -> Value {
    json!({
        "kind": "programme",
        "spec_sha256": "a".repeat(64),
        "model_sha256": render::model_sha256(),
        "expected": ["T0", "T1"],
        "leaves": [["T0", "d".repeat(64)], ["T1", "e".repeat(64)]],
    })
}

fn package_facts() -> Value {
    json!({
        "kind": "package",
        "spec_sha256": "a".repeat(64),
        "model_sha256": render::model_sha256(),
        "migrations": ["0076_lean_proof.sql"],
        "ledger": ["0076_lean_proof.sql"],
        "routes": ["GET /api/actions"],
        "served": ["GET /api/actions", "POST /api/actions/proofs.record"],
        "checks": [["registry", true], ["parity_http_mcp", true]],
    })
}

/// P3.1 — the same facts render byte-identical, twice, from a freshly parsed document.
#[test]
fn the_same_facts_render_byte_identical() {
    let canonical = slice_facts().to_string();
    let first = render::render(
        &serde_json::from_str(&canonical).expect("the facts parse"),
        "delivery_slice",
    )
    .expect("the first render");
    let second = render::render(
        &serde_json::from_str(&canonical).expect("the facts parse again"),
        "delivery_slice",
    )
    .expect("the second render");

    assert_eq!(first, second, "two renders of one document are one source");
    assert!(
        first.starts_with(render::MODEL_SOURCE),
        "the model travels inside every certificate"
    );
    assert!(
        first.contains("theorem delivery_slice : checkSlice facts = true := by decide"),
        "{first}"
    );
    assert!(
        first.contains("required := [\"audit\", \"gate\"]"),
        "lists are emitted sorted, whatever order the document had: {first}"
    );
}

/// P3.2 — a snapshot missing an evidence row is refused, and the refusal names the item.
#[test]
fn a_bad_snapshot_names_the_item() {
    let mut facts = slice_facts();
    facts["present"] = json!(["gate"]);

    let error = render::render(&facts, "delivery_slice").expect_err("an uncovered item is refused");
    assert_eq!(
        error,
        RenderError::Incomplete("required evidence \"audit\"".to_owned())
    );
    assert!(
        format!("{error:?}").contains("audit"),
        "the refusal names the item: {error:?}"
    );
}

/// A slice that is not closed is not a delivered slice.
#[test]
fn an_open_slice_is_refused() {
    let mut facts = slice_facts();
    facts["state"] = json!("inProgress");
    let error = render::render(&facts, "delivery_slice").expect_err("an open slice is refused");
    assert!(matches!(error, RenderError::Incomplete(_)), "{error:?}");
}

/// The bundle: a dropped leaf, an extra leaf, a repeated name and a shared certificate digest are
/// each refused by name.
#[test]
fn a_bundle_is_refused_when_its_leaf_set_is_wrong() {
    let mut dropped = programme_facts();
    dropped["leaves"] = json!([["T0", "d".repeat(64)]]);
    let error = render::render(&dropped, "bundle").expect_err("a dropped leaf is refused");
    assert_eq!(
        error,
        RenderError::Incomplete("leaf \"T1\" is missing".to_owned())
    );

    let mut extra = programme_facts();
    extra["leaves"] = json!([
        ["T0", "d".repeat(64)],
        ["T1", "e".repeat(64)],
        ["T2", "f".repeat(64)]
    ]);
    let error = render::render(&extra, "bundle").expect_err("an unexpected leaf is refused");
    assert_eq!(
        error,
        RenderError::Incomplete("leaf \"T2\" is not expected".to_owned())
    );

    let mut duplicated = programme_facts();
    duplicated["leaves"] = json!([["T1", "d".repeat(64)], ["T1", "e".repeat(64)]]);
    let error = render::render(&duplicated, "bundle").expect_err("a duplicated leaf is refused");
    assert_eq!(
        error,
        RenderError::Incomplete("leaf \"T1\" is present more than once".to_owned())
    );

    // One certificate backing two slices would let a bundle claim a slice it never proved.
    let mut shared = programme_facts();
    shared["leaves"] = json!([["T0", "d".repeat(64)], ["T1", "d".repeat(64)]]);
    let error = render::render(&shared, "bundle").expect_err("a shared certificate is refused");
    assert_eq!(
        error,
        RenderError::Incomplete(format!(
            "certificate \"{}\" backs more than one leaf",
            "d".repeat(64)
        ))
    );
}

/// The package: an unpaired migration, an unserved route and a failed check are each refused.
#[test]
fn a_package_is_refused_by_its_own_relations() {
    let mut unpaired = package_facts();
    unpaired["ledger"] = json!([]);
    let error = render::render(&unpaired, "package").expect_err("an unpaired migration");
    assert_eq!(
        error,
        RenderError::Incomplete(
            "migration \"0076_lean_proof.sql\" is not in the ledger".to_owned()
        )
    );

    let mut unserved = package_facts();
    unserved["served"] = json!([]);
    let error = render::render(&unserved, "package").expect_err("a route nobody serves");
    assert_eq!(
        error,
        RenderError::Incomplete(
            "route \"GET /api/actions\" is not in the served document".to_owned()
        )
    );

    let mut failed = package_facts();
    failed["checks"] = json!([["registry", false]]);
    let error = render::render(&failed, "package").expect_err("a failed check");
    assert_eq!(
        error,
        RenderError::Incomplete("check \"registry\" did not pass".to_owned())
    );
}

/// A facts document naming a model the binary does not embed is refused rather than rendered.
#[test]
fn another_model_is_refused() {
    let mut facts = slice_facts();
    facts["model_sha256"] = json!("b".repeat(64));
    let error = render::render(&facts, "delivery_slice").expect_err("another model");
    match error {
        RenderError::ModelMismatch { named, embedded } => {
            assert_eq!(named, "b".repeat(64));
            assert_eq!(embedded, render::model_sha256());
        }
        other => panic!("expected a model mismatch, got {other:?}"),
    }
}

/// Lean injection is impossible: a quote or a backslash is refused, and so is any non-ASCII byte
/// or a name the kernel does not back.
#[test]
fn facts_cannot_inject_lean() {
    for hostile in ["a\"b", "a\\b", "a\nb", "naïve"] {
        let mut facts = slice_facts();
        facts["present"] = json!([hostile]);
        let error = render::render(&facts, "delivery_slice")
            .expect_err("a string that could escape the literal is refused");
        assert!(
            matches!(error, RenderError::Forbidden(_)),
            "{hostile:?}: {error:?}"
        );
    }

    // The tactic `decide +native` is the same trust as `native_decide`; both are refused so a
    // `trust: kernel` certificate can never rest on the compiler.
    let mut facts = slice_facts();
    facts["present"] = json!(["by decide +native"]);
    let error = render::render(&facts, "delivery_slice").expect_err("a native reduction");
    assert!(matches!(error, RenderError::Forbidden(_)), "{error:?}");
}

/// An unknown kind and an identifier that is not a Lean name are refused, not guessed.
#[test]
fn a_kind_and_a_theorem_name_must_be_known() {
    let mut facts = slice_facts();
    facts["kind"] = json!("manifesto");
    assert_eq!(
        render::render(&facts, "t").expect_err("an unknown kind"),
        RenderError::UnknownKind("manifesto".to_owned())
    );

    for name in ["", "1abc", "a-b", "has space", "a;b"] {
        assert_eq!(
            render::render(&slice_facts(), name).expect_err("a bad theorem name"),
            RenderError::TheoremName(name.to_owned())
        );
    }
}

/// The model's digest is pinned: a model edit is a deliberate act, not a silent change to every
/// future certificate. If this fails, re-read `README.md` § Model drift and update the constant.
#[test]
fn the_model_digest_is_pinned() {
    assert_eq!(
        render::model_sha256(),
        "d19f8ad344d976d6a1a2e91d7eacdb5e6edc33e0fcd1367896f6af697baca504",
        "the embedded model changed; every future certificate carries the new bytes"
    );
}

/// Every kind renders its own structure and checker, and the checker name is the model's.
#[test]
fn each_kind_renders_its_own_checker() {
    let slice = render::render(&slice_facts(), "s").expect("slice");
    assert!(slice.contains("def facts : SliceFacts"), "{slice}");
    assert!(slice.contains("checkSlice facts = true"), "{slice}");

    let programme = render::render(&programme_facts(), "p").expect("programme");
    assert!(
        programme.contains("def facts : ProgrammeFacts"),
        "{programme}"
    );
    assert!(
        programme.contains("checkProgramme facts = true"),
        "{programme}"
    );

    let package = render::render(&package_facts(), "k").expect("package");
    assert!(package.contains("def facts : PackageFacts"), "{package}");
    assert!(package.contains("checkPackage facts = true"), "{package}");
    assert!(
        package.contains("checks := [(\"parity_http_mcp\", true), (\"registry\", true)]"),
        "the checks are emitted sorted by name: {package}"
    );
}
