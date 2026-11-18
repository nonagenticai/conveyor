//! `analyse.providers` — the seven generation providers as pure request/response mappings
//! (ADR 0007 §8).
//!
//! The source called these providers directly from Python: `submit()` posted a request and
//! returned a task id, `check_status()` read a poll response. This crate has no transport — the
//! manifest holds none by design — so the same contracts are expressed as data: [`build`] turns a
//! [`Submission`] into the [`ProviderRequest`] the caller's transport sends, and
//! [`parse_submission`] / [`parse_status`] read the provider's JSON back into [`SubmissionAck`] /
//! [`ProviderStatus`]. `build` is a pure function of its input and never panics; every parse
//! degrades to the source's own default instead of failing on an unexpected body.
//!
//! | provider | submit | ack field | status fields |
//! |---|---|---|---|
//! | Kling | `POST /videos/text2video` | `data.task_id` | `data.task_status`, `task_result.videos[0].url` |
//! | Runway | `POST /image_to_video` | `id` | `status`, `output[0]`, `failure` |
//! | Veo | `POST /models/{model}:predictLongRunning` | `name` | `done`, `response.generatedSamples[0].video.uri`, `error.message` |
//! | WAN | `LOCAL local://wan/ti2v-5b` | `task_id` / `id` | `status`, `video_url` / `path` |
//! | Seedance | `POST /generations` | `task_id` / `id` | `status`, `video_url`, `error` |
//! | Sora | `POST /video/generations` | `id`, else `direct:data[0].url` | `status`, `data[0].url`, `error.message` |
//! | OpenAI images | `POST /images/generations` | `data[0].url` | `data[0].url` / `data[0].b64_json` |
//!
//! # Idempotency
//!
//! Every request carries `Idempotency-Key: {job_id}:{attempt}` ([`IdempotencyKey::token`]). A
//! retry of the same `(job_id, attempt)` reproduces the token byte for byte, so a provider gateway
//! can dedupe it; a second attempt gets a different token and is allowed to spend again. The
//! source's `base.py` records that none of these seven APIs accepted a client idempotency field of
//! its own — its submit timeout is deliberately non-retriable for exactly that reason — so the
//! header is the only carrier and there is no per-provider body field to replicate.
//!
//! # Credentials
//!
//! No mapping holds, needs or emits a credential: the source's `Authorization: Bearer …` and
//! `x-goog-api-key` headers are deliberately absent, and the caller's transport attaches the
//! tenant's key at send time. Building a request requires nothing but the submission.
//! [`ProviderError`] carries only the provider and a static field name for the same reason.
//!
//! # What the source did not determine
//!
//! - **WAN** runs a local `Wan2.2-TI2V-5B` pipeline; it has no HTTP endpoint. It is mapped as
//!   `LOCAL` / `local://wan/ti2v-5b` so it cannot be mistaken for a URL to POST, and its ack
//!   fields (`task_id`, `id`) are the local runner's own, because the source minted the id
//!   in-process.
//! - **OpenAI images** is synchronous: `POST /images/generations` answers with the image, not a
//!   job. The only addressable handle is `data[0].url`; a `b64_json`-only answer has none and is
//!   reported as [`ProviderError::SyncResultNotAddressable`]. Its status parse mirrors the
//!   source's preference for inline bytes: a `b64_json` item is `Succeeded` with no URL.
//! - **Progress** is `None` everywhere: no response the source read carried a percentage.
//! - Width and height are not part of [`Submission`], so the source's base default 1920×1080 is
//!   baked into Runway's `ratio`, Sora's `size` and Seedance's `width`/`height`; the OpenAI body
//!   carries the source's `DalleImageSettings` defaults (`1792x1024`, `high`, `auto`, `png`).
//! - **Status poll paths** are not part of this surface: `build` describes the submission only.
//! - A successful status response with an empty-string locator publishes `None`, matching the
//!   source's `publishable_video_url` rule that an un-fetchable handle is not a URL.

use serde_json::{Map, Value, json};
use thiserror::Error;

/// The `(job_id, attempt)` pair an at-least-once caller replays across a retry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyKey {
    pub job_id: String,
    pub attempt: u32,
}

impl IdempotencyKey {
    pub fn new(job_id: impl Into<String>, attempt: u32) -> Self {
        Self {
            job_id: job_id.into(),
            attempt,
        }
    }

