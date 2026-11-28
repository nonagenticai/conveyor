#![forbid(unsafe_code)]
//! The declared-vs-evidenced output register (#756 P2).
//!
//! The standard (`seed/output-standard.json`) declares, per project class, which output kinds a
//! project must publish; the store holds which kinds it actually published. The register is the
//! one place that compares the two, per project, and the site renders its result.
//!
//! The comparison used to live in `scripts/check-output-standard.py`, which could only say
//! "clean" or "dirty" in CI. The register says it on the public record: every project names its
//! class, its declared kinds, the kinds it evidenced and the ones still missing.
//!
//! **Deterministic.** Projects come in the snapshot's own order, kinds are sorted, and every map
//! is a `BTreeMap`, so the same rows produce the same bytes.
//!
//! **One derivation.** The class comes from [`conveyor_domain::output_standard::class_for`], the
//! same embedded standard the publish gate reads; nothing is re-parsed here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use conveyor_domain::output_standard;
use serde_json::{Value, json};

/// The committed conformance gap register the site's posture line quotes.
pub(crate) const GAP_REGISTER: &str = "docs/evidence/conformance-gap-register.json";

/// One published output, as the register names it.
///
/// A small borrowed view of the document row: the register never needs the body, and keeping the
/// projection explicit is what makes [`build`] a pure function the tests can drive.
pub(crate) struct PublishedDoc {
    /// The document id.
    pub id: String,
    /// The owning project id, when the row belongs to one.
    pub project_id: Option<String>,
    /// The output-standard kind the row was published under.
    pub kind: Option<String>,
    /// The display title.
    pub title: String,
    /// The row's status (`active`, `rejected`, `failed`).
    pub status: String,
    /// The row's optimistic-concurrency version.
    pub version: u64,
    /// The sha256 of the body or the stored bytes.
    pub hash: Option<String>,
}

impl PublishedDoc {
    /// The page the pack renders for this row, relative to the pack root.
    fn path(&self) -> String {
        format!("docs/output-{}.html", self.id)
    }
}

/// Project the published document rows into the register's own view.
pub(crate) fn from_records(
    records: &[conveyor_store::document::DocumentRecord],
) -> Vec<PublishedDoc> {
    records
        .iter()
        .map(|record| PublishedDoc {
            id: record.id.to_string(),
            project_id: record.project_id.map(|id| id.to_string()),
            kind: record.category.clone(),
            title: record.title.clone(),
            status: record.status.clone(),
            version: record.version.get(),
            hash: record.content_hash.clone(),
        })
        .collect()
}

