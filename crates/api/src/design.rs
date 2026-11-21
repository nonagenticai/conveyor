#![forbid(unsafe_code)]
//! The live, unauthenticated `/design` window.
//!
//! Three paths answer from Postgres for the configured public tenant
//! (`CONVEYOR_PUBLIC_TENANT`); every other `/design` path stays the static pack:
//!
//! ```text
//! /design/data/snapshot.js          window.CONVEYOR_SNAPSHOT = {…}
//! /design/docs/outputs.js           window.CONVEYOR_OUTPUTS = {…}
//! /design/docs/output-<uuid>.html   one rendered published output
//! ```
//!
//! **Publication is the filter.** Only rows with `published_at IS NOT NULL AND status='active'`
//! and a body are served, in one read-only transaction built by
//! [`conveyor_pack::live::site_view`]. `publishable` alone is not publication.
//!
//! **Cache correctness.** A missing asset must never be cached as a 404: Cloudflare held a
//! static 404 for four hours on 2026-11-14. Every miss is `Cache-Control: no-store`; a hit
//! carries a content `ETag` and `max-age=0, must-revalidate`, so a publish is visible on the
//! next request. The built view is held for [`TTL`] in-process with a single-flight lock; a
//! database failure serves the last good view with `Warning: 110` when one exists, and a
//! `503` when none does.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use conveyor_pack::live::{SiteArgs, SiteView, site_view};
use conveyor_pack::snapshot::DEFAULT_FEEDBACK_URL;
use conveyor_store as store;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::state::AppState;

/// How long a built view is served before it is rebuilt. Short on purpose: a publish is a
/// row change, and the public URL must reflect it immediately.
const TTL: Duration = Duration::from_secs(5);

/// The HTTP cache directive on a live hit. `max-age=0` + `must-revalidate` means every reuse is
/// validated against the ETag; the content is never served stale from a CDN after a publish.
const HIT_CACHE: &str = "public, max-age=0, must-revalidate";

/// The directive on a miss. A 404 here may become a 200 the moment someone publishes; caching
/// the absence is the failure measured on 2026-11-14.
const MISS_CACHE: &str = "no-store";

/// The in-process view cache. One public tenant per process, so one slot is enough.
#[derive(Default)]
pub struct DesignCache {
    inner: Mutex<Option<Cached>>,
}

impl DesignCache {
    /// A cache with no built view.
    pub fn new() -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for DesignCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DesignCache")
    }
}

struct Cached {
    view: Arc<SiteView>,
    built: Instant,
}

/// A live path, parsed.
enum LivePath {
    /// `/design/data/snapshot.js`
    Snapshot,
    /// `/design/data/programme.json` — the programme section, downloadable.
    Programme,
    /// `/design/data/programme.csv` — the same rows as CSV.
    ProgrammeCsv,
    /// `/design/docs/outputs.js`
    Outputs,
    /// `/design/docs/output-<uuid>.html`
    Page(Uuid),
    /// `/design/docs/attachment-<uuid>`
    Attachment(Uuid),
    /// The output-page shape with an id that does not parse. It is a live path, so it must answer
    /// `404 no-store` rather than fall through to the static 404.
    Malformed,
}

fn live_path(path: &str) -> Option<LivePath> {
    match path {
        "/design/data/snapshot.js" => return Some(LivePath::Snapshot),
        "/design/data/programme.json" => return Some(LivePath::Programme),
        "/design/data/programme.csv" => return Some(LivePath::ProgrammeCsv),
        "/design/docs/outputs.js" => return Some(LivePath::Outputs),
        _ => {}
    }
    if let Some(id) = path.strip_prefix("/design/docs/attachment-") {
        return Some(Uuid::parse_str(id).map_or(LivePath::Malformed, LivePath::Attachment));
    }
    let id = path
        .strip_prefix("/design/docs/output-")?
        .strip_suffix(".html")?;
    Some(Uuid::parse_str(id).map_or(LivePath::Malformed, LivePath::Page))
}

/// Answer a live design path, or `None` when this is not one.
///
/// Called before the static lookup. `GET` and `HEAD` only; any other method falls through and
/// ends as the API's 405/404, which is correct for a read-only public surface.
pub async fn serve(
    state: &AppState,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
) -> Option<Response> {
    if *method != Method::GET && *method != Method::HEAD {
        return None;
    }
    let live = live_path(uri.path())?;
    let head = *method == Method::HEAD;
    Some(answer(state, live, head, headers).await)
}

