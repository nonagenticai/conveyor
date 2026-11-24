#![forbid(unsafe_code)]
//! `produce.speech` and `media.transcribe` — the two audio capabilities of the model hosts
//! (#173, SPEC_V2 §10.4).
//!
//! **Declared, not implemented.** The engine's `invoke_external` owns the HTTP call, the timeout,
//! the breaker, the idempotency claim, the audit row, the `inference_call` ledger row and — because
//! both specs declare an [`Artefact`] — the document the response carries. A `reqwest::` call in
//! this file would bypass all six and fails `crates/api/tests/externals_declared.rs`.
//!
//! **The services.** The estate's own model hosts, both already Rust and both outside this
//! repository by ADR 0007: the speech host serves Kokoro through `mlx_inference`, the transcription
//! host serves Whisper the same way. Their addresses are configuration only —
//! `CONVEYOR_EXTERNAL_TTS_URL` and `CONVEYOR_EXTERNAL_ASR_URL`, no default. An unset value is a
//! visible `unconfigured` refusal, never a guessable host.
//!
//! **Why the legacy narration route for speech.** `/api/tts/synthesize` answers the JSON envelope
//! when the request asks for JSON and raw `audio/wav` otherwise. It is the route that carries the
//! loudness premaster and the per-sentence pacing the narration pipeline depends on, and it is the
//! one contract both of the Python service's callers already agreed on. The newer
//! `/v1/audio/speech` is a byte stream with no envelope, which the artefact mechanism — a base64
//! member of the response JSON — cannot carry.
//!
//! **Why multipart for transcription.** OpenAI's transcription contract is a file upload, so the
//! handler declares an [`Upload`]: the engine fetches the named document under the caller's tenant,
//! hash-verifies it exactly as a documents byte route does, and posts one `file` part. The store
//! credential never leaves the process and the service never sees a document id.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::registry::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Artefact, ArtefactEncoding, Confirm, Effect,
    HandlerRef, Schema, Scope, Tenancy, Upload,
};

/// The speech service key: `CONVEYOR_EXTERNAL_TTS_URL`.
pub const SPEECH_SERVICE: &str = "tts";

/// The transcription service key: `CONVEYOR_EXTERNAL_ASR_URL`.
pub const TRANSCRIBE_SERVICE: &str = "asr";

/// The envelope member the narration route puts the WAV bytes in, base64.
const SPEECH_ARTEFACT_FIELD: &str = "audio_base64";

/// The member the transcription route puts the text in.
const TRANSCRIPT_FIELD: &str = "text";

/// The duration of a RIFF/WAVE file in seconds, or `None` when the bytes are not a complete WAV.
///
/// The transcription host answers only `{"text": …}`, so the only length the engine can price is
/// the uploaded document's own. The bytes are already in hand for the multipart part, and the parse
/// is pure — no IO and no host extension. A non-WAV upload is `None`, which the report counts as
/// unpriced, never as $0.
pub(crate) fn wav_duration_secs(bytes: &[u8]) -> Option<rust_decimal::Decimal> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    let mut byte_rate: Option<u32> = None;
    let mut data_len: Option<u32> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body = pos + 8;
        let end = body.checked_add(size)?;
        match id {
            b"fmt " if size >= 16 && end <= bytes.len() => {
                byte_rate = Some(u32::from_le_bytes([
                    bytes[body + 8],
                    bytes[body + 9],
                    bytes[body + 10],
                    bytes[body + 11],
                ]));
            }
            b"data" => {
                // A truncated data chunk is not a measurement; the host refuses it anyway.
                if end > bytes.len() {
                    return None;
                }
                data_len = Some(u32::try_from(size).ok()?);
                break;
            }
            _ => {}
        }
        if end > bytes.len() {
            break;
        }
        // RIFF chunks are word-aligned: an odd size carries one pad byte.
        pos = end + (size & 1);
    }
    let byte_rate = byte_rate.filter(|rate| *rate > 0)?;
    let data_len = data_len?;
    Some(rust_decimal::Decimal::from(data_len) / rust_decimal::Decimal::from(byte_rate))
}

/// `produce.speech`: text in, one stored WAV document out.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpeechIn {
    /// What to narrate. The route's own cap is 10,000 characters.
    pub text: String,
    /// A voice pack of the served checkpoint; absent means the service's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    /// Speaking rate; the route accepts 0.5–2.0 and refuses anything else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

