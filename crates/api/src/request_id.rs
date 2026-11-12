//! `X-Request-Id`: accepted or minted, echoed on every response, recorded for audit.
//!
//! The same middleware fills `instance` and `request_id` into every `problem+json` body.
//! `IntoResponse` for [`crate::ApiError`] runs without the request, and an extractor
//! rejection (401) must carry the same contract as a handler's error, so the middleware —
//! the one place that has both — completes the document. Problem bodies are tiny; only a
//! `problem+json` response is buffered.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

/// The header, lowercase as axum stores it.
pub const HEADER: &str = "x-request-id";

/// The request id of this request, available to handlers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestId(pub String);

/// A supplied id is used only if it is 1–128 printable ASCII characters. Anything else is
/// replaced, never echoed: a header reflected into logs and audit rows must not carry
/// control characters or unbounded length.
fn acceptable(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

fn header_name() -> HeaderName {
    HeaderName::from_static(HEADER)
}

/// The middleware.
pub async fn layer(mut request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get(HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| acceptable(value))
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    let instance = request.uri().path().to_owned();
    let method = request.method().as_str().to_owned();
    request.extensions_mut().insert(RequestId(id.clone()));

    let started = std::time::Instant::now();
    let mut response = next.run(request).await;
    record_request(
        &method,
        response.status().as_u16(),
        started.elapsed().as_secs_f64(),
    );
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(header_name(), value);
    }
    fill_problem(response, &id, &instance).await
}

async fn fill_problem(response: Response, request_id: &str, instance: &str) -> Response {
    let is_problem = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/problem+json"));
    if !is_problem {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(bytes) => bytes,
        // The body was not collectable (or exceeded the cap). Keep the status and headers
        // rather than turning a 4xx into a different failure.
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };
    let value = match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(value) => value,
        Err(_) => return Response::from_parts(parts, Body::from(bytes)),
    };
    let mut value = value;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "request_id".to_owned(),
            serde_json::Value::String(request_id.to_owned()),
        );
        if object
            .get("instance")
            .is_none_or(serde_json::Value::is_null)
        {
            object.insert(
                "instance".to_owned(),
                serde_json::Value::String(instance.to_owned()),
            );
        }
    }
    let body = serde_json::to_vec(&value).unwrap_or_else(|_| bytes.to_vec());
    parts.headers.remove(header::CONTENT_LENGTH);
    if let Ok(length) = HeaderValue::from_str(&body.len().to_string()) {
        parts.headers.insert(header::CONTENT_LENGTH, length);
    }
    Response::from_parts(parts, Body::from(body))
}

/// Record one request. Kept here so status/method instrumentation lives with the id.
pub fn record_request(method: &str, status: u16, seconds: f64) {
    metrics::counter!(
        "conveyor_http_requests_total",
        "method" => method.to_owned(),
        "status" => status.to_string()
    )
    .increment(1);
    metrics::histogram!("conveyor_http_request_duration_seconds", "method" => method.to_owned())
        .record(seconds);
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    async fn app() -> Router {
        Router::new()
            .route("/ok", get(|| async { "ok" }))
            .route(
                "/fail",
                get(|| async { crate::error::ApiError::unauthorized("no credential") }),
            )
            .layer(axum::middleware::from_fn(layer))
    }

    #[tokio::test]
    async fn a_supplied_request_id_is_echoed() {
        let response = app()
            .await
            .oneshot(
                Request::builder()
                    .uri("/ok")
                    .header(HEADER, "abc-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(HEADER).and_then(|v| v.to_str().ok()),
            Some("abc-123")
        );
    }

    #[tokio::test]
    async fn a_missing_or_invalid_request_id_is_minted() {
        let too_long = "a".repeat(129);
        for supplied in [None, Some(""), Some("has space"), Some(too_long.as_str())] {
            let mut request = Request::builder().uri("/ok");
            if let Some(value) = supplied {
                request = request.header(HEADER, value);
            }
            let response = request.body(Body::empty()).unwrap();
            let response = app().await.oneshot(response).await.unwrap();
            let id = response
                .headers()
                .get(HEADER)
                .and_then(|v| v.to_str().ok())
                .expect("every response carries an id");
            assert_eq!(id.len(), 36, "{id} is a uuid v7");
        }
    }

    #[tokio::test]
    async fn a_problem_body_is_completed_with_instance_and_request_id() {
        let response = app()
            .await
            .oneshot(
                Request::builder()
                    .uri("/fail")
                    .header(HEADER, "req-9")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["request_id"], "req-9");
        assert_eq!(body["instance"], "/fail");
    }
}
