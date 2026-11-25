#![forbid(unsafe_code)]
//! The estate import wave's manifests (SPEC_V2 §11.2, §11.8; T14, #83).
//!
//! One JSON file per toolkit under `seed/toolkits/`, embedded at build time — the same rule the
//! capability and catalogue seeds follow: the set of published toolkits is a reviewable file in
//! the repository, never a row somebody typed into a production database.
//!
//! A manifest names the toolkit's **destination service** and, per tool, the **registry action**
//! the broker dispatches. It never carries an input schema: the schema is copied from the live
//! `ActionSpec` at publish time, so a tool cannot drift from the action it dispatches. What the
//! manifest does carry is the one thing the registry cannot know — the destination's health path,
//! which is the probe's subject, and the owner a zero-use report asks about the toolkit.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::spec::{Effect, HandlerRef};

/// Every embedded manifest, in call order (the toolkits a job may use first).
const RAW: &[(&str, &str)] = &[
    (
        "coding",
        include_str!("../../../../seed/toolkits/coding.json"),
    ),
    (
        "speech",
        include_str!("../../../../seed/toolkits/speech.json"),
    ),
    (
        "transcription",
        include_str!("../../../../seed/toolkits/transcription.json"),
    ),
    (
        "image_gen",
        include_str!("../../../../seed/toolkits/image_gen.json"),
    ),
    (
        "retrieval",
        include_str!("../../../../seed/toolkits/retrieval.json"),
    ),
    // T14 (#560): the scientific processor's read surface, published as a draft until the
    // service's deployment is scaled up and its probe is green.
    (
        "scientific",
        include_str!("../../../../seed/toolkits/scientific.json"),
    ),
    // T14 (#561): the webscraper and the Lean prover, each a draft until its probe is green.
    // The media service is not here: its `/api/*` routes need an `X-Tenant-Id` header the
    // external engine cannot send, so its five operations are recorded as drops in the
    // provenance seed rather than published as a toolkit.
    (
        "webscraper",
        include_str!("../../../../seed/toolkits/webscraper.json"),
    ),
    (
        "lean4",
        include_str!("../../../../seed/toolkits/lean4.json"),
    ),
    // The `artifacts`, `billing` and `worker_management` manifests were removed by #450: their
    // actions are internal now, and a toolkit binds External handlers only (SPEC_V2 §11.2,
    // §11.3, D57; #46).
];

/// One toolkit's published declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolkitManifest {
    /// The callable prefix (`coding`); `toolkit.key`'s shape constraint applies.
    pub key: String,
    /// The destination service alias; its base URL is `CONVEYOR_EXTERNAL_<SERVICE>_URL`.
    pub service: String,
    /// The human title.
    pub title: String,
    /// Who is asked about a zero-use tool.
    pub owner: String,
    /// `bearer | none | sealed-profile`.
    pub auth_adapter: String,
    /// The licence application, when the toolkit is licence-gated.
    pub licence_app: Option<String>,
    /// `published` (bindable and callable) or `draft` (unbound and uncallable).
    pub status: String,
    /// The destination's read-only health path, relative to the service base URL.
    pub health_path: String,
    /// The operations this toolkit publishes, in declaration order.
    pub tools: Vec<ToolManifest>,
}

/// One operation inside a toolkit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolManifest {
    /// The operation key inside the toolkit (`jobs_create`).
    pub key: String,
    /// The registry action the broker dispatches (`coding_jobs.create`).
    pub action: String,
    /// The idempotency rule the published tool carries (`required | optional | forbidden`).
    /// Absent is `optional`; a write or destructive tool must declare `required`.
    #[serde(default)]
    pub idempotency: Option<String>,
    /// Response fields the broker replaces with `[redacted]` (SPEC_V2 §11.3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redact: Vec<String>,
    /// The sibling tool key that reads the written object back. Required for a write or
    /// destructive tool; the named sibling must be a `read` action.
    #[serde(default)]
    pub verification: Option<String>,
    /// How the broker proves the write happened; absent only on non-writing tools (§11.3).
    /// The richer declaration (verdict + checks, or read_back); `verification` above is the
    /// sibling-key form. A write tool declares at least one of the two.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<VerifyManifest>,
}

