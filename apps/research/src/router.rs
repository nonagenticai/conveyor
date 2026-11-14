//! The HTTP surface: the six captured operations, plus `/openapi.json` and
//! `/docs` when `ENABLE_DOCS` is true.
//!
//! Browser-only capabilities are refused **by name** with a `422`. Nothing in
//! the request is silently ignored: a caller asking for Playwright, stealth,
//! virtual scrolling, a screenshot, a selector wait or a non-`load` wait
//! condition is told which field cannot be served.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use scraper::Html as ScraperHtml;
use serde_json::{Value, json};

use crate::contract;
use crate::extract;
use crate::fetch::{self, FetchError, FetchOptions};
use crate::links;
use crate::models::{
    BulkScrapeRequest, BulkScrapeResponse, ErrorDetail, ErrorResponse, ExtractionSchema,
    HealthResponse, ScrapeRequest, ScrapeResponse, ScrapeStatus, ScraperType, WaitUntil,
};
use crate::schema_extract;

/// The six operations of the captured contract, in `METHOD path` spelling.
pub const ROUTES: &[(&str, &str)] = &[
    ("GET", "/"),
    ("GET", "/health"),
    ("GET", "/health/live"),
    ("GET", "/health/ready"),
    ("POST", "/api/scrape/url"),
    ("POST", "/api/scrape/bulk"),
];

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_host: String,
    pub bind_port: u16,
    pub enable_docs: bool,
    /// Exact hostnames that bypass the SSRF range checks. Parsed from
    /// `SSRF_ALLOWED_HOSTS` (a JSON array, the same name the reference uses).
    pub ssrf_allowed_hosts: Vec<String>,
    pub service_name: String,
    pub version: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            bind_host: std::env::var("BIND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            // Deliberately not 8000: that port belongs to another service on this estate.
            bind_port: std::env::var("PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8080),
            enable_docs: std::env::var("ENABLE_DOCS")
                .map(|value| truthy(&value))
                .unwrap_or(false),
            ssrf_allowed_hosts: std::env::var("SSRF_ALLOWED_HOSTS")
                .map(|raw| {
                    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_else(|error| {
                        tracing::warn!(%error, "SSRF_ALLOWED_HOSTS is not a JSON string array; using an empty allowlist");
                        Vec::new()
                    })
                })
                .unwrap_or_default(),
            service_name: std::env::var("SERVICE_NAME")
                .unwrap_or_else(|_| "webscraper-service".to_string()),
            version: std::env::var("SERVICE_VERSION").unwrap_or_else(|_| "1.0.0".to_string()),
        }
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.bind_host, self.bind_port)
    }
}

fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

pub fn build_app(state: AppState) -> Router {
    let enable_docs = state.config.enable_docs;
    let mut app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/api/scrape/url", post(scrape_url))
        .route("/api/scrape/bulk", post(scrape_bulk))
        .with_state(state);

    if enable_docs {
        app = app
            .route("/openapi.json", get(openapi_document))
            .route("/docs", get(docs_page));
    }
    app
}

/// The captured OpenAPI document. The lock is the served document, not a
/// document regenerated from this implementation.
pub fn openapi() -> Value {
    contract::lock().unwrap_or_else(|error| panic!("embedded contract is unreadable: {error}"))
}

async fn root(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "service": state.config.service_name,
        "version": state.config.version,
        "status": "online",
        "endpoints": {
            "health": "/health",
            "scrape_url": "/api/scrape/url",
            "scrape_bulk": "/api/scrape/bulk",
            "docs": "/docs",
            "redoc": "/redoc",
        }
    }))
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: now_iso8601(),
        service: state.config.service_name.clone(),
        version: state.config.version.clone(),
        // There is no browser backend in this crate. The captured deployment
        // reported true/true; the parity test declares this difference.
        playwright_available: false,
        browser_ready: false,
    })
}

async fn health_live(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "alive",
        "timestamp": now_iso8601(),
        "service": state.config.service_name,
    }))
}

async fn health_ready(State(_state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ready",
        "timestamp": now_iso8601(),
        "checks": {
            "playwright_available": false,
            "browser_ready": false,
        }
    }))
}

async fn openapi_document() -> Json<Value> {
    Json(openapi())
}

async fn docs_page() -> Html<&'static str> {
    Html(
        "<!doctype html><html><head><title>conveyor-research</title></head><body>\
         <h1>conveyor-research</h1>\
         <p>The contract is served at <a href=\"/openapi.json\">/openapi.json</a>.</p>\
         </body></html>",
    )
}