/// `media.transcribe`: one stored audio document in, a transcript document out.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranscribeIn {
    /// The audio document to transcribe, by id.
    pub document: uuid::Uuid,
}

/// The speech request schema.
///
/// Hand-written for the same reason `media.generate_image`'s is: the range on `speed` must be in
/// the schema the caller sees, not only in the service's own validation, so an impossible rate is
/// refused before a synthesis is spent.
fn speech_input() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": false,
        "required": ["text"],
        "properties": {
            "text": {
                "type": "string",
                "description": "What to narrate. The route caps it at 10,000 characters."
            },
            "voice": {
                "type": "string",
                "description": "A voice pack of the served checkpoint; absent means the service's default."
            },
            "speed": {
                "type": "number",
                "minimum": 0.5,
                "maximum": 2.0,
                "description": "Speaking rate. Outside 0.5–2.0 the service refuses the request."
            }
        }
    })
}

/// The transcription request schema.
fn transcribe_input() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": false,
        "required": ["document"],
        "properties": {
            "document": {
                "type": "string",
                "format": "uuid",
                "description": "The audio document to transcribe, by id."
            }
        }
    })
}

/// What a caller receives from either action: the service's own body, plus the stored document.
fn audio_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The model host's response, passed through, plus the stored document.",
        "additionalProperties": true,
        "properties": {
            "document": {
                "type": ["object", "null"],
                "description": "The tenant document the bytes or transcript were stored as.",
                "additionalProperties": true
            }
        }
    })
}

/// `produce.speech`.
fn speech() -> ActionSpec {
    ActionSpec {
        name: "produce.speech",
        version: 1,
        kind: ActionKind::External,
        input: speech_input,
        output: audio_output,
        // The production family's write scope: narrating is authoring the deliverable, exactly as
        // a render is.
        scopes: &[Scope::CapabilitiesWrite],
        // A replay with the same key returns the first clip — one synthesis, one charge.
        idempotent: true,
        // Long narration is chunked by the service; a 10,000-character ask is minutes of audio.
        // The host is shared with the image renders and the transcriptions, so the budget absorbs
        // the queue ahead of the ask as well as the ask itself.
        timeout_ms: 1_200_000,
        // Synthesis costs the tenant money and deletes nothing (§11.4).
        effect: Effect::Spend,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        task_capable: false,
        captures_usage: true,
        artefact: Some(Artefact {
            field: SPEECH_ARTEFACT_FIELD,
            encoding: ArtefactEncoding::Base64,
            content_type: "audio/wav",
            title: "Narration",
        }),
        approval_target: None,
        handler: HandlerRef::External {
            service: SPEECH_SERVICE,
            method: "POST",
            path: "/api/tts/synthesize",
            auth: None,
            auth_optional: false,
            upload: None,
        },
    }
}

/// `media.transcribe`.
fn transcribe() -> ActionSpec {
    ActionSpec {
        name: "media.transcribe",
        version: 1,
        kind: ActionKind::External,
        input: transcribe_input,
        output: audio_output,
        scopes: &[Scope::CapabilitiesWrite],
        // The same audio and the same key produce the same transcript; a replay returns the first.
        idempotent: true,
        // Transcription is linear in audio length, and the same host serves the estate's speech and
        // image work from one queue: ten minutes covers a long narration plus the wait behind a
        // render.
        timeout_ms: 600_000,
        // A model call that also creates a document: money, not deletion.
        effect: Effect::Spend,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        task_capable: false,
        captures_usage: true,
        artefact: Some(Artefact {
            field: TRANSCRIPT_FIELD,
            encoding: ArtefactEncoding::Utf8,
            content_type: "text/plain",
            title: "Transcript",
        }),
        approval_target: None,
        handler: HandlerRef::External {
            service: TRANSCRIBE_SERVICE,
            method: "POST",
            path: "/v1/audio/transcriptions",
            auth: None,
            auth_optional: false,
            upload: Some(Upload {
                input: "document",
                field: "file",
            }),
        },
    }
}