    /// `"{job_id}:{attempt}"` — the same pair always yields the same bytes.
    pub fn token(&self) -> String {
        let Self { job_id, attempt } = self;
        format!("{job_id}:{attempt}")
    }
}

/// The seven providers ADR 0007 §8 ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provider {
    Kling,
    Runway,
    Veo,
    Wan,
    Seedance,
    Sora,
    OpenAi,
}

impl Provider {
    /// The provider's name as the source spelled it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Kling => "kling",
            Self::Runway => "runway",
            Self::Veo => "veo",
            Self::Wan => "wan",
            Self::Seedance => "seedance",
            Self::Sora => "sora",
            Self::OpenAi => "openai_images",
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

/// What the caller asks one provider to generate.
#[derive(Clone, Debug)]
pub struct Submission {
    pub provider: Provider,
    pub prompt: String,
    pub model: String,
    pub duration_s: u32,
    pub negative_prompt: Option<String>,
    pub image_url: Option<String>,
    pub idempotency: IdempotencyKey,
}

/// A request the caller's transport executes. It holds no credential: the transport attaches the
/// provider key. `method` is the source's HTTP verb, except WAN's `LOCAL` (no endpoint exists).
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderRequest {
    pub method: &'static str,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

/// The handle a provider returns once it has accepted (and billed) a task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionAck {
    pub provider_job_id: String,
}

/// One poll result, in the source's own four-state vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

/// A parsed poll response. `progress` is always `None`: the source read no percentage field.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderStatus {
    pub state: ProviderState,
    pub video_url: Option<String>,
    pub error: Option<String>,
    pub progress: Option<f64>,
}

/// A response that cannot yield a job handle, or one whose result is inline-only.
///
/// Variants carry the provider and a static field name — never a response string, so a provider
/// that echoes credentials in an error body cannot leak one through this type.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("{provider} response carried no job id (expected `{field}`)")]
    MissingJobId {
        provider: Provider,
        field: &'static str,
    },
    #[error("{provider} returned a synchronous result with no addressable URL")]
    SyncResultNotAddressable { provider: Provider },
}

/// Build the provider's submit request. Pure: no clock, no network, no credential, no panic.
pub fn build(submission: &Submission) -> ProviderRequest {
    let (method, path, body) = match submission.provider {
        Provider::Kling => (
            "POST",
            "/videos/text2video".to_owned(),
            kling_body(submission),
        ),
        Provider::Runway => (
            "POST",
            "/image_to_video".to_owned(),
            runway_body(submission),
        ),
        Provider::Veo => {
            let model = &submission.model;
            (
                "POST",
                format!("/models/{model}:predictLongRunning"),
                veo_body(submission),
            )
        }
        Provider::Wan => (
            "LOCAL",
            "local://wan/ti2v-5b".to_owned(),
            wan_body(submission),
        ),
        Provider::Seedance => ("POST", "/generations".to_owned(), seedance_body(submission)),
        Provider::Sora => (
            "POST",
            "/video/generations".to_owned(),
            sora_body(submission),
        ),
        Provider::OpenAi => (
            "POST",
            "/images/generations".to_owned(),
            openai_body(submission),
        ),
    };

    let mut headers = vec![
        ("Content-Type".to_owned(), "application/json".to_owned()),
        ("Idempotency-Key".to_owned(), submission.idempotency.token()),
    ];
    if submission.provider == Provider::Runway {
        // The source pinned this protocol version; it is not a credential.
        headers.push(("X-Runway-Version".to_owned(), "2024-11-06".to_owned()));
    }

    ProviderRequest {
        method,
        path,
        headers,
        body,
    }
}

