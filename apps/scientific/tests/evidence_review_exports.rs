//! P4 — the inclusion-set export and its seal, driven through the real router.

mod evidence_review_support;

use axum::http::StatusCode;
use evidence_review_support::{Harness, TENANT_A};
use serde_json::Value;
use sqlx::Row;

const REVIEW: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

/// A review with a locked protocol and an empty sealed corpus: nothing is unscreened, so an
/// export is a complete (empty) inclusion set.
async fn seed_empty(harness: &mut Harness) {
    harness
        .seed(&[
            "INSERT INTO reviews (review_id, tenant_id, project_id, external_key, title, created_by_subject_id)
             VALUES ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'project', 'key-a', 'A review', 'subject-lead')",
            "INSERT INTO review_protocol_versions (
                 protocol_version_id, review_id, tenant_id, version_number, schema_ref, content_hash,
                 inclusion_criteria, exclusion_criteria, stage_definitions, exclusion_reason_vocabulary,
                 reviewers_required, locked_at, created_by_subject_id)
             VALUES ('bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 1, 'urn:test:protocol:1', 'protocol',
                     '{}'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, 2, now(), 'subject-lead')",
            "INSERT INTO review_state_history
                 (state_event_id, review_id, tenant_id, revision_number, from_state, to_state, rationale,
                  actor_subject_id, protocol_version_id)
             VALUES ('13131313-1313-1313-1313-131313131313', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 1, NULL, 'draft', 'review created',
                     'subject-lead', NULL),
                    ('14141414-1414-1414-1414-141414141414', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 2, 'draft', 'configured', 'protocol locked',
                     'subject-lead', NULL),
                    ('15151515-1515-1515-1515-151515151515', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 3, 'configured', 'active', 'activated',
                     'subject-lead', 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb')",
            "INSERT INTO review_stages (
                 stage_id, review_id, tenant_id, protocol_version_id, stage_key, stage_order, name,
                 reviewers_required)
             VALUES ('cccccccc-cccc-cccc-cccc-cccccccccccc', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
                     'title_abstract', 1, 'Title/abstract', 1)",
            "INSERT INTO corpus_versions (
                 corpus_version_id, review_id, tenant_id, version_number, member_count,
                 member_ordering_rule, member_manifest_hash, created_by_subject_id)
             VALUES ('ffffffff-ffff-ffff-ffff-ffffffffffff', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 1, 0, 'record_version_id_asc',
                     'sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
                     'subject-lead')",
        ])
        .await;
}

/// One record, one assignment and one inclusion: the seal now has content that a change moves.
async fn seed_included_record(harness: &mut Harness) {
    harness
        .seed(&[
            "INSERT INTO import_batches (
                 import_batch_id, review_id, tenant_id, source_kind, raw_payload_digest,
                 submitted_by_subject_id)
             VALUES ('16161616-1616-1616-1616-161616161616', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'canonical_json', 'sha256:0000', 'subject-lead')",
            "INSERT INTO review_records (review_record_id, review_id, tenant_id)
             VALUES ('dddddddd-dddd-dddd-dddd-dddddddddddd', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111')",
            "INSERT INTO review_record_versions (
                 review_record_version_id, review_record_id, review_id, tenant_id, version_number,
                 schema_ref, content_hash, record_type, title, created_by_subject_id)
             VALUES ('eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee', 'dddddddd-dddd-dddd-dddd-dddddddddddd',
                     'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
                     1, 'urn:test:record:1', 'sha256:0000', 'journal_article', 'A paper', 'subject-lead')",
            "INSERT INTO import_rows (
                 import_row_id, import_batch_id, review_id, tenant_id, row_index, raw_representation,
                 status, review_record_id)
             VALUES ('17171717-1717-1717-1717-171717171717', '16161616-1616-1616-1616-161616161616',
                     'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111', 0,
                     '{}'::jsonb, 'accepted', 'dddddddd-dddd-dddd-dddd-dddddddddddd')",
            "INSERT INTO screening_assignments (
                 assignment_id, review_id, tenant_id, stage_id, round_number, review_record_id,
                 reviewer_subject_id, assigned_by_subject_id, corpus_version_id)
             VALUES ('99999999-9999-9999-9999-999999999999', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'cccccccc-cccc-cccc-cccc-cccccccccccc', 1,
                     'dddddddd-dddd-dddd-dddd-dddddddddddd', 'subject-lead', 'subject-lead',
                     'ffffffff-ffff-ffff-ffff-ffffffffffff')",
            "INSERT INTO screening_decisions (
                 decision_id, review_id, tenant_id, stage_id, round_number, review_record_id,
                 review_record_version_id, assignment_id, reviewer_subject_id, disposition, rationale)
             VALUES ('12121212-1212-1212-1212-121212121212', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'cccccccc-cccc-cccc-cccc-cccccccccccc', 1,
                     'dddddddd-dddd-dddd-dddd-dddddddddddd', 'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee',
                     '99999999-9999-9999-9999-999999999999', 'subject-lead', 'include', 'relevant')",
        ])
        .await;
}