async fn answer(state: &AppState, live: LivePath, head: bool, headers: &HeaderMap) -> Response {
    let Some(slug) = state.config.public_tenant.as_deref() else {
        return miss(StatusCode::NOT_FOUND, "no public tenant is configured");
    };
    if matches!(live, LivePath::Malformed) {
        return miss(StatusCode::NOT_FOUND, "no such output");
    }
    // An attachment's bytes live in the object store, not in the cached site view: one read of the
    // row, one storage read, and the same ETag/`no-store` discipline as the other live paths.
    if let LivePath::Attachment(id) = live {
        return answer_attachment(state, slug, id, head, headers).await;
    }

    let (view, stale) = match build_or_cached(state, slug).await {
        Ok(value) => value,
        Err(Reason::UnknownTenant) => {
            return miss(StatusCode::NOT_FOUND, "no public tenant is configured");
        }
        Err(Reason::Database) => {
            return problem(
                StatusCode::SERVICE_UNAVAILABLE,
                "the public view is unavailable",
            );
        }
    };

    let (bytes, content_type) = match &live {
        LivePath::Snapshot => (
            view.snapshot_js.as_bytes(),
            "application/javascript; charset=utf-8",
        ),
        LivePath::Programme => (
            view.programme_json.as_bytes(),
            "application/json; charset=utf-8",
        ),
        LivePath::ProgrammeCsv => (view.programme_csv.as_bytes(), "text/csv; charset=utf-8"),
        LivePath::Outputs => (
            view.outputs_js.as_bytes(),
            "application/javascript; charset=utf-8",
        ),
        LivePath::Page(id) => match view.pages.get(id) {
            Some(page) => (page.as_bytes(), "text/html; charset=utf-8"),
            None => return miss(StatusCode::NOT_FOUND, "no such output"),
        },
        LivePath::Attachment(_) | LivePath::Malformed => {
            unreachable!("attachment and malformed paths answer before the build")
        }
    };

    let etag = format!("\"{}\"", sha256_hex(bytes));
    if let Some(response) = not_modified(headers, &etag) {
        return response;
    }
    hit(bytes, content_type, &etag, head, stale)
}

#[derive(Debug, Clone, Copy)]
enum Reason {
    UnknownTenant,
    Database,
}

/// Serve the cached view when fresh, otherwise rebuild it under a single-flight lock.
async fn build_or_cached(state: &AppState, slug: &str) -> Result<(Arc<SiteView>, bool), Reason> {
    let mut guard = state.design.inner.lock().await;

    if let Some(cached) = guard.as_ref()
        && cached.built.elapsed() < TTL
    {
        return Ok((Arc::clone(&cached.view), false));
    }

    let tenant = store::tenants::id_by_slug(&state.pool, slug)
        .await
        .map_err(|_| Reason::Database)?;
    let Some(tenant) = tenant else {
        return Err(Reason::UnknownTenant);
    };

    let issues = issues_registry();
    let view = site_view(
        &state.pool,
        SiteArgs {
            tenant,
            tenant_slug: slug,
            issues,
            feedback_url: DEFAULT_FEEDBACK_URL,
            git_sha: state.config.git_sha.as_deref().unwrap_or("unknown"),
        },
    )
    .await;

    match view {
        Ok(view) => {
            let view = Arc::new(view);
            *guard = Some(Cached {
                view: Arc::clone(&view),
                built: Instant::now(),
            });
            Ok((view, false))
        }
        Err(error) => {
            tracing::error!(%error, "the live design view could not be built");
            match guard.as_ref() {
                // A warm cache is the difference between a degraded page and a blank site.
                Some(cached) => Ok((Arc::clone(&cached.view), true)),
                None => Err(Reason::Database),
            }
        }
    }
}

/// The committed issue registry beside the pack, when the image carries one.
fn issues_registry() -> Value {
    let path = crate::web::web_root()
        .map(|root| root.join("design/data/issues.json"))
        .filter(|path| path.is_file());
    let Some(path) = path else {
        return json!({"issues": []});
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|value| {
            if value.get("issues").is_some() {
                value
            } else {
                json!({"issues": value})
            }
        })
        .unwrap_or_else(|| json!({"issues": []}))
}

fn not_modified(headers: &HeaderMap, etag: &str) -> Option<Response> {
    not_modified_with(headers, etag, HIT_CACHE)
}

fn not_modified_with(headers: &HeaderMap, etag: &str, cache: &'static str) -> Option<Response> {
    let candidate = headers.get(header::IF_NONE_MATCH)?.to_str().ok()?;
    if !candidate
        .split(',')
        .map(str::trim)
        .any(|value| value == etag || value == "*")
    {
        return None;
    }
    let mut response = StatusCode::NOT_MODIFIED.into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(header::ETAG, HeaderValue::from_str(etag).ok()?);
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    Some(response)
}

