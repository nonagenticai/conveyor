//! The authenticated SSE client.
//!
//! `EventSource` cannot send an `Authorization` header, so this is `fetch` + a
//! `ReadableStreamDefaultReader` over `GET /api/events?topics=…`. Frames are parsed by hand;
//! `Last-Event-ID` resumes a dropped stream; a `resync` frame tells the screen to refetch its
//! projection rather than trust the gap. Reconnect is exponential, capped at 15 s.

use crate::api::{self, ApiError, Client};
use js_sys::Uint8Array;
use serde_json::Value;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, AbortSignal, Headers, ReadableStreamDefaultReader, Request,
    RequestCredentials, RequestInit, RequestMode, Response, TextDecoder,
};

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Frame {
    pub id: Option<i64>,
    pub event: String,
    pub data: Option<Value>,
}

/// The live subscription. Abort it from `on_cleanup`; an unaborted stream outlives the screen.
pub struct Subscription {
    controller: AbortController,
}

impl Subscription {
    pub fn abort(&self) {
        self.controller.abort();
    }
}

/// Start a subscription. The callback runs on every frame, and once per failed reconnect
/// attempt (`on(Err(..))`); a `resync` event is delivered as `Frame { event: "resync" }`.
pub fn subscribe(
    client: Client,
    topics: String,
    last: Option<i64>,
    mut on: impl FnMut(Result<Frame, ApiError>) + 'static,
) -> Result<Subscription, ApiError> {
    let controller = AbortController::new().map_err(js)?;
    let signal = controller.signal();
    wasm_bindgen_futures::spawn_local(async move {
        let mut last_id = last;
        let mut backoff = 1_000i32;
        loop {
            if signal.aborted() {
                break;
            }
            match run_once(&client, &topics, last_id, &signal, &mut on).await {
                Ok(seen) => {
                    if let Some(seen) = seen {
                        last_id = Some(seen);
                    }
                    backoff = 1_000;
                }
                Err(ApiError::Network(_)) if signal.aborted() => break,
                Err(error) => {
                    on(Err(error));
                }
            }
            if signal.aborted() {
                break;
            }
            sleep(backoff).await;
            backoff = (backoff * 2).min(15_000);
        }
    });
    Ok(Subscription { controller })
}

async fn run_once(
    client: &Client,
    topics: &str,
    last: Option<i64>,
    signal: &AbortSignal,
    on: &mut impl FnMut(Result<Frame, ApiError>),
) -> Result<Option<i64>, ApiError> {
    let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
    let headers = Headers::new().map_err(js)?;
    headers.set("accept", "text/event-stream").map_err(js)?;
    if let Some(token) = &client.token {
        headers
            .set("authorization", &format!("Bearer {token}"))
            .map_err(js)?;
    }
    if let Some(tenant) = &client.tenant {
        headers.set("x-active-tenant", tenant).map_err(js)?;
    }
    if let Some(last) = last {
        headers
            .set("last-event-id", &last.to_string())
            .map_err(js)?;
    }

    let encoded = js_sys::encode_uri_component(topics)
        .as_string()
        .unwrap_or_default();
    let init = RequestInit::new();
    init.set_method("GET");
    init.set_headers(&headers);
    init.set_mode(RequestMode::SameOrigin);
    init.set_credentials(RequestCredentials::SameOrigin);
    init.set_signal(Some(signal));
    let request = Request::new_with_str_and_init(&format!("/api/events?topics={encoded}"), &init)
        .map_err(js)?;

    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(js)?;
    let response: Response = response
        .dyn_into()
        .map_err(|_| ApiError::Network("fetch did not return a Response".into()))?;
    if !response.ok() {
        let status = response.status();
        let text = JsFuture::from(response.text().map_err(js)?)
            .await
            .map_err(js)?
            .as_string()
            .unwrap_or_default();
        return Err(api::problem(status, &text));
    }

    let body = response
        .body()
        .ok_or_else(|| ApiError::Network("the event stream has no body".into()))?;
    let reader: ReadableStreamDefaultReader = body
        .get_reader()
        .dyn_into()
        .map_err(|_| ApiError::Network("the event stream has no reader".into()))?;
    let decoder = TextDecoder::new().map_err(js)?;
    let mut pending: Vec<u8> = Vec::new();
    let mut last_id = last;

    loop {
        if signal.aborted() {
            return Ok(last_id);
        }
        let read = JsFuture::from(reader.read()).await.map_err(js)?;
        let done = js_sys::Reflect::get(&read, &JsValue::from_str("done"))
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if done {
            return Ok(last_id);
        }
        let value = js_sys::Reflect::get(&read, &JsValue::from_str("value"))
            .map_err(|_| ApiError::Network("the stream chunk has no value".into()))?;
        let bytes: Uint8Array = value
            .dyn_into()
            .map_err(|_| ApiError::Network("the stream chunk is not bytes".into()))?;
        pending.extend_from_slice(&bytes.to_vec());

        // Complete frames only: a split multi-byte character is never decoded half-way.
        while let Some(index) = find_double_newline(&pending) {
            let frame_bytes: Vec<u8> = pending.drain(..index + 2).collect();
            let text = decoder
                .decode_with_u8_array(&frame_bytes[..frame_bytes.len() - 2])
                .map_err(js)?;
            if let Some(frame) = parse_frame(&text) {
                if let Some(id) = frame.id {
                    last_id = Some(id);
                }
                on(Ok(frame));
            }
        }
    }
}

fn find_double_newline(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|pair| pair == b"\n\n")
}

fn parse_frame(text: &str) -> Option<Frame> {
    let mut id = None;
    let mut event = String::from("message");
    let mut data = String::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "id" => id = value.trim().parse::<i64>().ok(),
            "event" => event = value.to_owned(),
            "data" => {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value);
            }
            _ => {}
        }
    }
    if data.is_empty() && event == "message" {
        return None;
    }
    let data = serde_json::from_str(&data)
        .ok()
        .or_else(|| (!data.is_empty()).then(|| Value::String(data.clone())));
    Some(Frame { id, event, data })
}

async fn sleep(milliseconds: i32) {
    let window = match web_sys::window() {
        Some(window) => window,
        None => return,
    };
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let _ =
            window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, milliseconds);
    });
    let _ = JsFuture::from(promise).await;
}

fn js(value: JsValue) -> ApiError {
    ApiError::Network(value.as_string().unwrap_or_else(|| format!("{value:?}")))
}