inventory::submit! { ActionRegistration::new(speech) }
inventory::submit! { ActionRegistration::new(transcribe) }

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> ActionSpec {
        crate::registry::registry()
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"))
            .clone()
    }

    #[test]
    fn speech_declares_its_contract() {
        let spec = spec("produce.speech");
        assert_eq!(spec.kind, ActionKind::External);
        assert_eq!(spec.effect, Effect::Spend);
        assert!(spec.captures_usage, "a synthesis is a model call");
        assert_eq!(
            spec.timeout_ms, 1_200_000,
            "narration shares the render host's queue (#571)"
        );
        assert_eq!(spec.scope_names(), vec!["capabilities:write"]);
        let artefact = spec
            .artefact
            .expect("the clip is stored, not returned only");
        assert_eq!(artefact.field, "audio_base64");
        assert_eq!(artefact.encoding, ArtefactEncoding::Base64);
        assert_eq!(artefact.content_type, "audio/wav");
        match spec.handler {
            HandlerRef::External {
                service,
                method,
                path,
                upload,
                ..
            } => {
                assert_eq!(service, "tts");
                assert_eq!(method, "POST");
                assert_eq!(path, "/api/tts/synthesize");
                assert!(upload.is_none(), "speech sends JSON, not a file");
            }
            HandlerRef::Internal(_)
            | HandlerRef::Workflow
            | HandlerRef::Broker
            | HandlerRef::ToolDestination => {
                panic!("produce.speech must be a declared external action")
            }
        }
    }

    #[test]
    fn transcription_uploads_a_document_and_stores_the_text() {
        let spec = spec("media.transcribe");
        assert_eq!(spec.kind, ActionKind::External);
        assert_eq!(spec.effect, Effect::Spend);
        assert!(spec.captures_usage, "a transcription is a model call");
        let artefact = spec.artefact.expect("the transcript is stored");
        assert_eq!(artefact.field, "text");
        assert_eq!(artefact.encoding, ArtefactEncoding::Utf8);
        assert_eq!(artefact.content_type, "text/plain");
        match spec.handler {
            HandlerRef::External {
                service,
                path,
                upload,
                ..
            } => {
                assert_eq!(service, "asr");
                assert_eq!(path, "/v1/audio/transcriptions");
                let upload = upload.expect("transcription is a multipart upload");
                assert_eq!(upload.input, "document");
                assert_eq!(upload.field, "file");
            }
            HandlerRef::Internal(_)
            | HandlerRef::Workflow
            | HandlerRef::Broker
            | HandlerRef::ToolDestination => {
                panic!("media.transcribe must be a declared external action")
            }
        }
    }

    #[test]
    fn both_input_schemas_are_closed() {
        for name in ["produce.speech", "media.transcribe"] {
            assert_eq!(
                spec(name).input_schema()["additionalProperties"],
                serde_json::json!(false),
                "{name}: the input schema must refuse unknown fields"
            );
        }
    }

    /// The schema the caller sees and the type the handler receives must not drift: a range the
    /// schema omits is a request that reaches a synthesis and is refused by the service instead.
    #[test]
    fn the_speech_schema_and_the_type_agree() {
        let schema = spec("produce.speech").input_schema();
        assert_eq!(schema["required"], serde_json::json!(["text"]));
        assert_eq!(
            schema["properties"]["speed"]["minimum"],
            serde_json::json!(0.5)
        );
        assert_eq!(
            schema["properties"]["speed"]["maximum"],
            serde_json::json!(2.0)
        );
        let value = serde_json::json!({"text": "hello", "speed": 1.0});
        assert!(serde_json::from_value::<SpeechIn>(value).is_ok());
    }

    #[test]
    fn a_wav_duration_is_read_from_the_header() {
        assert_eq!(
            wav_duration_secs(&wav(2)),
            Some(rust_decimal::Decimal::new(2, 0))
        );
        assert_eq!(
            wav_duration_secs(&wav(0)),
            Some(rust_decimal::Decimal::ZERO),
            "a zero-length data chunk is zero seconds, not an unknown"
        );
        assert_eq!(wav_duration_secs(b"not a wave file"), None);
        assert_eq!(wav_duration_secs(&wav(1)[..20]), None, "truncated");
    }

    /// A header-shaped fixture whose data chunk really carries `seconds` of silence.
    fn wav(seconds: u32) -> Vec<u8> {
        let byte_rate = 48_000u32;
        let data = byte_rate * seconds;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&24_000u32.to_le_bytes());
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&16u16.to_le_bytes()); // bits
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data.to_le_bytes());
        bytes.resize(bytes.len() + data as usize, 0);
        bytes
    }
}