/// Read the provider's submit response. An empty string counts as absent: the source's
/// `get(key, "")` would otherwise hand back a handle nothing can poll.
pub fn parse_submission(provider: Provider, body: &Value) -> Result<SubmissionAck, ProviderError> {
    match provider {
        Provider::Kling => field_ack(
            provider,
            non_empty_str(body.get("data").and_then(|data| data.get("task_id"))),
            "data.task_id",
        ),
        Provider::Runway => field_ack(provider, non_empty_str(body.get("id")), "id"),
        Provider::Veo => field_ack(provider, non_empty_str(body.get("name")), "name"),
        Provider::Wan => field_ack(
            provider,
            non_empty_str(body.get("task_id")).or_else(|| non_empty_str(body.get("id"))),
            "task_id",
        ),
        Provider::Seedance => field_ack(
            provider,
            non_empty_str(body.get("task_id")).or_else(|| non_empty_str(body.get("id"))),
            "task_id",
        ),
        Provider::Sora => match non_empty_str(body.get("id")) {
            Some(id) => Ok(SubmissionAck {
                provider_job_id: id.to_owned(),
            }),
            None => match first_data_item(body).and_then(|item| non_empty_str(item.get("url"))) {
                Some(url) => Ok(SubmissionAck {
                    provider_job_id: format!("direct:{url}"),
                }),
                None => Err(ProviderError::MissingJobId {
                    provider,
                    field: "id",
                }),
            },
        },
        Provider::OpenAi => {
            let item = first_data_item(body);
            if let Some(url) = item.and_then(|item| non_empty_str(item.get("url"))) {
                Ok(SubmissionAck {
                    provider_job_id: url.to_owned(),
                })
            } else if item
                .and_then(|item| non_empty_str(item.get("b64_json")))
                .is_some()
            {
                Err(ProviderError::SyncResultNotAddressable { provider })
            } else {
                Err(ProviderError::MissingJobId {
                    provider,
                    field: "data[0].url",
                })
            }
        }
    }
}

/// Read the provider's poll response. Never fails: an unreadable body falls back to the source's
/// default for that provider — `Pending` for the pollers, `Failed` for WAN and OpenAI images,
/// which have no pending phase.
pub fn parse_status(provider: Provider, body: &Value) -> ProviderStatus {
    match provider {
        Provider::Kling => kling_status(body),
        Provider::Runway => runway_status(body),
        Provider::Veo => veo_status(body),
        Provider::Wan => wan_status(body),
        Provider::Seedance => seedance_status(body),
        Provider::Sora => sora_status(body),
        Provider::OpenAi => openai_status(body),
    }
}

// ---------------------------------------------------------------------------
// Request bodies — field for field from the source's `submit()`.
// ---------------------------------------------------------------------------

fn kling_body(submission: &Submission) -> Value {
    let mut body = Map::new();
    body.insert("model_name".to_owned(), json!(submission.model.as_str()));
    body.insert("prompt".to_owned(), json!(submission.prompt.as_str()));
    // The source sends the duration as a string (`str(int(...))`).
    body.insert(
        "duration".to_owned(),
        json!(submission.duration_s.to_string()),
    );
    if let Some(negative) = non_empty(&submission.negative_prompt) {
        body.insert("negative_prompt".to_owned(), json!(negative));
    }
    Value::Object(body)
}

fn runway_body(submission: &Submission) -> Value {
    let mut body = Map::new();
    body.insert("model".to_owned(), json!(submission.model.as_str()));
    body.insert("promptText".to_owned(), json!(submission.prompt.as_str()));
    body.insert("duration".to_owned(), json!(submission.duration_s));
    // The source's base class always supplied width=1920, height=1080; `ratio` is required, so
    // the default is baked here. Runway ignores `negative_prompt` in the source.
    body.insert("ratio".to_owned(), json!("1920:1080"));
    if let Some(image) = non_empty(&submission.image_url) {
        body.insert("promptImage".to_owned(), json!(image));
    }
    Value::Object(body)
}

fn veo_body(submission: &Submission) -> Value {
    // Veo's source ignores duration, negative prompt and image; only the prompt is mapped.
    json!({
        "instances": [{"prompt": submission.prompt.as_str()}],
        "parameters": {"sampleCount": 1},
    })
}

fn wan_body(submission: &Submission) -> Value {
    // Local pipeline: prompt, optional reference image and the checkpoint the source resolved
    // from `model_name`. Duration is fixed by the pipeline (81 frames at 24 fps).
    let mut body = Map::new();
    body.insert("model".to_owned(), json!(submission.model.as_str()));
    body.insert("prompt".to_owned(), json!(submission.prompt.as_str()));
    if let Some(image) = non_empty(&submission.image_url) {
        body.insert("image_url".to_owned(), json!(image));
    }
    Value::Object(body)
}