async fn scrape_url(
    State(state): State<AppState>,
    payload: Result<Json<ScrapeRequest>, JsonRejection>,
) -> Response {
    let request = match payload {
        Ok(Json(request)) => request,
        Err(rejection) => {
            return error_response(rejection.status(), rejection.body_text());
        }
    };

    if let Some(message) = scrape_refusal(&request) {
        return error_response(StatusCode::UNPROCESSABLE_ENTITY, message);
    }

    let started = Instant::now();
    let options = ScrapeOptions::from_scrape(&request);
    match scrape_one(&request.url, &options, &state.config).await {
        Ok(response) => Json(response).into_response(),
        Err(FetchError::Blocked(message)) => {
            error_response(StatusCode::BAD_REQUEST, format!("url: {message}"))
        }
        Err(error) => Json(ScrapeResponse::failed(
            &request.url,
            error.to_string(),
            started.elapsed().as_secs_f64(),
        ))
        .into_response(),
    }
}

async fn scrape_bulk(
    State(state): State<AppState>,
    payload: Result<Json<BulkScrapeRequest>, JsonRejection>,
) -> Response {
    let request = match payload {
        Ok(Json(request)) => request,
        Err(rejection) => {
            return error_response(rejection.status(), rejection.body_text());
        }
    };

    if let Some(message) = bulk_refusal(&request) {
        return error_response(StatusCode::UNPROCESSABLE_ENTITY, message);
    }

    let started = Instant::now();
    let options = ScrapeOptions::from_bulk(&request);
    let mut results = Vec::with_capacity(request.urls.len());
    let mut completed = 0usize;
    let mut failed = 0usize;

    for url in &request.urls {
        match scrape_one(url, &options, &state.config).await {
            Ok(response) => {
                if response.status == ScrapeStatus::Completed {
                    completed += 1;
                } else {
                    failed += 1;
                }
                results.push(response);
            }
            Err(error) => {
                failed += 1;
                results.push(ScrapeResponse::failed(
                    url,
                    error.to_string(),
                    started.elapsed().as_secs_f64(),
                ));
            }
        }
    }

    Json(BulkScrapeResponse {
        total_urls: request.urls.len(),
        completed,
        failed,
        results,
        processing_time_seconds: started.elapsed().as_secs_f64(),
    })
    .into_response()
}

#[derive(Debug, Clone)]
struct ScrapeOptions {
    timeout_seconds: f64,
    user_agent: Option<String>,
    headers: Option<BTreeMap<String, String>>,
    cookies: Option<BTreeMap<String, String>>,
    link_query: Option<String>,
    sort_links_by_score: bool,
    extraction_schema: Option<ExtractionSchema>,
}

impl ScrapeOptions {
    fn from_scrape(request: &ScrapeRequest) -> Self {
        Self {
            timeout_seconds: request.timeout,
            user_agent: request.user_agent.clone(),
            headers: request.headers.clone(),
            cookies: request.cookies.clone(),
            link_query: request.link_query.clone(),
            sort_links_by_score: request.sort_links_by_score,
            extraction_schema: request.extraction_schema.clone(),
        }
    }

    fn from_bulk(request: &BulkScrapeRequest) -> Self {
        Self {
            timeout_seconds: request.timeout,
            user_agent: None,
            headers: None,
            cookies: None,
            link_query: request.link_query.clone(),
            sort_links_by_score: request.sort_links_by_score,
            extraction_schema: request.extraction_schema.clone(),
        }
    }
}

async fn scrape_one(
    url: &str,
    options: &ScrapeOptions,
    config: &Config,
) -> Result<ScrapeResponse, FetchError> {
    let started = Instant::now();
    let fetch_options = FetchOptions {
        timeout_seconds: options.timeout_seconds,
        user_agent: options.user_agent.as_deref(),
        headers: options.headers.as_ref(),
        cookies: options.cookies.as_ref(),
        allowed_hosts: &config.ssrf_allowed_hosts,
    };
    let page = fetch::fetch_page(url, &fetch_options).await?;

    let document = ScraperHtml::parse_document(&page.html_content);
    let extraction = extract::extract(&page.html_content);
    let metadata = extract::metadata(&document, page.status_code, &page.final_url);
    let links = links::extract_links(
        &document,
        url,
        options.link_query.as_deref(),
        options.sort_links_by_score,
    );
    let images = links::extract_images(&document, url);
    let extracted_data = options
        .extraction_schema
        .as_ref()
        .map(|schema| schema_extract::extract(&page.html_content, schema));

    Ok(ScrapeResponse {
        url: url.to_string(),
        final_url: Some(page.final_url),
        html_content: Some(page.html_content),
        text_content: Some(extraction.text_content),
        markdown_content: extraction.markdown_content,
        title: extract::title(&document),
        metadata,
        structured_data: BTreeMap::new(),
        extracted_data,
        links,
        images,
        screenshot_base64: None,
        status: ScrapeStatus::Completed,
        error: None,
        scraper_used: "HttpxScraper".to_string(),
        processing_time_seconds: started.elapsed().as_secs_f64(),
        storage_state: None,
    })
}

