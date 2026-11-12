//! Profile validation — the checks `models.validate` runs in the worker (S08 §5.8).
//!
//! This module is pure: it takes a [`ModelSnapshot`], the [`Profile`] the model was created
//! against, and the governed assets the snapshot's elements claim to represent. The worker
//! resolves those assets in one query and passes the map in, so `conveyor-domain` stays free
//! of IO (CONTRACT §1).
//!
//! Findings are **sorted** before they leave here — by pointer, then code — so the same
//! snapshot always produces the same `model_validation_run.findings` document. The job's
//! dedupe key is the snapshot's content hash, so an unstable report would make "the same
//! graph produced a different verdict" possible.
//!
//! Write-time rejection and validation are deliberately different instruments. A `kind` the
//! profile does not contain is refused at the door with a `422`; everything that can only be
//! judged once the whole graph is in view — endpoints, containment, cycles, dangling view
//! connectors, asset links — is a finding here (slice trap 6: a full graph check must not
//! hold the model's row lock inside the mutation).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{ModelSnapshot, SnapshotElement};
use crate::profile::{Layer, Profile, RelationshipRule};

/// How a finding should be read. `Error` means the model is not publishable as-is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// A profile or integrity violation.
    Error,
    /// Legal but worth surfacing (a title that has drifted from its governed asset).
    Warning,
}

/// The stable finding vocabulary. The string form is what lands in `findings[].code`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCode {
    /// The element or relationship kind is not in the model's profile.
    OutOfProfileKind,
    /// The relationship's ends violate the kind's rule.
    IllegalRelationshipEndpoints,
    /// The containment parent is missing or in a layer that may not contain the child.
    IllegalParent,
    /// The containment links form a cycle.
    ParentCycle,
    /// A relationship names an element the model does not hold.
    DanglingRelationship,
    /// A view connection names a relationship the model does not hold.
    ViewConnectionMissingRelationship,
    /// A view connection names an object that is not on its view.
    DanglingViewObject,
    /// An element names a governed asset that is not in the tenant.
    GovernedAssetMissing,
    /// An element's name has drifted from the governed asset's title.
    GovernedAssetTitleMismatch,
}

impl FindingCode {
    /// The string stored in `findings[].code`.
    pub fn as_str(self) -> &'static str {
        match self {
            FindingCode::OutOfProfileKind => "out_of_profile_kind",
            FindingCode::IllegalRelationshipEndpoints => "illegal_relationship_endpoints",
            FindingCode::IllegalParent => "illegal_parent",
            FindingCode::ParentCycle => "parent_cycle",
            FindingCode::DanglingRelationship => "dangling_relationship",
            FindingCode::ViewConnectionMissingRelationship => {
                "view_connection_missing_relationship"
            }
            FindingCode::DanglingViewObject => "dangling_view_object",
            FindingCode::GovernedAssetMissing => "governed_asset_missing",
            FindingCode::GovernedAssetTitleMismatch => "governed_asset_title_mismatch",
        }
    }
}

/// One thing wrong with a model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// What kind of problem this is.
    pub code: FindingCode,
    /// How serious it is.
    pub severity: Severity,
    /// The element the finding is about, when it is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_id: Option<Uuid>,
    /// The relationship the finding is about, when it is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<Uuid>,
    /// A stable path into the snapshot, e.g. `/elements/<uuid>/kind`.
    pub pointer: String,
    /// A human sentence naming the offending value.
    pub message: String,
}

/// A governed asset an element may represent, resolved by the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedAssetRef {
    /// `governed_asset.kind` — `requirement`, `principle` or `adr`.
    pub kind: String,
    /// `governed_asset.title`, which the element's name must still match.
    pub title: String,
}

