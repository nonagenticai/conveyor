//! RFC 9457 `application/problem+json` (CONTRACT §5).
//!
//! `type` is a stable slug, not a URL that 404s. `instance` (the request path) and
//! `request_id` are filled by the request-id middleware: `IntoResponse` runs without the
//! request, and every rejection — including one raised by an extractor — must carry the
//! same contract. The middleware rewrites only `application/problem+json` bodies.

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

/// One field-level validation failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct FieldError {
    /// The input member (or path/query parameter) that failed.
    pub field: String,
    /// A stable slug for the failure.
    pub code: String,
    /// What was wrong, in the caller's terms.
    pub detail: String,
}

/// The problem document. Field names are the RFC's; `type` is serialized as-is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct ProblemDetail {
    /// A stable slug for this class of problem (`unauthorized`, `not_found`, …).
    #[serde(rename = "type")]
    pub problem_type: String,
    /// A short human-readable summary, the same for every occurrence.
    pub title: String,
    /// The HTTP status code, repeated in the body.
    pub status: u16,
    /// This occurrence's explanation.
    pub detail: String,
    /// The request path; filled by the request-id middleware.
    pub instance: Option<String>,
    /// The request id, echoed in `X-Request-Id`; filled by the middleware.
    pub request_id: Option<String>,
    /// Validation failures, omitted when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<FieldError>,
}

/// An HTTP error that renders as `problem+json`.
#[derive(Debug, thiserror::Error)]
#[error("{code}: {detail}")]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    detail: String,
    errors: Vec<FieldError>,
}

impl ApiError {
    /// Build from a status and a slug.
    pub fn new(status: StatusCode, code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status,
            code,
            detail: detail.into(),
            errors: Vec::new(),
        }
    }

    /// 400 — the request is malformed.
    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", detail)
    }

    /// 401 — no credential, or one this service refuses.
    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", detail)
    }

    /// 403 — authenticated but not permitted.
    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", detail)
    }

    /// 404 — no such resource in this tenant.
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", detail)
    }

    /// 409 — the state moved, or a live duplicate exists.
    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", detail)
    }

    /// 422 — the body parsed but failed a domain rule.
    pub fn unprocessable(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "unprocessable", detail)
    }

    /// 503 — a dependency this request needs is unavailable.
    pub fn service_unavailable(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            detail,
        )
    }

    /// 500 — an internal failure. The detail never includes a database message.
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", detail)
    }

    /// Attach field-level validation failures (422).
    pub fn with_errors(mut self, errors: Vec<FieldError>) -> Self {
        self.errors = errors;
        self
    }

    /// The stable slug (for logs and tests).
    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(error = %error, "database error");
        Self::internal("the request could not be completed")
    }
}

impl From<conveyor_auth::AuthError> for ApiError {
    fn from(error: conveyor_auth::AuthError) -> Self {
        use conveyor_auth::AuthError;
        match &error {
            AuthError::Unauthenticated
            | AuthError::UntrustedIssuer
            | AuthError::InvalidToken
            | AuthError::NotProvisioned { .. } => Self::unauthorized(error.to_string()),
            AuthError::ActiveTenantRequired { .. } => Self::new(
                StatusCode::BAD_REQUEST,
                "active_tenant_required",
                error.to_string(),
            ),
            AuthError::ActiveTenantInvalid(_) => Self::new(
                StatusCode::BAD_REQUEST,
                "active_tenant_invalid",
                error.to_string(),
            ),
            AuthError::ActiveTenantNotPermitted => Self::new(
                StatusCode::FORBIDDEN,
                "active_tenant_not_permitted",
                error.to_string(),
            ),
            AuthError::TenantNotActive { .. } => Self::forbidden(error.to_string()),
            AuthError::JwksUnavailable(_)
            | AuthError::Config(_)
            | AuthError::LicenceUnavailable(_) => {
                tracing::error!(error = %error, "authentication dependency failed");
                Self::service_unavailable("authentication is temporarily unavailable")
            }
            AuthError::UnknownRole(_)
            | AuthError::UnknownPrincipalKind(_)
            | AuthError::Store(_) => {
                tracing::error!(error = %error, "identity resolution failed");
                Self::internal("the request could not be completed")
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let problem = ProblemDetail {
            problem_type: self.code.to_owned(),
            title: self.status.canonical_reason().unwrap_or("Error").to_owned(),
            status: self.status.as_u16(),
            detail: self.detail,
            instance: None,
            request_id: None,
            errors: self.errors,
        };
        (
            self.status,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/problem+json"),
            )],
            Json(problem),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::response::IntoResponse;

    use super::*;

    #[tokio::test]
    async fn a_problem_has_the_rfc_fields_and_the_problem_content_type() {
        let response = ApiError::not_found("no such card").into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json")
        );
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["type"], "not_found");
        assert_eq!(body["title"], "Not Found");
        assert_eq!(body["status"], 404);
        assert_eq!(body["detail"], "no such card");
        assert!(
            body.get("instance").is_some(),
            "the field must be present for the middleware"
        );
        assert!(body.get("request_id").is_some());
        assert!(
            body.get("errors").is_none(),
            "no validation errors: the member is omitted"
        );
    }

    #[tokio::test]
    async fn validation_errors_are_serialized() {
        let response = ApiError::unprocessable("invalid input")
            .with_errors(vec![FieldError {
                field: "title".into(),
                code: "required".into(),
                detail: "must not be empty".into(),
            }])
            .into_response();
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["errors"][0]["field"], "title");
        assert_eq!(body["errors"][0]["code"], "required");
    }

    #[test]
    fn database_failures_do_not_leak_their_message() {
        let error = ApiError::from(sqlx::Error::RowNotFound);
        assert_eq!(error.code(), "internal_error");
        assert!(!format!("{error}").contains("row not found"), "{error}");
    }
}
