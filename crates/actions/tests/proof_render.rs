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
        "bbbc56e12e729e6daea3b244d29038819fb5802d187dfbc2a14a0a050a138e57",
        "the embedded model changed; every future certificate carries the new bytes"
    );
}

// ── the unit ledger (03-DELIVERY, A0) ────────────────────────────────────────────────────

fn unit(key: &str, issue: Value, branch_deleted: Value, pull: &str, ci_runs: u64) -> Value {
    json!({
        "key": key,
        "issue": issue,
        "branches": 1,
        "branchDeleted": branch_deleted,
        "pulls": 1,
        "pull": pull,
        "ciRuns": ci_runs,
        "closesKeyword": true,
        "verifyParts": true,
    })
}

/// Three units: two merged and complete, one in flight and complete. Every rule holds.
fn unit_ledger_facts() -> Value {
    json!({
        "kind": "unit_ledger",
        "spec_sha256": "a".repeat(64),
        "model_sha256": render::model_sha256(),
        "units": [
            unit("conveyor#837", json!("closed"), json!(true), "merged", 1),
            unit("conveyor#836", json!("closed"), json!(true), "merged", 2),
            unit("conveyor#838", Value::Null, Value::Null, "ready", 1),
        ],
    })
}

/// The exact emission format, including the per-rule literal the kernel proves. This is the A0
/// template with the two textual fields added; a change here is a deliberate format change.
#[test]
fn a_unit_ledger_renders_its_observations_and_rules() {
    let source = render::render(&unit_ledger_facts(), "ledger").expect("the ledger renders");
    assert!(source.starts_with(render::MODEL_SOURCE), "{source}");
    let expected = "\
def facts : List UnitObs := [
  { issue := some .closed, branches := some 1, branchDeleted := some true, pulls := some 1, pull := some .merged, ciRuns := some 2, closesKeyword := some true, verifyParts := some true },
  { issue := some .closed, branches := some 1, branchDeleted := some true, pulls := some 1, pull := some .merged, ciRuns := some 1, closesKeyword := some true, verifyParts := some true },
  { issue := none, branches := some 1, branchDeleted := none, pulls := some 1, pull := some .ready, ciRuns := some 1, closesKeyword := some true, verifyParts := some true }
]