async fn create_export(harness: &Harness) -> (StatusCode, Value) {
    harness
        .call(
            Some("subject-lead|lead"),
            TENANT_A,
            "POST",
            &format!("/api/reviews/{REVIEW}/exports"),
            None,
        )
        .await
}

/// P4.1 — the same inclusion set sealed twice is the same canonical hash; a changed record moves
/// the seal.
#[tokio::test]
async fn seal() {
    let Some(mut harness) = Harness::new("seal").await else {
        return;
    };
    seed_empty(&mut harness).await;

    let (status, first) = create_export(&harness).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let (status, second) = create_export(&harness).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a replay is not a new seal: {second}"
    );
    assert_eq!(second["replayed"], true, "{second}");
    assert_eq!(
        first["manifest_hash"], second["manifest_hash"],
        "the same inclusion set must seal to the same canonical hash"
    );
    assert_eq!(
        first["export_id"], second["export_id"],
        "a replay serves the recorded seal, not a second row"
    );

    // A changed record changes the seal.
    seed_included_record(&mut harness).await;
    let (status, changed) = create_export(&harness).await;
    assert_eq!(status, StatusCode::CREATED, "{changed}");
    assert_ne!(
        changed["manifest_hash"], first["manifest_hash"],
        "a changed inclusion set must move the seal"
    );
    assert_eq!(changed["included_count"], 1, "{changed}");

    harness.cleanup().await;
}

/// P4.2 — a seal is append-only: the recorded bytes are served even after the ledger moves, and
/// the row refuses UPDATE and DELETE.
#[tokio::test]
async fn a_seal_is_append_only() {
    let Some(mut harness) = Harness::new("a_seal_is_append_only").await else {
        return;
    };
    seed_empty(&mut harness).await;
    let (status, seal) = create_export(&harness).await;
    assert_eq!(status, StatusCode::CREATED, "{seal}");
    let export_id = seal["export_id"].as_str().expect("export id").to_owned();

    // The ledger moves after the seal.
    seed_included_record(&mut harness).await;

    let (status, read) = harness
        .call(
            Some("subject-lead|lead"),
            TENANT_A,
            "GET",
            &format!("/api/reviews/{REVIEW}/exports/{export_id}?with_payload=true"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(
        read["included_count"], 0,
        "the seal serves the bytes it recorded, not a re-derivation"
    );
    let included = read["payload"]["included"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(usize::MAX);
    assert_eq!(included, 0, "the recorded payload is unchanged: {read}");

    // The verified bytes are the recorded ones.
    let (status, verified) = harness
        .call(
            Some("subject-lead|lead"),
            TENANT_A,
            "GET",
            &format!("/api/reviews/{REVIEW}/exports/{export_id}:verify"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{verified}");
    assert_eq!(verified["verified"], true, "{verified}");

    // Append-only at the database: UPDATE and DELETE raise restrict_violation.
    for statement in [
        "UPDATE inclusion_exports SET manifest_hash = 'changed'",
        "DELETE FROM inclusion_exports",
    ] {
        let error = sqlx::query(statement)
            .execute(&mut harness.admin)
            .await
            .expect_err("the append-only trigger must refuse");
        let code = error
            .as_database_error()
            .and_then(|error| error.code())
            .map(|code| code.into_owned());
        assert_eq!(code.as_deref(), Some("23001"), "{statement}");
    }

    let row = sqlx::query("SELECT count(*) AS n FROM inclusion_exports")
        .fetch_one(&mut harness.admin)
        .await
        .expect("count");
    assert_eq!(row.get::<i64, _>("n"), 1);

    harness.cleanup().await;
}