/// The verification path a write tool declares (SPEC_V2 §11.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyManifest {
    /// The response member carrying a pass/fail verdict (synchronous writes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// The response member carrying read-back `checks[]` (synchronous writes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<String>,
    /// The tool key of a read-back tool in THIS toolkit (asynchronous writes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_back: Option<String>,
}

/// Why a manifest cannot be published.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// The embedded JSON does not parse — a build defect, never a caller's.
    #[error("seed/toolkits/{file}.json does not parse: {detail}")]
    Unreadable {
        /// The file stem.
        file: String,
        /// The parser's message.
        detail: String,
    },
    /// The key is not the shape `toolkit.key` allows.
    #[error("toolkit key {key:?} must match ^[a-z][a-z0-9_]*$")]
    KeyShape {
        /// The offending key.
        key: String,
    },
    /// Two manifests declare the same key.
    #[error("toolkit {key:?} is declared more than once")]
    Duplicate {
        /// The duplicated key.
        key: String,
    },
    /// A toolkit with no operations would be a catalogue row that can never be called.
    #[error("toolkit {key:?} declares no tools")]
    Empty {
        /// The empty toolkit's key.
        key: String,
    },
    /// Two tools inside one toolkit share a key.
    #[error("toolkit {key:?} declares tool {tool:?} more than once")]
    DuplicateTool {
        /// The toolkit.
        key: String,
        /// The duplicated tool key.
        tool: String,
    },
    /// The named action is not in the registry.
    #[error("{action:?} is not a registered action")]
    UnknownAction {
        /// The action the manifest named.
        action: String,
    },
    /// The named action is internal or workflow-shaped; only an external dispatch is a tool.
    #[error("{action:?} is not an external action")]
    NotExternal {
        /// The action the manifest named.
        action: String,
    },
    /// The action dispatches to a different service than the toolkit declares.
    #[error("{action:?} dispatches to {actual:?}, not the toolkit's service {declared:?}")]
    ServiceMismatch {
        /// The action.
        action: String,
        /// The service the action dispatches to.
        actual: String,
        /// The service the toolkit declares.
        declared: String,
    },
    /// A writing tool that declares no verification path would be publishable but unprovable.
    #[error(
        "toolkit {toolkit:?} tool {tool:?} is a {effect} tool with no verification path; declare verification, verify.verdict + verify.checks, or verify.read_back"
    )]
    UnverifiedWrite {
        /// The toolkit.
        toolkit: String,
        /// The tool key inside the toolkit.
        tool: String,
        /// The registry effect that made verification mandatory.
        effect: String,
    },
    /// A write's `read_back` points at a tool that is absent or does not read.
    #[error(
        "toolkit {toolkit:?} tool {tool:?} names read_back {read_back:?}, which is not a read tool in this toolkit"
    )]
    InvalidReadBack {
        /// The toolkit.
        toolkit: String,
        /// The tool key inside the toolkit.
        tool: String,
        /// The read-back tool key the manifest named.
        read_back: String,
    },
    /// The idempotency rule is not one of the three the column accepts.
    #[error("{tool:?} has idempotency {value:?}, not required|optional|forbidden")]
    InvalidIdempotency {
        /// The tool.
        tool: String,
        /// The rejected value.
        value: String,
    },
    /// The verification names a tool the manifest does not declare.
    #[error("{tool:?} verifies through unknown tool {verification:?}")]
    UnknownVerification {
        /// The tool that declares the verification.
        tool: String,
        /// The missing sibling.
        verification: String,
    },
    /// The verification tool is not a read.
    #[error("{tool:?} verifies through {verification:?}, which is not a read")]
    VerificationNotRead {
        /// The tool that declares the verification.
        tool: String,
        /// The sibling that must read.
        verification: String,
    },
}

/// The embedded manifests, validated once.
///
/// Validation is deliberately fatal at first use: an unreadable or inconsistent manifest is a
/// deployment defect, and publishing a partial catalogue would be worse than refusing to publish.
pub fn manifests() -> Result<&'static [ToolkitManifest], ManifestError> {
    static PARSED: OnceLock<Result<Vec<ToolkitManifest>, ManifestError>> = OnceLock::new();
    PARSED
        .get_or_init(parse_and_validate)
        .as_deref()
        .map_err(Clone::clone)
}