theorem ledger_rules : ruleResults facts = [(\"oneBranch\", true), (\"onePull\", true), (\"pullUsable\", true), (\"ciBudget\", true), (\"mergeComplete\", true), (\"closesKeyword\", true), (\"verifyParts\", true)] := by decide

theorem ledger : checkLedger facts = true := by decide
";
    assert!(
        source.ends_with(expected),
        "the emitted tail changed:\n{source}"
    );
}

/// A failing ledger must still render: the kernel, not the renderer, is what refuses it, and the
/// unproven certificate is the deliverable. Every rule reports its own outcome.
#[test]
fn a_failing_unit_ledger_still_renders_and_names_the_rule() {
    let mut facts = unit_ledger_facts();
    facts["units"][0]["branches"] = json!(2);
    facts["units"][1]["verifyParts"] = json!(false);

    let source = render::render(&facts, "bad").expect("a failing ledger renders");
    assert!(source.contains("(\"oneBranch\", false)"), "{source}");
    assert!(source.contains("(\"verifyParts\", false)"), "{source}");
    assert!(source.contains("(\"onePull\", true)"), "{source}");
    assert!(
        source.contains("theorem bad : checkLedger facts = true := by decide"),
        "the aggregate keeps the positive statement so the kernel refuses it: {source}"
    );
}

/// Unknown is not true: a null fact fails its rule closed.
#[test]
fn an_unknown_fact_fails_closed() {
    let mut facts = unit_ledger_facts();
    facts["units"][0]["closesKeyword"] = Value::Null;
    let source = render::render(&facts, "bad").expect("an unknown fact still renders");
    assert!(source.contains("(\"closesKeyword\", false)"), "{source}");
    assert!(
        source.contains("closesKeyword := none"),
        "the unknown travels as none: {source}"
    );
}

/// Two units with one key would let a ledger hide one behind the other; the renderer refuses.
#[test]
fn a_duplicate_unit_key_is_refused() {
    let mut facts = unit_ledger_facts();
    facts["units"][1]["key"] = json!("conveyor#836");
    facts["units"][2]["key"] = json!("conveyor#836");
    let error = render::render(&facts, "ledger").expect_err("a duplicate key");
    assert_eq!(
        error,
        RenderError::Incomplete("unit \"conveyor#836\" appears more than once".to_owned())
    );
}

/// The A0 chunk is the renderer's limit: a bigger ledger is chunked by the caller, never a source
/// that silently needs `maxRecDepth`.
#[test]
fn more_than_two_hundred_units_are_refused() {
    let units: Vec<Value> = (0..201)
        .map(|index| {
            unit(
                &format!("conveyor#{}", 1000 + index),
                json!("closed"),
                json!(true),
                "merged",
                1,
            )
        })
        .collect();
    let facts = json!({
        "kind": "unit_ledger",
        "spec_sha256": "a".repeat(64),
        "model_sha256": render::model_sha256(),
        "units": units,
    });
    let error = render::render(&facts, "ledger").expect_err("a ledger over the chunk");
    assert_eq!(
        error,
        RenderError::Incomplete(
            "the unit ledger holds 201 units; a certificate chunk is at most 200".to_owned()
        )
    );
}

/// The document's order is not the source's order: the ledger is sorted by key, so the same
/// snapshot renders the same bytes whatever order the collector wrote.
#[test]
fn a_unit_ledger_is_sorted_and_deterministic() {
    let facts = unit_ledger_facts();
    let mut reversed = facts.clone();
    reversed["units"].as_array_mut().expect("units").reverse();
    assert_eq!(
        render::render(&facts, "ledger").expect("forward"),
        render::render(&reversed, "ledger").expect("reversed"),
    );
}

/// The unit arm validates the document structurally, exactly like every other kind.
#[test]
fn a_malformed_unit_document_is_refused() {
    let mut unknown = unit_ledger_facts();
    unknown["units"][0]["pull"] = json!("abandoned");
    assert!(
        matches!(
            render::render(&unknown, "ledger"),
            Err(RenderError::NotA(_))
        ),
        "an enum the model does not have is refused"
    );

    let mut missing = unit_ledger_facts();
    missing["units"][0]
        .as_object_mut()
        .expect("unit")
        .remove("ciRuns");
    assert!(
        matches!(
            render::render(&missing, "ledger"),
            Err(RenderError::NotA(_))
        ),
        "a missing observation is refused, never defaulted"
    );

    let mut negative = unit_ledger_facts();
    negative["units"][0]["pulls"] = json!(-1);
    assert!(
        matches!(
            render::render(&negative, "ledger"),
            Err(RenderError::NotA(_))
        ),
        "a negative count is not a Nat"
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

    let ledger = render::render(&unit_ledger_facts(), "u").expect("unit ledger");
    assert!(ledger.contains("def facts : List UnitObs"), "{ledger}");
    assert!(
        ledger.contains("theorem u_rules : ruleResults facts"),
        "{ledger}"
    );
    assert!(
        ledger.contains("theorem u : checkLedger facts = true"),
        "{ledger}"
    );
}

/// #837 — a complete engine record renders the `checkRecord` instance the kernel closes.
fn unit_record_facts() -> Value {
    json!({
        "kind": "unit_record",
        "spec_sha256": "a".repeat(64),
        "spec_name": "docs/SPEC_V2.md",
        "model_sha256": render::model_sha256(),
        "issue_ref": "nonagenticai/conveyor#837",
        "repo": "nonagenticai/conveyor",
        "lane": "issue/837-engine-dod",
        "pull_number": 837,
        "pull_url": "https://github.com/nonagenticai/conveyor/pull/837",
        "pull_head": "b".repeat(40),
        "ci_run_id": "run-837",
        "ci_sha": "b".repeat(40),
        "ci_verdict": "green",
        "merge_sha": "c".repeat(40),
        "deploy_digest": "sha256:deployed",
        "serving_digest": "sha256:deployed",
        "live_health": 200,
        "live_acceptance_exit": 0,
        "gate_review": true,
        "gate_merge": true,
        "gate_deploy": true,
    })
}

#[test]
fn a_unit_record_renders_the_record_theorem() {
    let source = render::render(&unit_record_facts(), "unit_record_ok").expect("the unit record");
    assert!(source.contains("def facts : UnitRecordFacts"), "{source}");
    assert!(
        source.contains("theorem unit_record_ok : checkRecord facts = true := by decide"),
        "{source}"
    );
    assert!(
        source.contains("issueRef := some \"nonagenticai/conveyor#837\""),
        "{source}"
    );
    assert!(
        source.contains("servingDigest := some \"sha256:deployed\""),
        "{source}"
    );
    // The corpus check streams this exact emission into the prover's own project. It is opt-in
    // so a normal run stays hermetic.
    if std::env::var("CONVEYOR_DUMP_UNIT_SOURCE").is_ok() {
        std::fs::write("/tmp/unit_record_ok.lean", &source).expect("dump the valid unit source");
    }
}

/// An incomplete record still renders: the renderer does not hide the defect, the kernel refuses
/// it. `mergeSha := none` is the fail-closed fact the universal theorem names.
#[test]
fn an_incomplete_unit_record_renders_and_fails_closed() {
    let mut facts = unit_record_facts();
    facts["merge_sha"] = json!(null);
    let source =
        render::render(&facts, "unit_record_incomplete").expect("the incomplete record renders");
    assert!(source.contains("mergeSha := none"), "{source}");
    if std::env::var("CONVEYOR_DUMP_UNIT_SOURCE").is_ok() {
        std::fs::write("/tmp/unit_record_incomplete.lean", &source)
            .expect("dump the incomplete unit source");
    }
}