/// Run every check. An empty result means the model is clean.
///
/// `assets` is keyed by `governed_asset.id`; the caller resolves every id named by an
/// element's `meta.governed_asset_id` in one in-tenant query. A referenced asset that is not
/// in the map is `governed_asset_missing` — the map is the in-tenant truth, so "absent" and
/// "belongs to another tenant" are the same answer.
pub fn check(
    snapshot: &ModelSnapshot,
    profile: &Profile,
    assets: &HashMap<Uuid, GovernedAssetRef>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut by_id: BTreeMap<Uuid, &SnapshotElement> = BTreeMap::new();
    for element in &snapshot.elements {
        by_id.insert(element.id, element);
    }

    let layer_of = |element: &SnapshotElement| -> Option<Layer> {
        profile.element(&element.kind).and_then(|entry| entry.layer)
    };

    check_elements(snapshot, profile, assets, &by_id, &layer_of, &mut findings);
    check_relationships(snapshot, profile, &by_id, &layer_of, &mut findings);
    check_views(snapshot, profile, &mut findings);

    // Deterministic output: two runs over the same graph produce the same document.
    findings.sort_by(|a, b| {
        a.pointer
            .cmp(&b.pointer)
            .then_with(|| a.code.as_str().cmp(b.code.as_str()))
    });
    findings
}

fn check_elements(
    snapshot: &ModelSnapshot,
    profile: &Profile,
    assets: &HashMap<Uuid, GovernedAssetRef>,
    by_id: &BTreeMap<Uuid, &SnapshotElement>,
    layer_of: &dyn Fn(&SnapshotElement) -> Option<Layer>,
    findings: &mut Vec<Finding>,
) {
    // Sorted so the cycle report below names the same element every run.
    let mut ids: Vec<Uuid> = snapshot.elements.iter().map(|e| e.id).collect();
    ids.sort_unstable();
    let parent_of: BTreeMap<Uuid, Option<Uuid>> = snapshot
        .elements
        .iter()
        .map(|element| (element.id, element.parent_element_id))
        .collect();

    for id in &ids {
        let element = by_id[id];

        if !profile.has_element_kind(&element.kind) {
            findings.push(Finding {
                code: FindingCode::OutOfProfileKind,
                severity: Severity::Error,
                element_id: Some(element.id),
                relationship_id: None,
                pointer: format!("/elements/{}/kind", element.id),
                message: format!(
                    "kind {:?} is not in profile {} — accepted kinds are listed in the ruleset",
                    element.kind, profile.ruleset
                ),
            });
        }

        if let Some(parent_id) = element.parent_element_id {
            match by_id.get(&parent_id) {
                None => findings.push(Finding {
                    code: FindingCode::IllegalParent,
                    severity: Severity::Error,
                    element_id: Some(element.id),
                    relationship_id: None,
                    pointer: format!("/elements/{}/parent_element_id", element.id),
                    message: format!("parent element {parent_id} is not in this model"),
                }),
                Some(parent) => {
                    // ArchiMate containment stays inside a layer; a composite groups anything.
                    if let (Some(child_layer), Some(parent_layer)) =
                        (layer_of(element), layer_of(parent))
                        && child_layer != parent_layer
                        && parent_layer != Layer::Composite
                    {
                        findings.push(Finding {
                            code: FindingCode::IllegalParent,
                            severity: Severity::Error,
                            element_id: Some(element.id),
                            relationship_id: None,
                            pointer: format!("/elements/{}/parent_element_id", element.id),
                            message: format!(
                                "a {} element may not be contained by a {} element",
                                child_layer.as_str(),
                                parent_layer.as_str()
                            ),
                        });
                    }
                }
            }
        }

        check_governed_asset(element, assets, findings);
    }

    report_cycles(&ids, &parent_of, findings);
}

