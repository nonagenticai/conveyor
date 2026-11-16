#![forbid(unsafe_code)]
//! `media.generate_image` — image generation on the estate's own model host (#174, S09 §5.1, §11.4).
//!
//! **Declared, not implemented.** The engine's `invoke_external` owns the timeout, the process-local
//! breaker, the idempotency claim, the audit row, the `inference_call` ledger row and — because this
//! spec declares an [`Artefact`] — the document the response carries. A `reqwest::` call in this file
//! would bypass all five and fails `crates/api/tests/externals_declared.rs`.
//!
//! **The service.** The estate's own image host: the Rust `mlx_inference` server in front of the
//! vendored `mlxcel` engine running MOSS-Image (`VisualInference/MOSS-Image-SFT-MLX-compact`). It
//! replaced the Python/mflux FLUX service on 2026-11-15 and keeps the same host, port and DNS the
//! FLUX service had — none of which belongs in this file: the address is configuration.
//! The base URL is configuration only: `CONVEYOR_EXTERNAL_IMAGE_GEN_URL`, no default — unset is a
//! visible `unconfigured` refusal, never a guessable host.
//!
//! **No credential.** The host mounts no licence gate (`AUTH_ENABLED=false`) and is reachable only
//! from inside the cluster, exactly as its tokenless Python predecessor was. `auth: None` sends no
//! bearer; there is no platform token to substitute.
//!
//! **Why the native route and not the legacy dialect.** `/api/image/generate` exists for the Python
//! service's one live caller (w2v's `local_image` provider), which mapped FLUX's 4-step ask onto this
//! engine's own schedule. Conveyor is a new caller: it speaks the engine's `/v1/images/generations`
//! contract directly rather than inheriting a translation aimed at a caller that is going away.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::registry::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Artefact, Confirm, Effect, HandlerRef, Schema,
    Scope, Tenancy,
};

/// The service key: `CONVEYOR_EXTERNAL_IMAGE_GEN_URL`. Underscores, not hyphens: the engine's env
/// lookup only uppercases the key, so `image-gen` would ask for `CONVEYOR_EXTERNAL_IMAGE-GEN_URL`.
pub const SERVICE: &str = "image_gen";

/// The JSON member of the engine's response that carries the rendered PNG, base64.
const ARTEFACT_FIELD: &str = "data.0.b64_json";

/// The sizes this host serves, as the engine's own `WxH` spelling.
///
/// An enum rather than a free string on purpose: the engine's grid is 16 px and its bounds are
/// 256..2048, neither of which JSON Schema can state, and the action must refuse an unusable size
/// **before** spending a render. These three are the sizes measured to fit the serving host's 16 GiB
/// of unified memory (see the gitops ConfigMap for the render timings); a caller that needs another
/// size adds it here, with a measurement, rather than discovering the limit through a 504.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ImageSize {
    /// 512×512 — the legacy parity size, ~103 s at 12 steps.
    #[serde(rename = "512x512")]
    S512,
    /// 768×768.
    #[serde(rename = "768x768")]
    S768,
    /// 1024×1024 — the largest size measured on this host.
    #[serde(rename = "1024x1024")]
    S1024,
}

impl ImageSize {
    /// The `WxH` string the engine's `size` member takes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::S512 => "512x512",
            Self::S768 => "768x768",
            Self::S1024 => "1024x1024",
        }
    }
}

/// The request: a prompt, and the three knobs the engine exposes to a caller.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerateImageIn {
    /// What to draw. The engine's cap is 2000 characters.
    pub prompt: String,
    /// The canvas. Absent means the engine's own default (512×512).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<ImageSize>,
    /// A fixed seed makes a render reproducible; absent means a random one, echoed back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Denoising steps. Absent means the engine's own default (50). A render costs time linearly in
    /// this number, and the host answers `504` past its own 900 s budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
}

/// The request schema.
///
/// Written by hand rather than derived, because `schemars` renders `Option<ImageSize>` as an
/// `anyOf` of a `$ref` and `null`, and `validate_input` enforces `enum` only where it appears
/// inline — a derived schema would let `2048x2048` through to a render that cannot serve it. The
/// size list is duplicated from [`ImageSize`] deliberately; `the_schema_and_the_enum_agree` fails
/// if the two drift apart.
fn generate_image_input() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": false,
        "required": ["prompt"],
        "properties": {
            "prompt": {
                "type": "string",
                "description": "What to draw. The engine caps it at 2000 characters."
            },
            "size": {
                "type": "string",
                "enum": ["512x512", "768x768", "1024x1024"],
                "description": "The canvas. Absent means the engine's own default (512x512). Only \
                                sizes measured to fit this host are accepted, and an unusable one \
                                is refused before any render is spent."
            },
            "seed": {
                "type": "integer",
                "description": "A fixed seed makes a render reproducible; absent means a random one, echoed back."
            },
            "steps": {
                "type": "integer",
                "description": "Denoising steps. Absent means the engine's own default (50). A render \
                                costs time linearly in this number."
            }
        }
    })
}