fn seedance_body(submission: &Submission) -> Value {
    let mut body = Map::new();
    body.insert("model".to_owned(), json!(submission.model.as_str()));
    body.insert("prompt".to_owned(), json!(submission.prompt.as_str()));
    if let Some(negative) = non_empty(&submission.negative_prompt) {
        body.insert("negative_prompt".to_owned(), json!(negative));
    }
    if let Some(image) = non_empty(&submission.image_url) {
        body.insert("image_url".to_owned(), json!(image));
    }
    // The source sends `duration` only when truthy and the base default dimensions otherwise.
    if submission.duration_s > 0 {
        body.insert("duration".to_owned(), json!(submission.duration_s));
    }
    body.insert("width".to_owned(), json!(1920));
    body.insert("height".to_owned(), json!(1080));
    Value::Object(body)
}

fn sora_body(submission: &Submission) -> Value {
    let mut body = Map::new();
    body.insert("model".to_owned(), json!(submission.model.as_str()));
    body.insert("prompt".to_owned(), json!(submission.prompt.as_str()));
    if submission.duration_s > 0 {
        body.insert("duration".to_owned(), json!(submission.duration_s));
    }
    // The source's base class always supplied width=1920, height=1080 (`size` = "WxH").
    body.insert("size".to_owned(), json!("1920x1080"));
    if let Some(image) = non_empty(&submission.image_url) {
        body.insert("image_url".to_owned(), json!(image));
    }
    Value::Object(body)
}

fn openai_body(submission: &Submission) -> Value {
    // The source folds the negative prompt and the reference URLs into the prompt text
    // (`_compose_prompt`), joined by blank lines.
    let mut prompt = submission.prompt.clone();
    if let Some(negative) = non_empty(&submission.negative_prompt) {
        prompt.push_str("\n\nAvoid: ");
        prompt.push_str(negative);
    }
    if let Some(image) = non_empty(&submission.image_url) {
        prompt.push_str("\n\nReference inspiration URLs: ");
        prompt.push_str(image);
    }

    // The remaining values are the source's `DalleImageSettings` defaults.
    json!({
        "model": submission.model.as_str(),
        "prompt": prompt,
        "size": "1792x1024",
        "quality": "high",
        "background": "auto",
        "output_format": "png",
    })
}

// ---------------------------------------------------------------------------
// Status parsers — the source's status vocabulary, mapped onto the four states.
// ---------------------------------------------------------------------------

fn kling_status(body: &Value) -> ProviderStatus {
    let data = body.get("data");
    let state = match data
        .and_then(|data| data.get("task_status"))
        .and_then(Value::as_str)
    {
        Some("submitted") => ProviderState::Pending,
        Some("processing") => ProviderState::Running,
        Some("succeed") => ProviderState::Succeeded,
        Some("failed") => ProviderState::Failed,
        _ => ProviderState::Pending,
    };
    let video_url = data
        .and_then(|data| data.get("task_result"))
        .and_then(|result| result.get("videos"))
        .and_then(Value::as_array)
        .and_then(|videos| videos.first())
        .and_then(|video| video.get("url"))
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .map(str::to_owned);
    let error = data
        .and_then(|data| data.get("task_status_msg"))
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .map(str::to_owned);
    ProviderStatus {
        state,
        video_url,
        error,
        progress: None,
    }
}

fn runway_status(body: &Value) -> ProviderStatus {
    let state = match body.get("status").and_then(Value::as_str) {
        Some("PENDING") => ProviderState::Pending,
        Some("RUNNING") => ProviderState::Running,
        Some("SUCCEEDED") => ProviderState::Succeeded,
        Some("FAILED") => ProviderState::Failed,
        _ => ProviderState::Pending,
    };
    let video_url = body
        .get("output")
        .and_then(Value::as_array)
        .and_then(|output| output.first())
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .map(str::to_owned);
    let error = body
        .get("failure")
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .map(str::to_owned);
    ProviderStatus {
        state,
        video_url,
        error,
        progress: None,
    }
}

fn veo_status(body: &Value) -> ProviderStatus {
    let done = body.get("done").and_then(Value::as_bool).unwrap_or(false);
    if !done {
        return ProviderStatus {
            state: ProviderState::Running,
            video_url: None,
            error: None,
            progress: None,
        };
    }
    let error = body
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .map(str::to_owned);
    let video_url = body
        .get("response")
        .and_then(|response| response.get("generatedSamples"))
        .and_then(Value::as_array)
        .and_then(|samples| samples.first())
        .and_then(|sample| sample.get("video"))
        .and_then(|video| video.get("uri"))
        .and_then(Value::as_str)
        .filter(|uri| !uri.is_empty())
        .map(str::to_owned);
    if error.is_some() || video_url.is_none() {
        // A finished operation with an error or no video is a failure, never a billed success.
        ProviderStatus {
            state: ProviderState::Failed,
            video_url: None,
            error: error.or_else(|| Some("Veo operation finished with no video".to_owned())),
            progress: None,
        }
    } else {
        ProviderStatus {
            state: ProviderState::Succeeded,
            video_url,
            error: None,
            progress: None,
        }
    }
}

