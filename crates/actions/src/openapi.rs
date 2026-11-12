#![forbid(unsafe_code)]
//! The OpenAPI fold: a literal path item per action, built from the same `ActionSpec` the
//! manifest and the MCP tool read (S04 §5.4).
//!
//! `utoipa`'s router owns the hand-written routes. The action operations are folded in as
//! JSON path items — the shape S04's binding decision allows — because the operation's
//! members come from `ActionSpec.input`/`output`, not from a second declaration. Nothing
//! here reads anything else: `x-action-spec-hash` is [`ActionSpec::spec_hash`], the same
//! value the manifest carries and `lock.rs` hashes into the served-spec lock.

use serde_json::{Value, json};

use crate::registry::Registry;
use crate::spec::ActionSpec;

/// The tag every action operation carries.
pub const TAG: &str = "actions";

/// The security scheme name the action operations reference.
pub const SECURITY_SCHEME: &str = "bearerAuth";

/// One action's invocation OPERATION (the `post` member of its path item).
pub fn operation_json(spec: &ActionSpec) -> Value {
    json!({
        "tags": [TAG],
        "summary": spec.name,
        "description": format!(
            "Invoke `{}` ({} action, {} effect). The input is validated against the schema \
             below; a successful invocation is 2xx. `200` carries `{{output, request_id, \
             duration_ms}}`, a creation answers `201` with the same envelope and an `ETag`, \
             and a transition with no representation answers `204` with no body.",
            spec.name,
            spec.kind.as_str(),
            spec.effect.as_str()
        ),
        "operationId": spec.operation_id(),
        "requestBody": {
            "required": true,
            "content": {
                "application/json": {
                    "schema": {
                        "type": "object",
                        "required": ["input"],
                        "properties": { "input": spec.input_schema() }
                    }
                }
            }
        },
        "responses": {
            "200": {
                "description": "The action's output",
                "content": {
                    "application/json": { "schema": envelope_schema(spec) }
                }
            },
            "default": {
                "description": "RFC 9457 application/problem+json",
                "content": {
                    "application/problem+json": {
                        "schema": {
                            "type": "object",
                            "required": ["type", "title", "status", "detail"],
                            "properties": {
                                "type": {"type": "string"},
                                "title": {"type": "string"},
                                "status": {"type": "integer"},
                                "detail": {"type": "string"},
                                "instance": {"type": ["string", "null"]},
                                "request_id": {"type": ["string", "null"]},
                                "errors": {"type": "array", "items": {"type": "object"}}
                            }
                        }
                    }
                }
            }
        },
        "security": [{ SECURITY_SCHEME: [] }],
        "x-action-name": spec.name,
        "x-action-version": spec.version,
        "x-action-kind": spec.kind.as_str(),
        "x-action-scopes": spec.scope_names(),
        "x-action-idempotent": spec.idempotent,
        "x-action-timeout-ms": spec.timeout_ms,
        "x-action-effect": spec.effect.as_str(),
        "x-action-confirm": spec.confirm.as_str(),
        "x-action-tenancy": spec.tenancy.as_str(),
        "x-action-mcp-name": spec.mcp_name(),
        "x-action-spec-hash": spec.spec_hash(),
    })
}

/// The 200 envelope's schema.
pub fn envelope_schema(spec: &ActionSpec) -> Value {
    json!({
        "type": "object",
        "required": ["output"],
        "properties": {
            "output": spec.output_schema(),
            "request_id": {"type": ["string", "null"]},
            "duration_ms": {"type": "integer", "minimum": 0}
        }
    })
}

/// `GET /api/actions` — the manifest operation.
pub fn manifest_operation(registry: &Registry) -> Value {
    json!({
        "tags": [TAG],
        "summary": "The action manifest",
        "description": "Every action this caller's role may invoke. ETag/If-None-Match wraps it.",
        "operationId": "actions_manifest",
        "responses": {
            "200": {
                "description": "The manifest",
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "object",
                            "required": ["actions", "generated_at"],
                            "properties": {
                                "actions": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "required": ["name", "version", "kind", "spec_hash",
                                                     "scopes", "idempotent", "timeout_ms"],
                                        "properties": {
                                            "name": {"type": "string"},
                                            "version": {"type": "integer"},
                                            "kind": {"type": "string"},
                                            "spec_hash": {"type": "string"},
                                            "scopes": {"type": "array",
                                                       "items": {"type": "string"}},
                                            "idempotent": {"type": "boolean"},
                                            "timeout_ms": {"type": "integer"}
                                        }
                                    }
                                },
                                "generated_at": {"type": "string", "format": "date-time"}
                            }
                        }
                    }
                }
            },
            "304": {"description": "The caller's ETag still matches"},
            "default": {"description": "RFC 9457 application/problem+json"}
        },
        "security": [{ SECURITY_SCHEME: [] }],
        "x-action-count": registry.len(),
    })
}

/// Fold every action operation into the serialized `base` document.
///
/// The document stays a JSON value: utoipa owns the hand-written routes and produces the
/// base, and the generated operations are inserted as JSON. Re-deserializing the folded
/// document into utoipa's typed tree buys nothing and can reject a valid JSON-Schema
/// construct (`$defs`); the served document and the lock read the same value either way.
pub fn fold(mut document: Value, registry: &Registry) -> Value {
    let Some(object) = document.as_object_mut() else {
        return document;
    };

    let paths = object
        .entry("paths")
        .or_insert_with(|| Value::Object(Default::default()));
    if let Some(paths) = paths.as_object_mut() {
        paths.insert(
            "/api/actions".into(),
            json!({ "get": manifest_operation(registry) }),
        );
        for spec in registry.iter() {
            paths.insert(spec.route_path(), json!({ "post": operation_json(spec) }));
        }
    }

    // The operation's security references this scheme; declare it once.
    let components = object
        .entry("components")
        .or_insert_with(|| Value::Object(Default::default()));
    if let Some(components) = components.as_object_mut() {
        let schemes = components
            .entry("securitySchemes")
            .or_insert_with(|| Value::Object(Default::default()));
        if let Some(schemes) = schemes.as_object_mut() {
            schemes.entry(SECURITY_SCHEME).or_insert_with(|| {
                json!({
                    "type": "http",
                    "scheme": "bearer",
                    "description": "OIDC access token (or `dev:<login>` under CONVEYOR_DEV_AUTH)"
                })
            });
        }
    }

    document
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_action_has_one_operation_and_one_hash() {
        let registry = crate::registry::registry();
        for spec in registry.iter() {
            let item = operation_json(spec);
            assert_eq!(item["x-action-spec-hash"], spec.spec_hash());
            assert_eq!(item["x-action-name"], spec.name);
            assert_eq!(
                item["requestBody"]["content"]["application/json"]["schema"]["properties"]["input"],
                spec.input_schema()
            );
        }
    }
}
