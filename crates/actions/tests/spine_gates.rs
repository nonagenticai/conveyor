//! The plan spine's approval rule: an approval is a gate effect, never a status a caller writes.
//!
//! Three scenarios pin the rule the tranche re-baseline depends on:
//! a change row cannot be born `approved`, an open change cannot be patched to `approved`, and a
//! baseline moves only when the `change` gate itself approved the change — not when the register
//! row merely says so.
//!
//! `cargo-guard.py test -p conveyor-actions --test spine_gates`.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

async fn create_programme(running: &support::Running, name: &str) -> String {
    let (status, body) = running
        .invoke(
            "programmes.create",
            TOKEN,
            json!({"name": name}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

/// A programme baseline and a tranche that points nowhere yet.
async fn create_tranche(
    running: &support::Running,
    programme: &str,
    number: i32,
) -> (String, String) {
    let (status, headers, body) = running
        .invoke_full(
            "tranches.create",
            TOKEN,
            json!({"programme_id": programme, "number": number}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let etag = headers
        .get("etag")
        .cloned()
        .unwrap_or_else(|| panic!("the tranche carries an ETag: {body}"));
    (id_of(output(&body), "id"), etag)
}

async fn programme_baseline(running: &support::Running, programme: &str) -> String {
    let (status, body) = running
        .invoke(
            "programmes.baseline",
            TOKEN,
            json!({"programme_id": programme, "kind": "management"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

async fn create_project(running: &support::Running, programme: &str, title: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": title, "project_type_code": "delivery", "programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

async fn project_baseline(running: &support::Running, project: &str) -> String {
    let (status, body) = running
        .invoke(
            "baselines.take",
            TOKEN,
            json!({"project_id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

/// An open change row against a project baseline — the register's own shape for a change.
async fn open_change(running: &support::Running, project: &str, baseline: &str) -> String {
    let (status, body) = running
        .invoke(
            "registers.create",
            TOKEN,
            json!({
                "project_id": project,
                "kind": "change",
                "title": "Replace the plan",
                "baseline_ref": baseline,
                "meta": {"impact": {"scope": 0, "schedule_days": 3, "cost": 0, "risk": "low"}}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    id_of(output(&body), "id")
}

/// P1.1 — a change row cannot be born `approved`, and an open one cannot be patched to it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_register_cannot_be_born_approved() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Approval rule").await;
    let project = create_project(&running, &programme, "Approval project").await;
    let baseline = project_baseline(&running, &project).await;

    let (status, refused) = running
        .invoke(
            "registers.create",
            TOKEN,
            json!({
                "project_id": project,
                "kind": "change",
                "title": "Self-declared",
                "status": "approved",
                "baseline_ref": baseline,
                "meta": {"impact": {"scope": 0, "schedule_days": 1, "cost": 0, "risk": "low"}}
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 422,
        "a row born approved must be refused: {refused}"
    );
    assert_eq!(
        output(&refused)["errors"][0]["code"],
        "approval_is_a_gate_effect",
        "{refused}"
    );

    let change = open_change(&running, &project, &baseline).await;
    let (status, refused) = running
        .invoke(
            "registers.update",
            TOKEN,
            json!({"id": change, "status": "approved"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 422,
        "an open change cannot be patched to approved: {refused}"
    );
    assert_eq!(
        output(&refused)["errors"][0]["code"],
        "approval_is_a_gate_effect",
        "{refused}"
    );

    // The row is untouched: still open, still version 1.
    let (status, read) = running
        .invoke(
            "registers.get",
            TOKEN,
            json!({"id": change}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(output(&read)["status"], "open", "{read}");
}

/// P1.2a — a change row that only *says* approved does not move a tranche's baseline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_self_declared_approved_change_cannot_advance_a_baseline() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Self-declared change").await;
    let (tranche, etag) = create_tranche(&running, &programme, 0).await;
    let target = programme_baseline(&running, &programme).await;
    let project = create_project(&running, &programme, "Change project").await;
    let baseline = project_baseline(&running, &project).await;
    let change = open_change(&running, &project, &baseline).await;

    // The store's own door, left open on purpose: a row written straight to `approved`,
    // bypassing every action. The guard must not read this status.
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope the register update");
    sqlx::query("UPDATE project_register_item SET status = 'approved' WHERE id = $1::uuid")
        .bind(&change)
        .execute(&mut *tx)
        .await
        .expect("self-declare the change");
    tx.commit().await.expect("commit the self-declaration");

    let (status, refused) = running
        .invoke(
            "tranches.update",
            TOKEN,
            json!({"id": tranche, "baseline_ref": target, "change_ref": change}),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 409,
        "a self-declared approval must not move a baseline: {refused}"
    );
    assert_eq!(output(&refused)["type"], "change_not_approved", "{refused}");
}

/// P1.2b — the same change, approved through its gate, does move the baseline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_change_gate_advances_the_baseline() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Gated change").await;
    let (tranche, etag) = create_tranche(&running, &programme, 0).await;
    let target = programme_baseline(&running, &programme).await;
    let project = create_project(&running, &programme, "Gated project").await;
    let baseline = project_baseline(&running, &project).await;
    let change = open_change(&running, &project, &baseline).await;

    // The `change` gate. `changes.request` is its production writer; that path needs a PM job
    // binding, so the fixture opens the same row through the store's own writer with the meta
    // `changes.request` builds (`{"change_id": …}`).
    let mut new_gate = conveyor_store::approval::NewApproval::new("change", "pm");
    new_gate.project_id = Some(uuid::Uuid::parse_str(&project).expect("project id"));
    new_gate.required_decisions = 1;
    new_gate.requested_by = Some(seeded.user_id);
    new_gate.meta = json!({"change_id": change});
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope the gate");
    let gate = conveyor_store::approval::insert_approval_tx(&mut *tx, seeded.tenant, &new_gate)
        .await
        .expect("open the change gate");
    tx.commit().await.expect("commit the gate");
    let approval = gate.id.to_string();

    let (status, decided) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approval, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{decided}");

    // The gate's effect is what writes the status.
    let (status, read) = running
        .invoke(
            "registers.get",
            TOKEN,
            json!({"id": change}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(
        output(&read)["status"],
        "approved",
        "the change gate approves its row: {read}"
    );

    let (status, updated) = running
        .invoke(
            "tranches.update",
            TOKEN,
            json!({"id": tranche, "baseline_ref": target, "change_ref": change}),
            Some(etag.as_str()),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["baseline_ref"], target, "{updated}");
}