fn wan_status(body: &Value) -> ProviderStatus {
    let video_url = body
        .get("video_url")
        .or_else(|| body.get("path"))
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(str::to_owned);
    let state = match body.get("status").and_then(Value::as_str) {
        Some("pending") => ProviderState::Pending,
        Some("processing" | "running") => ProviderState::Running,
        Some("completed" | "succeeded") => ProviderState::Succeeded,
        Some("failed" | "error") => ProviderState::Failed,
        _ if video_url.is_some() => ProviderState::Succeeded,
        _ => ProviderState::Failed,
    };
    let error = if state == ProviderState::Failed {
        Some(
            body.get("error")
                .and_then(Value::as_str)
                .filter(|message| !message.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| "output file not found".to_owned()),
        )
    } else {
        None
    };
    ProviderStatus {
        state,
        video_url,
        error,
        progress: None,
    }
}

fn seedance_status(body: &Value) -> ProviderStatus {
    let state = match body.get("status").and_then(Value::as_str) {
        Some("pending") => ProviderState::Pending,
        Some("processing") => ProviderState::Running,
        Some("completed") => ProviderState::Succeeded,
        Some("failed" | "error") => ProviderState::Failed,
        _ => ProviderState::Pending,
    };
    let video_url = if state == ProviderState::Succeeded {
        body.get("video_url")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .map(str::to_owned)
    } else {
        None
    };
    let error = if state == ProviderState::Failed {
        Some(
            body.get("error")
                .and_then(Value::as_str)
                .filter(|message| !message.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| "Generation failed".to_owned()),
        )
    } else {
        None
    };
    ProviderStatus {
        state,
        video_url,
        error,
        progress: None,
    }
}

fn sora_status(body: &Value) -> ProviderStatus {
    let state = match body.get("status").and_then(Value::as_str) {
        Some("pending") => ProviderState::Pending,
        Some("processing") => ProviderState::Running,
        Some("completed") => ProviderState::Succeeded,
        Some("failed") => ProviderState::Failed,
        _ => ProviderState::Pending,
    };
    let video_url = if state == ProviderState::Succeeded {
        first_data_item(body)
            .and_then(|item| item.get("url"))
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .map(str::to_owned)
    } else {
        None
    };
    let error = if state == ProviderState::Failed {
        Some(
            body.get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .filter(|message| !message.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| "Generation failed".to_owned()),
        )
    } else {
        None
    };
    ProviderStatus {
        state,
        video_url,
        error,
        progress: None,
    }
}