/// Report each containment cycle exactly once, on its lowest id.
fn report_cycles(
    ids: &[Uuid],
    parent_of: &BTreeMap<Uuid, Option<Uuid>>,
    findings: &mut Vec<Finding>,
) {
    let mut reported: BTreeSet<Uuid> = BTreeSet::new();
    for id in ids {
        let mut path: Vec<Uuid> = Vec::new();
        let mut cursor = Some(*id);
        let mut repeated: Option<usize> = None;
        while let Some(current) = cursor {
            if let Some(position) = path.iter().position(|seen| *seen == current) {
                repeated = Some(position);
                break;
            }
            path.push(current);
            cursor = parent_of.get(&current).copied().flatten();
        }
        let Some(position) = repeated else { continue };
        let cycle: BTreeSet<Uuid> = path[position..].iter().copied().collect();
        let Some(first) = cycle.iter().next().copied() else {
            continue;
        };
        if reported.insert(first) {
            findings.push(Finding {
                code: FindingCode::ParentCycle,
                severity: Severity::Error,
                element_id: Some(first),
                relationship_id: None,
                pointer: format!("/elements/{first}/parent_element_id"),
                message: format!(
                    "{} element(s) form a containment cycle; the parent chain never terminates",
                    cycle.len()
                ),
            });
        }
    }
}

fn check_governed_asset(
    element: &SnapshotElement,
    assets: &HashMap<Uuid, GovernedAssetRef>,
    findings: &mut Vec<Finding>,
) {
    let Some(raw) = element
        .meta
        .get("governed_asset_id")
        .and_then(serde_json::Value::as_str)
    else {
        return;
    };
    let Ok(asset_id) = Uuid::parse_str(raw) else {
        findings.push(Finding {
            code: FindingCode::GovernedAssetMissing,
            severity: Severity::Error,
            element_id: Some(element.id),
            relationship_id: None,
            pointer: format!("/elements/{}/meta/governed_asset_id", element.id),
            message: format!("{raw:?} is not a uuid"),
        });
        return;
    };
    match assets.get(&asset_id) {
        None => findings.push(Finding {
            code: FindingCode::GovernedAssetMissing,
            severity: Severity::Error,
            element_id: Some(element.id),
            relationship_id: None,
            pointer: format!("/elements/{}/meta/governed_asset_id", element.id),
            message: format!("governed asset {asset_id} is not in this tenant"),
        }),
        Some(asset) if asset.title != element.name => findings.push(Finding {
            code: FindingCode::GovernedAssetTitleMismatch,
            severity: Severity::Warning,
            element_id: Some(element.id),
            relationship_id: None,
            pointer: format!("/elements/{}/name", element.id),
            message: format!(
                "element name {:?} no longer matches the {} asset's title {:?}",
                element.name, asset.kind, asset.title
            ),
        }),
        Some(_) => {}
    }
}

fn check_relationships(
    snapshot: &ModelSnapshot,
    profile: &Profile,
    by_id: &BTreeMap<Uuid, &SnapshotElement>,
    layer_of: &dyn Fn(&SnapshotElement) -> Option<Layer>,
    findings: &mut Vec<Finding>,
) {
    for relationship in &snapshot.relationships {
        let Some(entry) = profile.relationship(&relationship.kind) else {
            findings.push(Finding {
                code: FindingCode::OutOfProfileKind,
                severity: Severity::Error,
                element_id: None,
                relationship_id: Some(relationship.id),
                pointer: format!("/relationships/{}/kind", relationship.id),
                message: format!(
                    "relationship kind {:?} is not in profile {}",
                    relationship.kind, profile.ruleset
                ),
            });
            continue;
        };

        let source = by_id.get(&relationship.source_id).copied();
        let target = by_id.get(&relationship.target_id).copied();
        if source.is_none() || target.is_none() {
            let missing = if source.is_none() {
                relationship.source_id
            } else {
                relationship.target_id
            };
            findings.push(Finding {
                code: FindingCode::DanglingRelationship,
                severity: Severity::Error,
                element_id: None,
                relationship_id: Some(relationship.id),
                pointer: format!("/relationships/{}/source_id", relationship.id),
                message: format!(
                    "element {missing} named by this relationship is not in the model"
                ),
            });
            continue;
        }
        let (source, target) = (source.expect("checked"), target.expect("checked"));

        let illegal = match entry.rule {
            RelationshipRule::Any => false,
            RelationshipRule::SameLayer => match (layer_of(source), layer_of(target)) {
                (Some(a), Some(b)) => a != b,
                // A UML kind has no layer; `SameLayer` is ArchiMate-only by construction.
                _ => false,
            },
            RelationshipRule::Kinds { sources, targets } => {
                sources.is_some_and(|allowed| !allowed.contains(&source.kind.as_str()))
                    || targets.is_some_and(|allowed| !allowed.contains(&target.kind.as_str()))
            }
        };
        if illegal {
            findings.push(Finding {
                code: FindingCode::IllegalRelationshipEndpoints,
                severity: Severity::Error,
                element_id: None,
                relationship_id: Some(relationship.id),
                pointer: format!("/relationships/{}/kind", relationship.id),
                message: format!(
                    "{}:{} -> {}:{} is not a legal {} in {}",
                    source.kind,
                    source.id,
                    target.kind,
                    target.id,
                    relationship.kind,
                    profile.ruleset
                ),
            });
        }
    }
}