/// What the caller receives: the engine's document, plus the one member the engine wrote.
///
/// The engine's response is passed through unchanged (`data[]`, `created`, `size`, `output_format`);
/// `document` is added by the engine because the PNG was stored as a tenant document, so a caller
/// never has to hold the base64 to refer to the image afterwards.
fn generate_image_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The image host's document, passed through, plus the stored document.",
        "additionalProperties": true,
        "properties": {
            "document": {
                "type": ["object", "null"],
                "description": "The tenant document the rendered PNG was stored as.",
                "additionalProperties": true
            }
        }
    })
}

/// `media.generate_image`.
fn generate_image() -> ActionSpec {
    ActionSpec {
        name: "media.generate_image",
        version: 1,
        kind: ActionKind::External,
        input: generate_image_input,
        output: generate_image_output,
        // The media family's write scope: this is the same capability surface as `media.render_html`
        // and `media.compile_video`, one step further along the production chain.
        scopes: &[Scope::CapabilitiesWrite],
        // A replay with the same key returns the first render — one image, one charge.
        idempotent: true,
        // A render is a diffusion run plus three checkpoint loads. Measured on the host: ~103 s at
        // 512²/12 steps, ~415 s at 512²/50. The host's own ceiling is 900 s; 600 s covers any
        // render it can answer and fails fast on one it cannot.
        timeout_ms: 600_000,
        // Generation costs the tenant money and deletes nothing (§11.4).
        effect: Effect::Spend,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        // Not task-capable: `projects.execute` is the one action that declares the MCP task
        // transport, and `mcp_tasks_contract` holds that to exactly one. A render is long enough to
        // want it, but widening the transport contract is not this unit's business.
        task_capable: false,
        captures_usage: true,
        artefact: Some(Artefact {
            field: ARTEFACT_FIELD,
            content_type: "image/png",
            title: "Generated image",
        }),
        handler: HandlerRef::External {
            service: SERVICE,
            method: "POST",
            path: "/v1/images/generations",
            auth: None,
            auth_optional: false,
        },
    }
}

inventory::submit! { ActionRegistration::new(generate_image) }

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ActionSpec {
        generate_image()
    }

    #[test]
    fn the_spec_is_a_declared_external_spend_that_stores_its_artefact() {
        let spec = spec();
        assert_eq!(spec.name, "media.generate_image");
        assert_eq!(spec.kind, ActionKind::External);
        assert_eq!(spec.effect, Effect::Spend);
        assert_eq!(spec.scope_names(), vec!["capabilities:write"]);
        assert!(spec.idempotent);
        assert!(spec.captures_usage);
        let artefact = spec
            .artefact
            .expect("the image is stored, not returned only");
        assert_eq!(artefact.content_type, "image/png");
        match spec.handler {
            HandlerRef::External {
                service,
                method,
                path,
                auth,
                ..
            } => {
                assert_eq!(service, "image_gen");
                assert_eq!(method, "POST");
                assert_eq!(path, "/v1/images/generations");
                assert!(auth.is_none(), "the host mounts no credential");
            }
            other => panic!("expected an external handler, got {other:?}"),
        }
    }

    /// P1.2: the size the engine cannot serve is refused by the input schema, so
    /// `validate_input` rejects it before `invoke_external` can reach the network.
    #[test]
    fn an_unserved_size_fails_validation_before_the_call() {
        let schema = spec().input_schema();
        let refused = crate::spec::validate_input(
            &schema,
            &serde_json::json!({"prompt": "a fox", "size": "2048x2048"}),
        );
        assert!(
            !refused.is_empty(),
            "2048x2048 is not a size this host serves and must be refused"
        );
        let accepted = crate::spec::validate_input(
            &schema,
            &serde_json::json!({"prompt": "a fox", "size": "1024x1024"}),
        );
        assert!(accepted.is_empty(), "1024x1024 is served: {accepted:?}");
    }

    /// P1.1: an unknown field is refused, not silently dropped.
    #[test]
    fn an_unknown_field_fails_validation() {
        let schema = spec().input_schema();
        let refused = crate::spec::validate_input(
            &schema,
            &serde_json::json!({"prompt": "a fox", "cfg_scale": 3.0}),
        );
        assert!(!refused.is_empty(), "an unknown field must be refused");
    }

    #[test]
    fn the_size_enum_sends_the_engine_spelling() {
        assert_eq!(ImageSize::S1024.as_str(), "1024x1024");
        assert_eq!(
            serde_json::to_value(ImageSize::S768).expect("serializes"),
            serde_json::json!("768x768")
        );
    }

    /// The wire schema and the Rust enum are two spellings of one list; this is the join.
    #[test]
    fn the_schema_and_the_enum_agree() {
        let schema = spec().input_schema();
        let offered = schema["properties"]["size"]["enum"]
            .as_array()
            .expect("the size member is an enum");
        let declared: Vec<&str> = [ImageSize::S512, ImageSize::S768, ImageSize::S1024]
            .iter()
            .map(|size| size.as_str())
            .collect();
        let offered: Vec<&str> = offered.iter().filter_map(|value| value.as_str()).collect();
        assert_eq!(offered, declared);
    }
}
