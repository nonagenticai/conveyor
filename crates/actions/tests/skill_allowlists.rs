//! Every role playbook's `allowed-actions` names a real, machine-grantable action.
//!
//! #781: the role playbooks act through `/api/actions/<name>/invocations` with the session
//! bearer. An action name in a playbook is a claim on the registry, and a wrong one fails only
//! live: `404`/`422` for a name the registry does not have, `403 action-forbidden` for one the
//! machine floor closes however the tenant grants it. This test reads the playbooks and checks
//! both, so a failure names the file and the action instead of a live call.
//!
//! The live half (P4.2) is the bearer probe. It needs the tenant's stored grant set:
//!
//! ```bash
//! conveyor auth token --tenant nonagentic
//! TOKEN=$(cat ~/.conveyor/nonagentic.token)
//! for action in development_projects.get documents.get; do
//!   curl -s -o /dev/null -w "%{http_code} $action\n" \
//!     -X POST "$CONVEYOR_URL/api/actions/$action/invocations" \
//!     -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
//!     -d '{"input":{}}'
//! done
//! ```
//!
//! A `403` is a missing grant — one `channels.policy_set` call, which is a person's act (the
//! `channels` resource is closed to every machine) — never a playbook edit. Any other status
//! proves the route and the grant.

use std::fs;
use std::path::{Path, PathBuf};

/// The playbooks that must declare their action set. The `issue-*` skills act through git and
/// `gh`, not the action route, and are deliberately not here.
const ROLE_PLAYBOOKS: [&str; 8] = [
    "benefits-manager",
    "business-analyst",
    "delivery-unit",
    "programme-architect",
    "programme-manager",
    "programme-sync",
    "quality-manager",
    "release-manager",
];

fn skills_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".claude")
        .join("skills")
}

/// The `allowed-actions` list in a skill's YAML frontmatter; empty when the key is absent.
fn allowed_actions(text: &str) -> Result<Vec<String>, String> {
    let rest = text
        .strip_prefix("---\n")
        .ok_or("the file does not start with a `---` frontmatter block")?;
    let (yaml, _) = rest
        .split_once("\n---")
        .ok_or("the frontmatter block is not closed")?;
    let value: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|error| format!("the frontmatter is not YAML: {error}"))?;
    let mapping = value
        .as_mapping()
        .ok_or("the frontmatter is not a mapping")?;
    let key = serde_yaml::Value::String("allowed-actions".to_owned());
    let Some(entries) = mapping.get(&key) else {
        return Ok(Vec::new());
    };
    let sequence = entries
        .as_sequence()
        .ok_or("`allowed-actions` is not a list")?;
    sequence
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "an `allowed-actions` entry is not a string".to_owned())
        })
        .collect()
}

/// The reasons a machine caller cannot honour these names; empty when every one is grantable.
fn violations(actions: &[String]) -> Vec<String> {
    let registry = conveyor_actions::registry::registry();
    let mut found = Vec::new();
    for action in actions {
        match registry.get(action) {
            None => found.push(format!("{action}: not in the action registry")),
            Some(spec) if spec.machine_forbidden() => found.push(format!(
                "{action}: machine-forbidden (the scope/resource floor closes it to every \
                 machine channel)"
            )),
            Some(_) => {}
        }
    }
    found
}

#[test]
fn every_role_playbook_names_only_machine_grantable_actions() {
    let root = skills_root();
    let mut problems = Vec::new();
    for name in ROLE_PLAYBOOKS {
        let path = root.join(name).join("SKILL.md");
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                problems.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        match allowed_actions(&text) {
            Ok(actions) if actions.is_empty() => {
                problems.push(format!("{}: declares no `allowed-actions`", path.display()));
            }
            Ok(actions) => {
                for violation in violations(&actions) {
                    problems.push(format!("{}: {violation}", path.display()));
                }
            }
            Err(error) => problems.push(format!("{}: {error}", path.display())),
        }
    }
    assert!(
        problems.is_empty(),
        "role playbooks must name machine-grantable actions:\n{}",
        problems.join("\n")
    );
}

/// The negative controls: an unknown name and a machine-forbidden one are both caught, so the
/// test above is known to be able to fail.
#[test]
fn an_unknown_action_and_a_forbidden_action_are_both_caught() {
    let unknown = violations(&["development_projects.not_an_action".to_owned()]);
    assert_eq!(unknown.len(), 1, "{unknown:?}");
    assert!(
        unknown[0].contains("not in the action registry"),
        "{unknown:?}"
    );

    let forbidden = violations(&["lessons.review".to_owned()]);
    assert_eq!(forbidden.len(), 1, "{forbidden:?}");
    assert!(forbidden[0].contains("machine-forbidden"), "{forbidden:?}");
}