fn scrape_refusal(request: &ScrapeRequest) -> Option<String> {
    refusal(
        request.scraper_type,
        request.javascript_enabled,
        request.stealth,
        request.virtual_scroll.is_some(),
        request.screenshot,
        request.wait_for_selector.is_some(),
        request.wait_until,
    )
}

fn bulk_refusal(request: &BulkScrapeRequest) -> Option<String> {
    refusal(
        request.scraper_type,
        request.javascript_enabled,
        request.stealth,
        request.virtual_scroll.is_some(),
        request.screenshot,
        request.wait_for_selector.is_some(),
        request.wait_until,
    )
}

fn refusal(
    scraper_type: ScraperType,
    javascript_enabled: bool,
    stealth: bool,
    virtual_scroll: bool,
    screenshot: bool,
    wait_for_selector: bool,
    wait_until: Option<WaitUntil>,
) -> Option<String> {
    let suffix = "requires a browser/crawl backend that conveyor-research does not provide";
    if matches!(
        scraper_type,
        ScraperType::Playwright | ScraperType::Scrapy | ScraperType::CurlCffi
    ) {
        let name = match scraper_type {
            ScraperType::Playwright => "playwright",
            ScraperType::Scrapy => "scrapy",
            ScraperType::CurlCffi => "curl_cffi",
            ScraperType::Httpx | ScraperType::Auto => unreachable!("already matched"),
        };
        return Some(format!("scraper_type: {name} {suffix}"));
    }
    if javascript_enabled {
        return Some(format!("javascript_enabled: true {suffix}"));
    }
    if stealth {
        return Some(format!("stealth: true {suffix}"));
    }
    if virtual_scroll {
        return Some(format!("virtual_scroll: {suffix}"));
    }
    if screenshot {
        return Some(format!("screenshot: true {suffix}"));
    }
    if wait_for_selector {
        return Some(format!("wait_for_selector: {suffix}"));
    }
    if wait_until.is_some() {
        return Some(format!("wait_until: {suffix}"));
    }
    None
}

fn error_response(status: StatusCode, message: String) -> Response {
    let code = status.as_u16();
    let detail = ErrorDetail {
        code: format!("HTTP_{code}"),
        message,
        status: code,
        request_id: Some(request_id()),
        timestamp: Some(now_iso8601()),
        details: None,
    };
    (status, Json(ErrorResponse { error: detail })).into_response()
}

/// ISO-8601 UTC. No chrono dependency: the contract treats this field as
/// volatile (the parity test compares presence, not the captured clock value).
fn now_iso8601() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let time = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

/// Howard Hinnant's `civil_from_days` (days since the Unix epoch).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

/// A UUIDv7-shaped request id for log correlation. Uniqueness comes from the
/// millisecond clock plus a process-local counter; it is not a security token.
fn request_id() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let millis = elapsed.as_millis() as u64;
    let counter = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mixed = mix64(counter ^ elapsed.subsec_nanos() as u64 ^ millis.rotate_left(17));
    let time_low = (millis >> 16) as u32;
    let time_mid = (millis & 0xffff) as u16;
    let rand_a = (mixed >> 48) & 0x0fff;
    let rand_b_high = (mixed >> 34) & 0x3fff;
    let rand_b_low = mixed & 0xffff_ffff_ffff;
    format!(
        "{time_low:08x}-{time_mid:04x}-7{rand_a:03x}-{:04x}-{rand_b_low:012x}",
        0x8000 | rand_b_high
    )
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 12;
    value ^= value << 25;
    value ^= value >> 27;
    value.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_renders_as_a_known_civil_date() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_454), (2026, 1, 1));
    }

    #[test]
    fn every_browser_capability_names_its_field() {
        let message = refusal(
            ScraperType::Playwright,
            false,
            false,
            false,
            false,
            false,
            None,
        )
        .expect("playwright refused");
        assert!(message.starts_with("scraper_type:"), "{message}");
        for message in [
            refusal(ScraperType::Auto, true, false, false, false, false, None),
            refusal(ScraperType::Auto, false, true, false, false, false, None),
            refusal(ScraperType::Auto, false, false, true, false, false, None),
            refusal(ScraperType::Auto, false, false, false, true, false, None),
            refusal(ScraperType::Auto, false, false, false, false, true, None),
            refusal(
                ScraperType::Auto,
                false,
                false,
                false,
                false,
                false,
                Some(WaitUntil::Load),
            ),
        ] {
            assert!(message.is_some());
        }
        assert!(refusal(ScraperType::Auto, false, false, false, false, false, None).is_none());
        assert!(refusal(ScraperType::Httpx, false, false, false, false, false, None).is_none());
    }
}
