//! The blueprint manifest: a blueprint is data, not code.
//!
//! Each of the sixteen blueprints in `docs/plans/04-TOOL-ESTATE-INVENTORY.md` §2 is committed
//! under `blueprints/` as an ordered phase list, and embedded here. Every phase names an action.
//! [`load`] refuses a phase naming an action this build does not register, naming the blueprint,
//! the phase and the action. A phase whose action is deliberately not built yet is declared by
//! id in the blueprint's `unresolved` array and reported in [`Manifest::unresolved`] — never
//! dropped.
//!
//! The registered names arrive as an argument rather than through a dependency:
//! `conveyor-actions` carries the store and the HTTP client, and this crate's dependency tree
//! must stay free of transport and database crates. Tests pass
//! `conveyor_actions::registry().names()`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What the blueprint's output is for (plan 04 §2): evidence, or a published artefact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// Evidence for a gate or a QC phase.
    Analysis,
    /// A published deliverable.
    Production,
}

/// One step of a blueprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Phase {
    /// Stable identifier, unique within the blueprint; the unresolved declaration names it.
    pub id: String,
    /// The action this phase dispatches to.
    pub action: String,
    /// The action's input, verbatim. The manifest never interprets it.
    pub params: Value,
}

/// A deliverable blueprint: the conveyor's project-type vocabulary.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Blueprint {
    pub key: String,
    pub title: String,
    pub family: Family,
    /// The full phase sequence in order, including phases declared unresolved.
    pub phases: Vec<Phase>,
}

/// A phase whose action this build does not register yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedPhase {
    pub blueprint: String,
    pub phase: String,
    pub action: String,
}

/// One committed document, loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedBlueprint {
    pub blueprint: Blueprint,
    pub unresolved: Vec<UnresolvedPhase>,
}

/// The committed set, loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    /// The blueprints, sorted by key so the set is deterministic.
    pub blueprints: Vec<Blueprint>,
    /// Every unresolved phase, in blueprint-key then phase order.
    pub unresolved: Vec<UnresolvedPhase>,
}

/// Why a committed blueprint was refused.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// A phase names an action that is neither registered nor declared unresolved.
    #[error(
        "blueprint `{blueprint}` phase `{phase}` names action `{action}`, which is not registered"
    )]
    UnknownAction {
        blueprint: String,
        phase: String,
        action: String,
    },
    /// An unresolved declaration names a phase the blueprint does not have.
    #[error("blueprint `{blueprint}` declares phase `{phase}` unresolved, but has no such phase")]
    UnknownPhase { blueprint: String, phase: String },
    /// A phase id appears twice in one blueprint.
    #[error("blueprint `{blueprint}` declares phase `{phase}` twice")]
    DuplicatePhase { blueprint: String, phase: String },
    /// An unresolved declaration repeats a phase.
    #[error("blueprint `{blueprint}` declares phase `{phase}` unresolved twice")]
    DuplicateUnresolved { blueprint: String, phase: String },
    /// The document is not JSON in the manifest shape.
    #[error("blueprint document is not a valid manifest: {0}")]
    InvalidDocument(#[from] serde_json::Error),
}

/// The committed document as written: a blueprint plus its explicit unresolved declaration.
///
/// The declaration permits a missing action; it is not a second source of truth. Once the action
/// is registered the phase resolves and leaves the reported list, and the expectation test fails
/// so the committed list is updated in the same change.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    key: String,
    title: String,
    family: Family,
    phases: Vec<Phase>,
    unresolved: Vec<String>,
}

/// Load one committed document against the registered action names.
///
/// A phase naming an unregistered action is refused unless the document declares it unresolved.
/// The refusal names the blueprint, the phase and the action, so a typo is actionable without
/// opening the file.
pub fn load(json: &str, registered: &[&str]) -> Result<LoadedBlueprint, ManifestError> {
    let document: Document = serde_json::from_str(json)?;

    let mut ids = BTreeSet::new();
    for phase in &document.phases {
        if !ids.insert(phase.id.clone()) {
            return Err(ManifestError::DuplicatePhase {
                blueprint: document.key.clone(),
                phase: phase.id.clone(),
            });
        }
    }

    let mut declared = BTreeSet::new();
    for id in &document.unresolved {
        if !ids.contains(id) {
            return Err(ManifestError::UnknownPhase {
                blueprint: document.key.clone(),
                phase: id.clone(),
            });
        }
        if !declared.insert(id.clone()) {
            return Err(ManifestError::DuplicateUnresolved {
                blueprint: document.key.clone(),
                phase: id.clone(),
            });
        }
    }

    let known: BTreeSet<&str> = registered.iter().copied().collect();
    let mut unresolved = Vec::new();
    for phase in &document.phases {
        if known.contains(phase.action.as_str()) {
            continue;
        }
        if !declared.contains(&phase.id) {
            return Err(ManifestError::UnknownAction {
                blueprint: document.key.clone(),
                phase: phase.id.clone(),
                action: phase.action.clone(),
            });
        }
        unresolved.push(UnresolvedPhase {
            blueprint: document.key.clone(),
            phase: phase.id.clone(),
            action: phase.action.clone(),
        });
    }

    Ok(LoadedBlueprint {
        blueprint: Blueprint {
            key: document.key,
            title: document.title,
            family: document.family,
            phases: document.phases,
        },
        unresolved,
    })
}

/// The committed documents, embedded so a change to a file is a change to what this crate
/// reports: `include_str!` is tracked by cargo.
const BLUEPRINT_JSON: [&str; 16] = [
    include_str!("../blueprints/action_reel.json"),
    include_str!("../blueprints/app_review.json"),
    include_str!("../blueprints/audiobook.json"),
    include_str!("../blueprints/audit_video.json"),
    include_str!("../blueprints/course.json"),
    include_str!("../blueprints/cro_audit.json"),
    include_str!("../blueprints/demo_video.json"),
    include_str!("../blueprints/generic.json"),
    include_str!("../blueprints/growth_bundle.json"),
    include_str!("../blueprints/lesson.json"),
    include_str!("../blueprints/marketing_scorecard.json"),
    include_str!("../blueprints/promo.json"),
    include_str!("../blueprints/seo_audit.json"),
    include_str!("../blueprints/web_design.json"),
    include_str!("../blueprints/web_design_v2.json"),
    include_str!("../blueprints/website_review.json"),
];

/// Load the committed sixteen against the registered action names.
pub fn load_all(registered: &[&str]) -> Result<Manifest, ManifestError> {
    let mut loaded = Vec::with_capacity(BLUEPRINT_JSON.len());
    for json in BLUEPRINT_JSON {
        loaded.push(load(json, registered)?);
    }
    loaded.sort_by(|left, right| left.blueprint.key.cmp(&right.blueprint.key));

    let mut blueprints = Vec::with_capacity(loaded.len());
    let mut unresolved = Vec::new();
    for item in loaded {
        blueprints.push(item.blueprint);
        unresolved.extend(item.unresolved);
    }
    Ok(Manifest {
        blueprints,
        unresolved,
    })
}