fn parse_and_validate() -> Result<Vec<ToolkitManifest>, ManifestError> {
    let mut out = Vec::with_capacity(RAW.len());
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for (file, raw) in RAW {
        let manifest: ToolkitManifest =
            serde_json::from_str(raw).map_err(|error| ManifestError::Unreadable {
                file: (*file).to_owned(),
                detail: error.to_string(),
            })?;
        validate(&manifest)?;
        if !keys.insert(manifest.key.clone()) {
            return Err(ManifestError::Duplicate { key: manifest.key });
        }
        out.push(manifest);
    }
    Ok(out)
}

/// Validate one manifest against the registry and itself.
fn validate(manifest: &ToolkitManifest) -> Result<(), ManifestError> {
    if !key_shape(&manifest.key) {
        return Err(ManifestError::KeyShape {
            key: manifest.key.clone(),
        });
    }
    if manifest.tools.is_empty() {
        return Err(ManifestError::Empty {
            key: manifest.key.clone(),
        });
    }
    let mut tools: BTreeSet<&str> = BTreeSet::new();
    // Each tool's effect is needed twice — to decide whether it needs a verification path, and
    // to resolve another tool's `read_back` — so it is collected while the actions are checked.
    let mut effects: Vec<(&ToolManifest, Effect)> = Vec::with_capacity(manifest.tools.len());
    for tool in &manifest.tools {
        if !tools.insert(tool.key.as_str()) {
            return Err(ManifestError::DuplicateTool {
                key: manifest.key.clone(),
                tool: tool.key.clone(),
            });
        }
        if !key_shape(&tool.key) {
            return Err(ManifestError::KeyShape {
                key: tool.key.clone(),
            });
        }
        let spec = crate::registry::registry()
            .get(tool.action.as_str())
            .ok_or_else(|| ManifestError::UnknownAction {
                action: tool.action.clone(),
            })?;
        let HandlerRef::External { service, .. } = spec.handler else {
            return Err(ManifestError::NotExternal {
                action: tool.action.clone(),
            });
        };
        if service != manifest.service {
            return Err(ManifestError::ServiceMismatch {
                action: tool.action.clone(),
                actual: service.to_owned(),
                declared: manifest.service.clone(),
            });
        }
        effects.push((tool, spec.effect));
        if let Some(idempotency) = tool.idempotency.as_deref()
            && !matches!(idempotency, "required" | "optional" | "forbidden")
        {
            return Err(ManifestError::InvalidIdempotency {
                tool: tool.key.clone(),
                value: idempotency.to_owned(),
            });
        }
    }
    // A write with no verification path cannot be published (§11.3). Two shapes are accepted:
    // the sibling read-tool key (`verification`) and the declared block (`verify`), whose
    // `read_back` names another tool in this same manifest. They are resolved only after every
    // action above is known good.
    for (tool, effect) in &effects {
        if !matches!(*effect, Effect::Write | Effect::Destructive) {
            continue;
        }
        if let Some(verification) = tool.verification.as_deref() {
            let sibling = manifest
                .tools
                .iter()
                .find(|candidate| candidate.key == verification)
                .ok_or_else(|| ManifestError::UnknownVerification {
                    tool: tool.key.clone(),
                    verification: verification.to_owned(),
                })?;
            let sibling_spec = crate::registry::registry()
                .get(sibling.action.as_str())
                .ok_or_else(|| ManifestError::UnknownAction {
                    action: sibling.action.clone(),
                })?;
            if sibling_spec.effect != crate::spec::Effect::Read {
                return Err(ManifestError::VerificationNotRead {
                    tool: tool.key.clone(),
                    verification: verification.to_owned(),
                });
            }
        }
        if let Some(verify) = &tool.verify {
            if let Some(read_back) = &verify.read_back {
                let resolves = effects.iter().any(|(candidate, candidate_effect)| {
                    candidate.key == *read_back && *candidate_effect == Effect::Read
                });
                if !resolves {
                    return Err(ManifestError::InvalidReadBack {
                        toolkit: manifest.key.clone(),
                        tool: tool.key.clone(),
                        read_back: read_back.clone(),
                    });
                }
            } else if verify.verdict.is_none() || verify.checks.is_none() {
                // Synchronous writes need both halves: a verdict says pass/fail, `checks[]` says
                // what was read back. Either one alone is not a verification path.
                return Err(ManifestError::UnverifiedWrite {
                    toolkit: manifest.key.clone(),
                    tool: tool.key.clone(),
                    effect: effect.as_str().to_owned(),
                });
            }
        } else if tool.verification.is_none() {
            return Err(ManifestError::UnverifiedWrite {
                toolkit: manifest.key.clone(),
                tool: tool.key.clone(),
                effect: effect.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

/// `^[a-z][a-z0-9_]*$` — the same shape `toolkit.key` and `tool.key` carry in the migration.
fn key_shape(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The content hash of one manifest, as published.
///
/// Covers everything a caller could observe changing: the identity fields, the destination, the
/// status, and per tool the action, its redaction rule, its verification declaration, its effect
/// class, its timeout and a digest of the input schema copied from the registry. Two publishes of
/// the same content therefore no-op; any change — including a schema change in the action itself
/// — inserts a new version.
pub fn content_hash(manifest: &ToolkitManifest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"conveyor.toolkit.v2\0");
    for part in [
        manifest.key.as_str(),
        manifest.service.as_str(),
        manifest.title.as_str(),
        manifest.owner.as_str(),
        manifest.auth_adapter.as_str(),
        manifest.licence_app.as_deref().unwrap_or(""),
        manifest.status.as_str(),
        manifest.health_path.as_str(),
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    for tool in &manifest.tools {
        hasher.update(tool.key.as_bytes());
        hasher.update([0u8]);
        hasher.update(tool.action.as_bytes());
        hasher.update([0u8]);
        // Only a declared rule is content: a tool that never carried one hashes exactly as it did
        // before this field existed, so the first publish after the upgrade bumps only the tools
        // whose declarations changed.
        if !tool.redact.is_empty() {
            hasher.update(serde_json::to_vec(&tool.redact).unwrap_or_default());
            hasher.update([0u8]);
        }
        // Only a declared path is content: a tool that never carried one hashes exactly as it did
        // before this field existed, so the first publish after the upgrade bumps only the
        // toolkits whose tools gained a declaration.
        if let Some(verify) = &tool.verify {
            hasher.update(serde_json::to_vec(verify).unwrap_or_default());
            hasher.update([0u8]);
        }
        if let Some(spec) = crate::registry::registry().get(tool.action.as_str()) {
            hasher.update(spec.effect.as_str().as_bytes());
            hasher.update([0u8]);
            hasher.update(spec.timeout_ms.to_string().as_bytes());
            hasher.update([0u8]);
            let input = serde_json::to_vec(&(spec.input)()).unwrap_or_default();
            hasher.update(Sha256::digest(&input));
            hasher.update([0u8]);
            // The output schema is copied from the action at publish time; hashing it here makes
            // a registry schema change a new version, exactly like the input schema.
            let output = serde_json::to_vec(&(spec.output)()).unwrap_or_default();
            hasher.update(Sha256::digest(&output));
            hasher.update([0u8]);
        }
        hasher.update(tool.idempotency.as_deref().unwrap_or("").as_bytes());
        hasher.update([0u8]);
        let redact = serde_json::to_vec(&tool.redact).unwrap_or_default();
        hasher.update(Sha256::digest(&redact));
        hasher.update([0u8]);
        hasher.update(tool.verification.as_deref().unwrap_or("").as_bytes());
        hasher.update([0u8]);
    }
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_manifest_is_valid() {
        let manifests = manifests().expect("the embedded manifests validate");
        assert_eq!(manifests.len(), RAW.len());
        for manifest in manifests {
            assert!(!manifest.tools.is_empty(), "{} has no tools", manifest.key);
            assert!(
                manifest.status == "published" || manifest.status == "draft",
                "{} has status {:?}",
                manifest.key,
                manifest.status
            );
        }
    }

    #[test]
    fn a_key_outside_the_shape_is_refused() {
        assert!(key_shape("coding"));
        assert!(key_shape("image_gen"));
        assert!(!key_shape("Coding"));
        assert!(!key_shape("coding.x"));
        assert!(!key_shape(""));
    }

    /// A minimal toolkit whose one tool is the real external write `coding_jobs.create`, so the
    /// registry's effect — not the test — decides whether verification is mandatory.
    fn coding_manifest(verify: Option<VerifyManifest>) -> ToolkitManifest {
        ToolkitManifest {
            key: "coding".to_owned(),
            service: "coding".to_owned(),
            title: "Coding fleet".to_owned(),
            owner: "platform".to_owned(),
            auth_adapter: "none".to_owned(),
            licence_app: None,
            status: "published".to_owned(),
            health_path: "/health".to_owned(),
            tools: vec![ToolManifest {
                key: "jobs_create".to_owned(),
                action: "coding_jobs.create".to_owned(),
                idempotency: None,
                redact: Vec::new(),
                verification: None,
                verify,
            }],
        }
    }

    #[test]
    fn an_unverified_write_tool_is_refused() {
        assert_eq!(
            validate(&coding_manifest(None)),
            Err(ManifestError::UnverifiedWrite {
                toolkit: "coding".to_owned(),
                tool: "jobs_create".to_owned(),
                effect: "write".to_owned(),
            })
        );
    }

    #[test]
    fn a_read_back_naming_a_missing_tool_is_refused() {
        let verify = VerifyManifest {
            verdict: None,
            checks: None,
            read_back: Some("jobs_absent".to_owned()),
        };
        assert_eq!(
            validate(&coding_manifest(Some(verify))),
            Err(ManifestError::InvalidReadBack {
                toolkit: "coding".to_owned(),
                tool: "jobs_create".to_owned(),
                read_back: "jobs_absent".to_owned(),
            })
        );
    }

    #[test]
    fn a_synchronous_verdict_and_checks_declaration_is_accepted() {
        let verify = VerifyManifest {
            verdict: Some("verdict".to_owned()),
            checks: Some("checks".to_owned()),
            read_back: None,
        };
        assert_eq!(validate(&coding_manifest(Some(verify))), Ok(()));
    }

    /// The verification declaration is part of what "the same manifest" means: a verify-only
    /// edit must insert the next version rather than answer `unchanged` (§11.3).
    #[test]
    fn a_verify_only_change_moves_the_content_hash() {
        let unverified = ToolkitManifest {
            tools: vec![
                ToolManifest {
                    key: "jobs_create".to_owned(),
                    action: "coding_jobs.create".to_owned(),
                    idempotency: None,
                    redact: Vec::new(),
                    verification: None,
                    verify: None,
                },
                ToolManifest {
                    key: "jobs_get".to_owned(),
                    action: "coding_jobs.get".to_owned(),
                    idempotency: None,
                    redact: Vec::new(),
                    verification: None,
                    verify: None,
                },
            ],
            ..coding_manifest(None)
        };
        let mut verified = unverified.clone();
        verified.tools[0].verify = Some(VerifyManifest {
            verdict: None,
            checks: None,
            read_back: Some("jobs_get".to_owned()),
        });
        assert_ne!(content_hash(&unverified), content_hash(&verified));
    }

    /// Redaction is part of the tool declaration; changing it is a new manifest hash.
    #[test]
    fn a_redact_only_change_moves_the_content_hash() {
        let mut redacted = coding_manifest(None);
        redacted.tools[0].redact = vec!["token".to_owned()];
        assert_ne!(
            content_hash(&redacted),
            content_hash(&coding_manifest(None))
        );
    }

    #[test]
    fn a_changed_contract_is_a_new_version() {
        let base = manifests().expect("the embedded manifests validate")[0].clone();
        let original = content_hash(&base);

        let mut changed = base.clone();
        changed.tools[0].idempotency = Some("optional".to_owned());
        assert_ne!(content_hash(&changed), original, "idempotency must hash");

        let mut changed = base.clone();
        changed.tools[0].redact.push("secret".to_owned());
        assert_ne!(content_hash(&changed), original, "redact must hash");

        let mut changed = base.clone();
        changed.tools[0].verification = None;
        assert_ne!(content_hash(&changed), original, "verification must hash");
    }
}