/// Serve one published attachment's bytes.
///
/// The row is read in the public tenant and must be an active attachment whose parent is active
/// and published (`published_at IS NOT NULL`). The bytes are re-hashed and compared with the
/// row's hash — a mismatch is corruption, never a file. Only PNG and JPEG are `inline`; PDF, SVG,
/// JSON, CSV and text are
/// downloads, with `nosniff` and a sandbox CSP on every response.
async fn answer_attachment(
    state: &AppState,
    slug: &str,
    id: Uuid,
    head: bool,
    headers: &HeaderMap,
) -> Response {
    let tenant = match store::tenants::id_by_slug(&state.pool, slug).await {
        Ok(Some(tenant)) => tenant,
        Ok(None) => return miss(StatusCode::NOT_FOUND, "no public tenant is configured"),
        Err(error) => {
            tracing::error!(%error, "the public tenant could not be read");
            return problem(
                StatusCode::SERVICE_UNAVAILABLE,
                "the public view is unavailable",
            );
        }
    };
    let mut tx = match store::begin_scoped_read_only(&state.pool, tenant).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "the attachment read could not start");
            return problem(
                StatusCode::SERVICE_UNAVAILABLE,
                "the public view is unavailable",
            );
        }
    };
    let record = match store::document::get_published_attachment_tx(&mut tx, tenant, id).await {
        Ok(record) => record,
        Err(error) => {
            tracing::error!(%error, "the attachment row could not be read");
            let _ = tx.rollback().await;
            return problem(
                StatusCode::SERVICE_UNAVAILABLE,
                "the public view is unavailable",
            );
        }
    };
    let _ = tx.rollback().await;
    let Some(record) = record else {
        return miss(StatusCode::NOT_FOUND, "no such attachment");
    };
    let (Some(key), Some(expected_hash)) =
        (record.object_key.as_deref(), record.content_hash.as_deref())
    else {
        return miss(StatusCode::NOT_FOUND, "no such attachment bytes");
    };
    let Some(objects) = conveyor_actions::integrations::objects::Objects::from_env() else {
        tracing::error!("the object store is not configured");
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "the attachment store is unavailable",
        );
    };
    let bytes = match objects.get(key).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::error!(%error, %id, "an attachment object could not be read");
            return problem(
                StatusCode::SERVICE_UNAVAILABLE,
                "the attachment could not be read",
            );
        }
    };
    if sha256_hex(&bytes) != expected_hash {
        tracing::error!(%id, "an attachment's bytes do not match its recorded hash");
        return problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the attachment failed its integrity check",
        );
    }
    let etag = format!("\"{expected_hash}\"");
    if let Some(response) = not_modified_with(headers, &etag, HIT_CACHE) {
        return response;
    }
    let content_type = record
        .meta
        .get("content_type")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream");
    let disposition = if matches!(content_type, "image/png" | "image/jpeg") {
        format!("inline; filename=\"{}\"", safe_filename(&record.title))
    } else {
        format!("attachment; filename=\"{}\"", safe_filename(&record.title))
    };
    let mut response = StatusCode::OK.into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    response_headers.insert(
        header::ETAG,
        HeaderValue::from_str(&etag).expect("a sha256 etag is a valid header value"),
    );
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(HIT_CACHE));
    response_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).expect("the filename is sanitised ASCII"),
    );
    response_headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    if !head {
        *response.body_mut() = Body::from(bytes);
    }
    response
}

/// A file name safe for a `Content-Disposition` value: ASCII letters, digits, space, `-`, `_`, `.`;
/// no leading dots; bounded. The attachment's title is user-controlled.
fn safe_filename(title: &str) -> String {
    let mut out = String::with_capacity(title.len().min(120));
    for character in title.chars().take(120) {
        if character.is_ascii_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.') {
            out.push(character);
        } else {
            out.push('_');
        }
    }
    let cleaned = out.trim().trim_matches('.').trim();
    if cleaned.is_empty() {
        "attachment".to_owned()
    } else {
        cleaned.to_owned()
    }
}

fn hit(bytes: &[u8], content_type: &str, etag: &str, head: bool, stale: bool) -> Response {
    let mut response = (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_owned()),
            (header::ETAG, etag.to_owned()),
            (header::CACHE_CONTROL, HIT_CACHE.to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        ],
    )
        .into_response();
    if content_type.starts_with("text/html") {
        response.headers_mut().insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'none'; style-src 'self' 'unsafe-inline'; font-src 'self'; \
                 img-src 'self' data: https:; base-uri 'none'; form-action 'none'; \
                 frame-ancestors 'none'",
            ),
        );
    }
    if stale && let Ok(value) = HeaderValue::from_str("110 - \"Response is stale\"") {
        response.headers_mut().insert(header::WARNING, value);
    }
    if !head {
        *response.body_mut() = Body::from(bytes.to_vec());
    }
    response
}

fn miss(status: StatusCode, detail: &str) -> Response {
    let mut response = (
        status,
        [(header::CACHE_CONTROL, MISS_CACHE)],
        axum::Json(json!({
            "type": "about:blank",
            "title": status.canonical_reason().unwrap_or("not found"),
            "status": status.as_u16(),
            "detail": detail,
        })),
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    response
}

fn problem(status: StatusCode, detail: &str) -> Response {
    let mut response = (
        status,
        [
            (header::CACHE_CONTROL, MISS_CACHE.to_owned()),
            (header::RETRY_AFTER, "5".to_owned()),
        ],
        axum::Json(json!({
            "type": "about:blank",
            "title": status.canonical_reason().unwrap_or("unavailable"),
            "status": status.as_u16(),
            "detail": detail,
        })),
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    response
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