/// Build the register from the programme section and the published rows.
///
/// `docs` is the same set `outputs.js` renders, so the two cannot describe different rows when
/// they are built from one read (the live view and the CLI both do).
pub(crate) fn build(programme: &Value, docs: &[PublishedDoc], gaps: Option<&Value>) -> Value {
    let projects = programme
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut by_project: BTreeMap<&str, Vec<&PublishedDoc>> = BTreeMap::new();
    let mut unassigned: Vec<&PublishedDoc> = Vec::new();
    let known: BTreeSet<&str> = projects
        .iter()
        .filter_map(|project| project.get("id").and_then(Value::as_str))
        .collect();
    for doc in docs {
        match doc.project_id.as_deref() {
            Some(id) if known.contains(id) => by_project.entry(id).or_default().push(doc),
            _ => unassigned.push(doc),
        }
    }

    let mut rows = Vec::with_capacity(projects.len());
    let mut class_rows: BTreeMap<String, Value> = BTreeMap::new();
    let mut drift = Vec::new();
    let (mut declared_total, mut evidenced_total, mut missing_total) = (0usize, 0usize, 0usize);
    let mut projects_without_class = 0usize;
    let through_tranche = output_standard::through_tranche();

    for project in &projects {
        let id = project
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let key = project
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let status = project
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let meta = project.get("meta").cloned().unwrap_or(Value::Null);
        let facts: BTreeMap<String, String> = [
            ("key", key.to_owned()),
            (
                "track",
                meta.get("track")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ),
            ("status", status.to_owned()),
            (
                "measured_status",
                meta.get("measured_status")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ),
        ]
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(name, value)| (name.to_owned(), value))
        .collect();

        let class = output_standard::class_for(&facts);
        let (class_name, now, closure) = match &class {
            Some(class) => (
                Value::String(class.name.clone()),
                class.now.clone(),
                class.closure.clone(),
            ),
            None => {
                projects_without_class += 1;
                (Value::Null, Vec::new(), Vec::new())
            }
        };

        // The seed's own expected sets are derived from the same class. A disagreement is the
        // invariant the retired checker enforced; here it is a visible gap, never a silent drift.
        for (field, expected) in [("expected_outputs", &now), ("closure_outputs", &closure)] {
            let recorded: Vec<String> = meta
                .get(field)
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            if class.is_some() && recorded != *expected {
                drift.push(json!({
                    "project": key,
                    "field": field,
                    "recorded": recorded,
                    "class": expected,
                }));
            }
        }

        let project_docs = by_project.get(id).cloned().unwrap_or_default();
        let evidenced: BTreeSet<&str> = project_docs
            .iter()
            .filter_map(|doc| doc.kind.as_deref())
            .collect();
        let missing_now: Vec<&String> = now
            .iter()
            .filter(|kind| !evidenced.contains(kind.as_str()))
            .collect();
        let missing_closure: Vec<&String> = closure
            .iter()
            .filter(|kind| !evidenced.contains(kind.as_str()))
            .collect();

        declared_total += now.len();
        evidenced_total += evidenced.len();
        missing_total += missing_now.len() + missing_closure.len();

        if let Some(class) = &class {
            let entry = class_rows
                .entry(class.name.clone())
                .or_insert_with(|| json!({"projects": [], "declared": class.now.len()}));
            if let Some(list) = entry.get_mut("projects").and_then(Value::as_array_mut) {
                list.push(Value::String(key.to_owned()));
            }
        }

        let mut documents: Vec<Value> = project_docs
            .iter()
            .map(|doc| {
                json!({
                    "id": doc.id,
                    "title": doc.title,
                    "kind": doc.kind,
                    "status": doc.status,
                    "version": doc.version,
                    "hash": doc.hash,
                    "path": doc.path(),
                })
            })
            .collect();
        documents.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));

        // The scope is the standard's one declaration: archived is history (`excluded`), a
        // tranche beyond `scope.through_tranche` is `scheduled`, everything else is `due`.
        let archived = meta.get("measured_status").and_then(Value::as_str) == Some("archived");
        let tranche_number = meta.get("tranche").and_then(Value::as_u64);
        let due_state = if archived {
            "excluded"
        } else if tranche_number.is_some_and(|number| number > through_tranche) {
            "scheduled"
        } else {
            "due"
        };
        // One row per declared kind: the campaign's work list, and the drill-down a reader
        // needs to know whether a missing kind is evidenced, missing, or out of scope.
        let mut kinds: Vec<Value> = Vec::new();
        for (field, declared) in [("now", &now), ("closure", &closure)] {
            for kind in declared {
                kinds.push(json!({
                    "kind": kind,
                    "due": field,
                    "state": if evidenced.contains(kind.as_str()) {
                        "evidenced"
                    } else {
                        "missing"
                    },
                    "pack": format!("docs/evidence/rework/{key}.json"),
                    "acceptance": format!("python3 scripts/verify-project.py --project {key}"),
                }));
            }
        }

        rows.push(json!({
            "id": id,
            "key": key,
            "name": project.get("name").cloned().unwrap_or(Value::Null),
            "status": status,
            "tranche": meta.get("tranche").cloned().unwrap_or(Value::Null),
            // The unit numbers the project records: the register's drill-down to GitHub, so a
            // gap leads to the issue that owns it rather than to a bare project name.
            "issues": meta.get("issues").cloned().unwrap_or(Value::Null),
            "class": class_name,
            "due_state": due_state,
            "declared": {"now": now, "closure": closure},
            "kinds": kinds,
            "evidenced": evidenced.iter().map(|kind| Value::String((*kind).to_owned())).collect::<Vec<_>>(),
            "missing": {"now": missing_now, "closure": missing_closure},
            "documents": documents,
        }));
    }

    let mut unassigned_docs: Vec<Value> = unassigned
        .iter()
        .map(|doc| {
            json!({
                "id": doc.id,
                "title": doc.title,
                "kind": doc.kind,
                "status": doc.status,
                "version": doc.version,
                "hash": doc.hash,
                "path": doc.path(),
            })
        })
        .collect();
    unassigned_docs.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));

    let classes: Vec<Value> = class_rows
        .into_iter()
        .map(|(name, row)| {
            json!({
                "name": name,
                "declared": row.get("declared").cloned().unwrap_or(Value::Null),
                "projects": row.get("projects").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();

    json!({
        "standard_version": standard_version(),
        "categories": output_standard::categories().len(),
        "scope": {"through_tranche": through_tranche},
        "projects": rows,
        "classes": classes,
        "drift": drift,
        "unassigned": unassigned_docs,
        "totals": {
            "projects": projects.len(),
            "projects_without_class": projects_without_class,
            "declared": declared_total,
            "evidenced": evidenced_total,
            "missing": missing_total,
            "documents": docs.len(),
        },
        "gaps": gaps.map(gap_posture).unwrap_or(Value::Null),
    })
}

fn standard_version() -> Value {
    // The standard's own version, read through the module that owns the embedded copy: a
    // register that quoted a different version would describe a checklist nobody publishes.
    json!(output_standard::standard_version())
}

/// The gap register's posture: the counts it declares and the rules they come from.
fn gap_posture(gaps: &Value) -> Value {
    let rows = gaps
        .get("gaps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_rule: BTreeMap<(String, String), usize> = BTreeMap::new();
    for row in &rows {
        let severity = row
            .get("severity")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned();
        let rule = row
            .get("rule")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_owned();
        *counts.entry(severity.clone()).or_default() += 1;
        *by_rule.entry((severity, rule)).or_default() += 1;
    }
    let by_rule: Vec<Value> = by_rule
        .into_iter()
        .map(
            |((severity, rule), count)| json!({"severity": severity, "rule": rule, "count": count}),
        )
        .collect();
    json!({
        "source": GAP_REGISTER,
        "counts": counts,
        "total": rows.len(),
        "by_rule": by_rule,
    })
}

/// The committed gap register, when this checkout carries one.
///
/// The CLI publish runs from a checkout, so it does; the live view runs in a pod that may carry no
/// `docs/` tree, and an absent register is `None` rather than a failure: the posture line says
/// "not recorded" instead of inventing zero gaps.
pub(crate) fn read_gaps(root: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(root.join(GAP_REGISTER)).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::{PublishedDoc, build, gap_posture};
    use serde_json::json;

    fn programme() -> serde_json::Value {
        json!({
            "projects": [
                {
                    "id": "p0-id",
                    "key": "P0",
                    "name": "Programme mandate",
                    "status": "active",
                    "meta": {"track": "p0"},
                },
                {
                    "id": "t17-id",
                    "key": "T17",
                    "name": "Tenant asset authoring",
                    "status": "active",
                    "meta": {"track": "v2"},
                },
                {
                    "id": "odd-id",
                    "key": "ODD",
                    "name": "No class matches this",
                    "status": "active",
                    "meta": {},
                },
            ],
        })
    }

    /// P2: the register reconciles with the outputs index — every published row appears exactly
    /// once, and a declared-but-unpublished kind is a gap, never evidence.
    #[test]
    fn snapshot_register_reconciles_with_outputs() {
        let declared = conveyor_domain::output_standard::class_for(
            &[("key".to_owned(), "P0".to_owned())].into_iter().collect(),
        )
        .expect("the p0 class");
        let first = declared.now[0].clone();
        let docs = [
            PublishedDoc {
                id: "doc-b".to_owned(),
                project_id: Some("p0-id".to_owned()),
                kind: Some(first.clone()),
                title: "The mandate".to_owned(),
                status: "active".to_owned(),
                version: 3,
                hash: Some("abc".to_owned()),
            },
            PublishedDoc {
                id: "doc-a".to_owned(),
                project_id: Some("p0-id".to_owned()),
                kind: Some("not-a-declared-kind".to_owned()),
                title: "An on-demand note".to_owned(),
                status: "active".to_owned(),
                version: 1,
                hash: None,
            },
            PublishedDoc {
                id: "doc-c".to_owned(),
                project_id: None,
                kind: Some(first.clone()),
                title: "A tenant note".to_owned(),
                status: "active".to_owned(),
                version: 2,
                hash: None,
            },
        ];
        let register = build(&programme(), &docs, None);
        assert_eq!(
            register["totals"]["documents"], 3,
            "every published row is counted once"
        );
        assert_eq!(register["unassigned"][0]["id"], "doc-c");
        let p0 = &register["projects"][0];
        assert_eq!(p0["class"], "p0");
        assert_eq!(p0["documents"].as_array().expect("documents").len(), 2);
        assert!(
            p0["evidenced"]
                .as_array()
                .expect("evidenced")
                .iter()
                .any(|kind| kind == &json!(first)),
            "the published row evidences its kind"
        );
        assert!(
            !p0["missing"]["now"]
                .as_array()
                .expect("missing now")
                .iter()
                .any(|kind| kind == &json!(first)),
            "an evidenced kind is not missing"
        );
        assert_eq!(
            register["projects"][2]["class"],
            serde_json::Value::Null,
            "a project no class matches says so"
        );
        assert_eq!(register["totals"]["projects_without_class"], 1);
    }

    /// The invariant the retired checker carried: the seed's recorded sets must agree with the
    /// class the register resolves. A disagreement is published, never hidden.
    #[test]
    fn a_recorded_set_that_disagrees_with_its_class_is_a_gap() {
        let mut programme = programme();
        programme["projects"][0]["meta"]["expected_outputs"] = json!(["not-the-class-set"]);
        let register = build(&programme, &[], None);
        assert_eq!(register["drift"][0]["project"], "P0");
        assert_eq!(register["drift"][0]["field"], "expected_outputs");
    }

    /// The register carries the per-kind target and the scope, so the campaign and the page read
    /// one list: evidenced/missing per declared kind, the pack that owns it, and whether the
    /// project is due at all.
    #[test]
    fn the_register_carries_the_per_kind_target_and_the_scope() {
        let mut programme = programme();
        programme["projects"][0]["meta"]["tranche"] = json!(0);
        programme["projects"][1]["meta"]["tranche"] = json!(9);
        let register = build(&programme, &[], None);
        assert_eq!(register["scope"]["through_tranche"], json!(8));
        let p0 = &register["projects"][0];
        assert_eq!(p0["due_state"], "due");
        let kinds = p0["kinds"].as_array().expect("kinds");
        assert!(
            kinds
                .iter()
                .any(|row| row["kind"] == "project-brief" && row["due"] == "now"),
            "a declared kind is a row: {kinds:?}"
        );
        assert!(
            kinds
                .iter()
                .all(|row| row["pack"] == "docs/evidence/rework/P0.json"),
            "every kind names its project's pack"
        );
        assert!(
            kinds.iter().all(|row| row["state"] == "missing"),
            "with no published rows every kind is missing"
        );
        let t17 = &register["projects"][1];
        assert_eq!(
            t17["due_state"], "scheduled",
            "tranche 9 is beyond the scope"
        );
    }

    /// The posture is computed from the register's own rows, so a stale count cannot be quoted.
    #[test]
    fn the_gap_posture_counts_severity_and_rule() {
        let gaps = json!({"gaps": [
            {"severity": "P0", "rule": "closed_without_closure_review"},
            {"severity": "P0", "rule": "closed_without_closure_review"},
            {"severity": "P1", "rule": "missing_required_output"},
        ]});
        let posture = gap_posture(&gaps);
        assert_eq!(posture["counts"]["P0"], 2);
        assert_eq!(posture["counts"]["P1"], 1);
        assert_eq!(posture["total"], 3);
        assert_eq!(posture["by_rule"][0]["count"], 2);
        assert_eq!(posture["source"], super::GAP_REGISTER);
    }
}