fn check_views(snapshot: &ModelSnapshot, profile: &Profile, findings: &mut Vec<Finding>) {
    let relationship_ids: BTreeSet<Uuid> = snapshot
        .relationships
        .iter()
        .map(|relationship| relationship.id)
        .collect();

    for view in &snapshot.views {
        if !profile.has_view_kind(&view.kind) {
            findings.push(Finding {
                code: FindingCode::OutOfProfileKind,
                severity: Severity::Error,
                element_id: None,
                relationship_id: None,
                pointer: format!("/views/{}/kind", view.id),
                message: format!(
                    "view kind {:?} is not in profile {}",
                    view.kind, profile.ruleset
                ),
            });
        }
        if let Some(diagram) = view
            .meta
            .get("diagram_kind")
            .and_then(serde_json::Value::as_str)
            && !profile.has_diagram_kind(diagram)
        {
            findings.push(Finding {
                code: FindingCode::OutOfProfileKind,
                severity: Severity::Error,
                element_id: None,
                relationship_id: None,
                pointer: format!("/views/{}/meta/diagram_kind", view.id),
                message: format!(
                    "diagram kind {diagram:?} is not in profile {}",
                    profile.ruleset
                ),
            });
        }

        let object_ids: BTreeSet<Uuid> = view.objects.iter().map(|object| object.id).collect();
        for connection in &view.connections {
            if let Some(relationship_id) = connection.relationship_id
                && !relationship_ids.contains(&relationship_id)
            {
                findings.push(Finding {
                    code: FindingCode::ViewConnectionMissingRelationship,
                    severity: Severity::Error,
                    element_id: None,
                    relationship_id: Some(relationship_id),
                    pointer: format!("/views/{}/connections/{}", view.id, connection.id),
                    message: format!(
                        "connection draws relationship {relationship_id}, which is not in the model"
                    ),
                });
            }
            for end in [connection.source_object_id, connection.target_object_id] {
                if !object_ids.contains(&end) {
                    findings.push(Finding {
                        code: FindingCode::DanglingViewObject,
                        severity: Severity::Error,
                        element_id: None,
                        relationship_id: None,
                        pointer: format!("/views/{}/connections/{}", view.id, connection.id),
                        message: format!(
                            "connection names object {end}, which is not on this view"
                        ),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ModelSnapshot, SnapshotConnection, SnapshotModel, SnapshotObject, SnapshotRelationship,
        SnapshotView,
    };
    use crate::profile::archimate;
    use serde_json::json;

    fn element(kind: &str, id: Uuid, name: &str) -> SnapshotElement {
        SnapshotElement {
            id,
            kind: kind.to_owned(),
            name: name.to_owned(),
            documentation: String::new(),
            parent_element_id: None,
            meta: json!({}),
        }
    }

    fn snapshot(elements: Vec<SnapshotElement>) -> ModelSnapshot {
        ModelSnapshot {
            model: SnapshotModel {
                id: Uuid::now_v7(),
                language: "archimate".to_owned(),
                language_version: "3.2".to_owned(),
                title: "Model".to_owned(),
                description: String::new(),
                meta: json!({}),
            },
            elements,
            relationships: Vec::new(),
            views: Vec::new(),
        }
    }

    fn codes(findings: &[Finding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.code.as_str()).collect()
    }

    #[test]
    fn a_clean_model_has_no_findings() {
        let mut snapshot = snapshot(vec![
            element("application_component", Uuid::now_v7(), "API"),
            element("application_service", Uuid::now_v7(), "Serve"),
        ]);
        let (source, target) = (snapshot.elements[0].id, snapshot.elements[1].id);
        snapshot.relationships.push(SnapshotRelationship {
            id: Uuid::now_v7(),
            kind: "serving".to_owned(),
            source_id: source,
            target_id: target,
            meta: json!({}),
        });
        let findings = check(&snapshot, &archimate::PROFILE, &HashMap::new());
        assert!(findings.is_empty(), "unexpected findings: {findings:?}");
    }

    #[test]
    fn an_out_of_profile_kind_is_reported_by_name() {
        let snapshot = snapshot(vec![element("business_interaction", Uuid::now_v7(), "X")]);
        let findings = check(&snapshot, &archimate::PROFILE, &HashMap::new());
        assert_eq!(codes(&findings), vec!["out_of_profile_kind"]);
        assert!(findings[0].message.contains("business_interaction"));
    }

    #[test]
    fn a_cross_layer_composition_is_an_illegal_endpoint() {
        let mut snapshot = snapshot(vec![
            element("business_actor", Uuid::now_v7(), "Customer"),
            element("node", Uuid::now_v7(), "Server"),
        ]);
        let (a, b) = (snapshot.elements[0].id, snapshot.elements[1].id);
        snapshot.relationships.push(SnapshotRelationship {
            id: Uuid::now_v7(),
            kind: "composition".to_owned(),
            source_id: a,
            target_id: b,
            meta: json!({}),
        });
        let findings = check(&snapshot, &archimate::PROFILE, &HashMap::new());
        assert_eq!(codes(&findings), vec!["illegal_relationship_endpoints"]);
    }

    #[test]
    fn an_unrestricted_relationship_across_layers_is_legal() {
        let mut snapshot = snapshot(vec![
            element("business_service", Uuid::now_v7(), "Quote"),
            element("application_service", Uuid::now_v7(), "QuoteApi"),
        ]);
        let (a, b) = (snapshot.elements[0].id, snapshot.elements[1].id);
        snapshot.relationships.push(SnapshotRelationship {
            id: Uuid::now_v7(),
            kind: "realization".to_owned(),
            source_id: a,
            target_id: b,
            meta: json!({}),
        });
        let findings = check(&snapshot, &archimate::PROFILE, &HashMap::new());
        assert!(findings.is_empty(), "unexpected findings: {findings:?}");
    }

    #[test]
    fn a_dangling_relationship_names_its_own_id() {
        let mut snapshot = snapshot(vec![element("node", Uuid::now_v7(), "Server")]);
        let relationship_id = Uuid::now_v7();
        snapshot.relationships.push(SnapshotRelationship {
            id: relationship_id,
            kind: "serving".to_owned(),
            source_id: snapshot.elements[0].id,
            target_id: Uuid::now_v7(),
            meta: json!({}),
        });
        let findings = check(&snapshot, &archimate::PROFILE, &HashMap::new());
        assert_eq!(codes(&findings), vec!["dangling_relationship"]);
        assert_eq!(findings[0].relationship_id, Some(relationship_id));
    }

    #[test]
    fn a_containment_cycle_is_reported_once_on_its_lowest_id() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let mut first = element("node", a, "A");
        let mut second = element("node", b, "B");
        first.parent_element_id = Some(b);
        second.parent_element_id = Some(a);
        let snapshot = snapshot(vec![first, second]);
        let findings = check(&snapshot, &archimate::PROFILE, &HashMap::new());
        let cycles: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == FindingCode::ParentCycle)
            .collect();
        assert_eq!(cycles.len(), 1, "{findings:?}");
        let expected = if a < b { a } else { b };
        assert_eq!(cycles[0].element_id, Some(expected));
    }

    #[test]
    fn a_cross_layer_parent_is_illegal_unless_the_parent_is_composite() {
        let child = element("application_component", Uuid::now_v7(), "API");
        let parent = element("node", Uuid::now_v7(), "Server");

        let mut nested = child.clone();
        nested.parent_element_id = Some(parent.id);
        let illegal = snapshot(vec![nested, parent.clone()]);
        let findings = check(&illegal, &archimate::PROFILE, &HashMap::new());
        assert_eq!(codes(&findings), vec!["illegal_parent"]);

        let grouping = element("grouping", Uuid::now_v7(), "Everything");
        let mut nested = child;
        nested.parent_element_id = Some(grouping.id);
        let legal = snapshot(vec![nested, grouping]);
        let findings = check(&legal, &archimate::PROFILE, &HashMap::new());
        assert!(findings.is_empty(), "unexpected findings: {findings:?}");
    }

    #[test]
    fn a_view_connection_without_its_relationship_is_reported() {
        let element_id = Uuid::now_v7();
        let mut snapshot = snapshot(vec![element("node", element_id, "Server")]);
        let view_id = Uuid::now_v7();
        let object_id = Uuid::now_v7();
        let missing_relationship = Uuid::now_v7();
        snapshot.views.push(SnapshotView {
            id: view_id,
            kind: "archimate_view".to_owned(),
            name: "V".to_owned(),
            meta: json!({}),
            objects: vec![SnapshotObject {
                id: object_id,
                element_id: Some(element_id),
                parent_object_id: None,
                kind: "element".to_owned(),
                bounds: json!({"x":0,"y":0,"w":180,"h":90}),
                style: json!({}),
                label: None,
                position: 0.0,
            }],
            connections: vec![SnapshotConnection {
                id: Uuid::now_v7(),
                relationship_id: Some(missing_relationship),
                source_object_id: object_id,
                target_object_id: object_id,
                bendpoints: json!([]),
                style: json!({}),
            }],
        });
        let findings = check(&snapshot, &archimate::PROFILE, &HashMap::new());
        assert!(
            codes(&findings).contains(&"view_connection_missing_relationship"),
            "{findings:?}"
        );
    }

    #[test]
    fn a_governed_asset_element_is_checked_against_the_tenant() {
        let asset_id = Uuid::now_v7();
        let mut element = element("requirement", Uuid::now_v7(), "Fast");
        element.meta = json!({ "governed_asset_id": asset_id.to_string() });
        let snapshot = snapshot(vec![element]);

        let findings = check(&snapshot, &archimate::PROFILE, &HashMap::new());
        assert_eq!(codes(&findings), vec!["governed_asset_missing"]);

        let mut assets = HashMap::new();
        assets.insert(
            asset_id,
            GovernedAssetRef {
                kind: "requirement".to_owned(),
                title: "Slow".to_owned(),
            },
        );
        let findings = check(&snapshot, &archimate::PROFILE, &assets);
        assert_eq!(codes(&findings), vec!["governed_asset_title_mismatch"]);
        assert_eq!(findings[0].severity, Severity::Warning);

        assets.insert(
            asset_id,
            GovernedAssetRef {
                kind: "requirement".to_owned(),
                title: "Fast".to_owned(),
            },
        );
        assert!(check(&snapshot, &archimate::PROFILE, &assets).is_empty());
    }

    #[test]
    fn findings_are_sorted_so_two_runs_agree() {
        let mut snapshot = snapshot(vec![
            element("business_interaction", Uuid::now_v7(), "A"),
            element("business_event", Uuid::now_v7(), "B"),
            element("meaning", Uuid::now_v7(), "C"),
        ]);
        snapshot.normalize();
        let first = check(&snapshot, &archimate::PROFILE, &HashMap::new());
        let mut reordered = snapshot.clone();
        reordered.elements.reverse();
        reordered.normalize();
        let second = check(&reordered, &archimate::PROFILE, &HashMap::new());
        assert_eq!(first, second);
        assert_eq!(first.len(), 3);
    }
}