fn openai_status(body: &Value) -> ProviderStatus {
    let item = first_data_item(body);
    let inline = item
        .and_then(|item| non_empty_str(item.get("b64_json")))
        .is_some();
    let url = item
        .and_then(|item| non_empty_str(item.get("url")))
        .map(str::to_owned);
    if inline {
        // The source prefers inline bytes; this crate cannot carry them, so the result is a
        // success with no addressable locator rather than an invented URL.
        ProviderStatus {
            state: ProviderState::Succeeded,
            video_url: None,
            error: None,
            progress: None,
        }
    } else if let Some(url) = url {
        ProviderStatus {
            state: ProviderState::Succeeded,
            video_url: Some(url),
            error: None,
            progress: None,
        }
    } else {
        ProviderStatus {
            state: ProviderState::Failed,
            video_url: None,
            error: Some("OpenAI returned no image data".to_owned()),
            progress: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers.
// ---------------------------------------------------------------------------

fn field_ack(
    provider: Provider,
    value: Option<&str>,
    field: &'static str,
) -> Result<SubmissionAck, ProviderError> {
    match value {
        Some(provider_job_id) => Ok(SubmissionAck {
            provider_job_id: provider_job_id.to_owned(),
        }),
        None => Err(ProviderError::MissingJobId { provider, field }),
    }
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|text| !text.is_empty())
}

fn non_empty_str(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

fn first_data_item(body: &Value) -> Option<&Value> {
    body.get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn submission(provider: Provider) -> Submission {
        Submission {
            provider,
            prompt: "a lighthouse in a storm".to_owned(),
            model: "test-model".to_owned(),
            duration_s: 5,
            negative_prompt: Some("low contrast".to_owned()),
            image_url: Some("https://example.test/ref.png".to_owned()),
            idempotency: IdempotencyKey::new("job-154", 1),
        }
    }

    fn header<'a>(request: &'a ProviderRequest, name: &str) -> Option<&'a str> {
        request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn idempotency_token_is_job_and_attempt() {
        assert_eq!(IdempotencyKey::new("job-9", 0).token(), "job-9:0");
        assert_eq!(
            IdempotencyKey::new(String::from("job-9"), 4).token(),
            "job-9:4"
        );
    }

    #[test]
    fn kling_maps_text2video_and_reads_task_id_and_result() {
        let request = build(&submission(Provider::Kling));
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/videos/text2video");
        assert_eq!(
            request.body.get("model_name").and_then(Value::as_str),
            Some("test-model")
        );
        assert_eq!(
            request.body.get("duration").and_then(Value::as_str),
            Some("5")
        );
        assert_eq!(
            request.body.get("negative_prompt").and_then(Value::as_str),
            Some("low contrast")
        );
        assert!(request.body.get("image_url").is_none());

        let ack = parse_submission(Provider::Kling, &json!({"data": {"task_id": "kling-task"}}))
            .expect("task id");
        assert_eq!(ack.provider_job_id, "kling-task");

        let status = parse_status(
            Provider::Kling,
            &json!({
                "data": {
                    "task_status": "succeed",
                    "task_result": {"videos": [{"url": "https://cdn.test/kling.mp4"}]},
                },
            }),
        );
        assert_eq!(status.state, ProviderState::Succeeded);
        assert_eq!(
            status.video_url.as_deref(),
            Some("https://cdn.test/kling.mp4")
        );
        assert_eq!(status.progress, None);
    }

    #[test]
    fn runway_maps_image_to_video_and_reads_output() {
        let request = build(&submission(Provider::Runway));
        assert_eq!(request.path, "/image_to_video");
        assert_eq!(
            request.body.get("promptText").and_then(Value::as_str),
            Some("a lighthouse in a storm")
        );
        assert_eq!(
            request.body.get("ratio").and_then(Value::as_str),
            Some("1920:1080")
        );
        assert_eq!(
            request.body.get("promptImage").and_then(Value::as_str),
            Some("https://example.test/ref.png")
        );
        assert_eq!(header(&request, "X-Runway-Version"), Some("2024-11-06"));
        assert!(request.body.get("negative_prompt").is_none());

        let ack =
            parse_submission(Provider::Runway, &json!({"id": "runway-task"})).expect("task id");
        assert_eq!(ack.provider_job_id, "runway-task");

        let status = parse_status(
            Provider::Runway,
            &json!({"status": "SUCCEEDED", "output": ["https://cdn.test/runway.mp4"]}),
        );
        assert_eq!(status.state, ProviderState::Succeeded);
        assert_eq!(
            status.video_url.as_deref(),
            Some("https://cdn.test/runway.mp4")
        );
    }

    #[test]
    fn veo_maps_predict_long_running_and_reads_the_operation() {
        let request = build(&submission(Provider::Veo));
        assert_eq!(request.path, "/models/test-model:predictLongRunning");
        assert_eq!(
            request
                .body
                .pointer("/instances/0/prompt")
                .and_then(Value::as_str),
            Some("a lighthouse in a storm")
        );
        assert_eq!(
            request
                .body
                .pointer("/parameters/sampleCount")
                .and_then(Value::as_u64),
            Some(1)
        );

        let ack = parse_submission(Provider::Veo, &json!({"name": "operations/veo-1"}))
            .expect("operation name");
        assert_eq!(ack.provider_job_id, "operations/veo-1");

        let running = parse_status(Provider::Veo, &json!({"done": false}));
        assert_eq!(running.state, ProviderState::Running);

        let finished = parse_status(
            Provider::Veo,
            &json!({
                "done": true,
                "response": {"generatedSamples": [{"video": {"uri": "https://cdn.test/veo.mp4"}}]},
            }),
        );
        assert_eq!(finished.state, ProviderState::Succeeded);
        assert_eq!(
            finished.video_url.as_deref(),
            Some("https://cdn.test/veo.mp4")
        );

        let failed = parse_status(
            Provider::Veo,
            &json!({"done": true, "error": {"message": "quota exhausted"}}),
        );
        assert_eq!(failed.state, ProviderState::Failed);
        assert_eq!(failed.error.as_deref(), Some("quota exhausted"));
    }

    #[test]
    fn wan_maps_a_local_request_and_reads_the_output_path() {
        let request = build(&submission(Provider::Wan));
        assert_eq!(request.method, "LOCAL");
        assert_eq!(request.path, "local://wan/ti2v-5b");
        assert_eq!(
            request.body.get("prompt").and_then(Value::as_str),
            Some("a lighthouse in a storm")
        );
        assert_eq!(
            request.body.get("image_url").and_then(Value::as_str),
            Some("https://example.test/ref.png")
        );
        assert!(request.body.get("duration").is_none());

        let ack =
            parse_submission(Provider::Wan, &json!({"task_id": "wan-task"})).expect("task id");
        assert_eq!(ack.provider_job_id, "wan-task");

        let status = parse_status(
            Provider::Wan,
            &json!({"status": "completed", "video_url": "/tmp/wan/out.mp4"}),
        );
        assert_eq!(status.state, ProviderState::Succeeded);
        assert_eq!(status.video_url.as_deref(), Some("/tmp/wan/out.mp4"));

        let missing = parse_status(Provider::Wan, &json!({}));
        assert_eq!(missing.state, ProviderState::Failed);
        assert_eq!(missing.error.as_deref(), Some("output file not found"));
    }

    #[test]
    fn seedance_maps_generations_and_reads_status() {
        let request = build(&submission(Provider::Seedance));
        assert_eq!(request.path, "/generations");
        assert_eq!(
            request.body.get("negative_prompt").and_then(Value::as_str),
            Some("low contrast")
        );
        assert_eq!(
            request.body.get("image_url").and_then(Value::as_str),
            Some("https://example.test/ref.png")
        );
        assert_eq!(
            request.body.get("width").and_then(Value::as_u64),
            Some(1920)
        );
        assert_eq!(
            request.body.get("duration").and_then(Value::as_u64),
            Some(5)
        );

        let ack =
            parse_submission(Provider::Seedance, &json!({"id": "seedance-task"})).expect("task id");
        assert_eq!(ack.provider_job_id, "seedance-task");

        let running = parse_status(Provider::Seedance, &json!({"status": "processing"}));
        assert_eq!(running.state, ProviderState::Running);

        let done = parse_status(
            Provider::Seedance,
            &json!({"status": "completed", "video_url": "https://cdn.test/seedance.mp4"}),
        );
        assert_eq!(done.state, ProviderState::Succeeded);
        assert_eq!(
            done.video_url.as_deref(),
            Some("https://cdn.test/seedance.mp4")
        );

        let failed = parse_status(
            Provider::Seedance,
            &json!({"status": "error", "error": "content policy"}),
        );
        assert_eq!(failed.state, ProviderState::Failed);
        assert_eq!(failed.error.as_deref(), Some("content policy"));
    }

    #[test]
    fn sora_maps_video_generations_and_handles_a_direct_result() {
        let request = build(&submission(Provider::Sora));
        assert_eq!(request.path, "/video/generations");
        assert_eq!(
            request.body.get("size").and_then(Value::as_str),
            Some("1920x1080")
        );
        assert_eq!(
            request.body.get("image_url").and_then(Value::as_str),
            Some("https://example.test/ref.png")
        );
        assert!(request.body.get("negative_prompt").is_none());

        let ack =
            parse_submission(Provider::Sora, &json!({"id": "sora-task"})).expect("generation id");
        assert_eq!(ack.provider_job_id, "sora-task");

        let direct = parse_submission(
            Provider::Sora,
            &json!({"data": [{"url": "https://cdn.test/direct.mp4"}]}),
        )
        .expect("direct url");
        assert_eq!(direct.provider_job_id, "direct:https://cdn.test/direct.mp4");

        let status = parse_status(
            Provider::Sora,
            &json!({"status": "completed", "data": [{"url": "https://cdn.test/sora.mp4"}]}),
        );
        assert_eq!(status.state, ProviderState::Succeeded);
        assert_eq!(
            status.video_url.as_deref(),
            Some("https://cdn.test/sora.mp4")
        );
    }

    #[test]
    fn openai_images_maps_the_composed_prompt_and_the_sync_result() {
        let request = build(&submission(Provider::OpenAi));
        assert_eq!(request.path, "/images/generations");
        assert_eq!(
            request.body.get("prompt").and_then(Value::as_str),
            Some(
                "a lighthouse in a storm\n\nAvoid: low contrast\n\n\
                 Reference inspiration URLs: https://example.test/ref.png"
            )
        );
        assert_eq!(
            request.body.get("quality").and_then(Value::as_str),
            Some("high")
        );
        assert_eq!(
            request.body.get("background").and_then(Value::as_str),
            Some("auto")
        );
        assert_eq!(
            request.body.get("size").and_then(Value::as_str),
            Some("1792x1024")
        );

        let ack = parse_submission(
            Provider::OpenAi,
            &json!({"data": [{"url": "https://cdn.test/image.png"}]}),
        )
        .expect("image url");
        assert_eq!(ack.provider_job_id, "https://cdn.test/image.png");

        let inline = parse_status(Provider::OpenAi, &json!({"data": [{"b64_json": "aGk="}]}));
        assert_eq!(inline.state, ProviderState::Succeeded);
        assert_eq!(inline.video_url, None);

        let failed = parse_status(Provider::OpenAi, &json!({"data": []}));
        assert_eq!(failed.state, ProviderState::Failed);
        assert_eq!(
            failed.error.as_deref(),
            Some("OpenAI returned no image data")
        );
    }

    #[test]
    fn retry_repeats_the_token_and_a_new_attempt_gets_a_new_one() {
        let original = submission(Provider::Sora);
        let first = build(&original);
        let retried = build(&original);
        assert_eq!(header(&first, "Idempotency-Key"), Some("job-154:1"));
        assert_eq!(header(&retried, "Idempotency-Key"), Some("job-154:1"));

        let mut next_attempt = original.clone();
        next_attempt.idempotency = IdempotencyKey::new("job-154", 2);
        let second = build(&next_attempt);
        assert_eq!(header(&second, "Idempotency-Key"), Some("job-154:2"));
        assert_ne!(
            header(&first, "Idempotency-Key"),
            header(&second, "Idempotency-Key")
        );
    }

    #[test]
    fn building_a_request_never_needs_or_emits_a_credential() {
        for provider in [
            Provider::Kling,
            Provider::Runway,
            Provider::Veo,
            Provider::Wan,
            Provider::Seedance,
            Provider::Sora,
            Provider::OpenAi,
        ] {
            let request = build(&submission(provider));
            assert_eq!(
                header(&request, "Content-Type"),
                Some("application/json"),
                "{provider}"
            );
            assert!(header(&request, "Idempotency-Key").is_some(), "{provider}");
            for (name, value) in &request.headers {
                assert!(!name.eq_ignore_ascii_case("authorization"), "{provider}");
                assert!(!name.eq_ignore_ascii_case("x-goog-api-key"), "{provider}");
                assert!(!value.contains("Bearer"), "{provider}");
            }
        }
    }

    #[test]
    fn a_response_without_a_handle_is_an_error_not_an_empty_id() {
        assert!(matches!(
            parse_submission(Provider::Kling, &json!({})),
            Err(ProviderError::MissingJobId { .. })
        ));
        assert!(matches!(
            parse_submission(Provider::Seedance, &json!({"task_id": "", "id": ""})),
            Err(ProviderError::MissingJobId {
                field: "task_id",
                ..
            })
        ));
        assert!(matches!(
            parse_submission(Provider::Sora, &json!({"data": []})),
            Err(ProviderError::MissingJobId { .. })
        ));
        assert!(matches!(
            parse_submission(Provider::OpenAi, &json!({"data": [{"b64_json": "aGk="}]})),
            Err(ProviderError::SyncResultNotAddressable { .. })
        ));
    }

    #[test]
    fn unreadable_bodies_fall_back_instead_of_panicking() {
        for provider in [
            Provider::Kling,
            Provider::Runway,
            Provider::Veo,
            Provider::Wan,
            Provider::Seedance,
            Provider::Sora,
            Provider::OpenAi,
        ] {
            let status = parse_status(provider, &json!("not an object"));
            assert!(
                matches!(
                    status.state,
                    ProviderState::Pending | ProviderState::Running | ProviderState::Failed
                ),
                "an unreadable body must never parse as a success: {provider}"
            );
            assert!(
                parse_submission(provider, &json!([])).is_err(),
                "{provider}"
            );
        }
    }
}
