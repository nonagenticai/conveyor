#![forbid(unsafe_code)]
//! The webscraper's read actions (T14, SPEC_V2 §11.2/§11.9; ADR 0006 §4, ADR 0007).
//!
//! `apps/research` is two binaries with two destinations. This module declares the **scraper**
//! (`conveyor-research`) only: its two POST operations, `/api/scrape/url` and `/api/scrape/bulk`,
//! are static-path calls with no authentication, which is exactly what the external engine can
//! dispatch.
//!
//! The **media** binary (`conveyor-research-media`) is deliberately **not** ported here, and the
//! reason is recorded rather than hidden:
//!
//! * every `/api/*` route authenticates on `X-Tenant-Id` (`apps/research/src/media/tenant.rs`);
//!   `HandlerRef::External` can send only `Authorization: Bearer`, it has no custom-header field,
//!   so a call would be a `401` by construction;
//! * `/api/downloads` answers `501` (no adapter is enabled) and the item/collection reads by id
//!   are `GET /api/items/{id}` style, which the engine cannot express (a static path, no path
//!   templating).
//!
//! Porting the media surface is therefore a follow-up unit on the engine's header contract or on
//! the media service's auth, not a tool declaration this issue can make. The provenance seed
//! records the five `media.*` rows as `DROP` with that measurement.
//!
//! Every input is the app's own request body, closed at the top level: an unknown field is a
//! `422` from the engine's schema validation before any call reaches the service. The app
//! validates the same constraints again (a required URL, the scraper enum, the timeout), so the
//! declaration and the implementation cannot drift silently.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::ActionRegistration;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, HandlerRef, Schema, SchemaRef,
    Scope, Tenancy,
};

/// Every scraper read carries the integrations read scope.
const READ: &[Scope] = &[Scope::IntegrationsRead];

/// The service alias; its base URL is `CONVEYOR_EXTERNAL_WEBSCRAPER_URL`.
const SERVICE: &str = "webscraper";

/// The upstream document, passed through unchanged.
///
/// The conveyor does not re-model another service's answer: it validates nothing, rewrites
/// nothing and stores nothing. A typed copy would drift the day the service adds a field.
fn passthrough_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The webscraper's document, passed through unchanged.",
        "additionalProperties": true
    })
}

/// Build one read spec. The input is the operation's own request body.
const fn spec(
    name: &'static str,
    path: &'static str,
    timeout_ms: u32,
    input: fn() -> Schema,
) -> ActionSpec {
    ActionSpec {
        name: Cow::Borrowed(name),
        version: 1,
        kind: ActionKind::External,
        input: SchemaRef::Compiled(input),
        output: SchemaRef::Compiled(passthrough_output),
        scopes: Cow::Borrowed(READ),
        idempotent: true,
        timeout_ms,
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        task_capable: false,
        captures_usage: false,
        artefact: None,
        approval_target: None,
        handler: HandlerRef::External {
            service: Cow::Borrowed(SERVICE),
            method: Cow::Borrowed("POST"),
            path: Cow::Borrowed(path),
            auth: None,
            auth_optional: false,
            upload: None,
        },
    }
}

/// `POST /api/scrape/url` (the app's `ScrapeRequest`).
///
/// The nested extraction/virtual-scroll/auth documents are the app's own shapes: they are passed
/// through as JSON so this declaration cannot freeze a configuration the app evolves, while the
/// top-level field set stays closed.
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ScrapeIn {
    /// The page to fetch.
    url: String,
    /// `playwright | scrapy | httpx | curl_cffi | auto`; the app defaults to `auto`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scraper_type: Option<String>,
    /// Render JavaScript before extracting; the app defaults to false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    javascript_enabled: Option<bool>,
    /// The fetch timeout in seconds; the app defaults to 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout: Option<f64>,
    /// Wait for this selector before extracting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wait_for_selector: Option<String>,
    /// `load | domcontentloaded | networkidle | commit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wait_until: Option<String>,
    /// Capture a screenshot with the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    screenshot: Option<bool>,
    /// The viewport width; the app defaults to 1920.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    viewport_width: Option<u32>,
    /// The viewport height; the app defaults to 1080.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    viewport_height: Option<u32>,
    /// A custom user agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_agent: Option<String>,
    /// Extra request headers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    headers: Option<BTreeMap<String, String>>,
    /// Request cookies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cookies: Option<BTreeMap<String, String>>,
    /// A query that selects the links to follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link_query: Option<String>,
    /// Rank links by their relevance score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sort_links_by_score: Option<bool>,
    /// Use the stealth profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stealth: Option<bool>,
    /// Delay between requests, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delay: Option<f64>,
    /// Download the page's images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    download_images: Option<bool>,
    /// The app's `ExtractionSchema` document (baseSelector, name, fields).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    extraction_schema: Option<serde_json::Value>,
    /// Bypass the cache for this request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    no_cache: Option<bool>,
    /// The app's `VirtualScrollConfig` document (container_selector, scroll_count, scroll_by, wait_ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    virtual_scroll: Option<serde_json::Value>,
    /// The app's `AuthConfig` document (credential_id, login_url, selectors, type, wait_timeout_ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_config: Option<serde_json::Value>,
}

fn scrape_input() -> Schema {
    schemars::schema_for!(ScrapeIn)
}

/// `POST /api/scrape/bulk` (the app's `BulkScrapeRequest`).
#[derive(schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct BulkIn {
    /// The pages to fetch.
    urls: Vec<String>,
    /// `playwright | scrapy | httpx | curl_cffi | auto`; the app defaults to `auto`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scraper_type: Option<String>,
    /// Render JavaScript before extracting; the app defaults to false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    javascript_enabled: Option<bool>,
    /// The fetch timeout in seconds; the app defaults to 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout: Option<f64>,
    /// Follow links found on each page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    follow_links: Option<bool>,
    /// The crawl depth when following links; the app defaults to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_depth: Option<u32>,
    /// Restrict the crawl to these domains.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowed_domains: Option<Vec<String>>,
    /// A query that selects the links to follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link_query: Option<String>,
    /// Rank links by their relevance score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sort_links_by_score: Option<bool>,
    /// Delay between requests, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delay: Option<f64>,
    /// Download each page's images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    download_images: Option<bool>,
    /// The app's `ExtractionSchema` document (baseSelector, name, fields).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    extraction_schema: Option<serde_json::Value>,
    /// Capture a screenshot with each page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    screenshot: Option<bool>,
    /// Use the stealth profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stealth: Option<bool>,
    /// The app's `VirtualScrollConfig` document (container_selector, scroll_count, scroll_by, wait_ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    virtual_scroll: Option<serde_json::Value>,
    /// Wait for this selector before extracting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wait_for_selector: Option<String>,
    /// `load | domcontentloaded | networkidle | commit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wait_until: Option<String>,
    /// The app's `AuthConfig` document (credential_id, login_url, selectors, type, wait_timeout_ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_config: Option<serde_json::Value>,
}

fn bulk_input() -> Schema {
    schemars::schema_for!(BulkIn)
}

/// `webscraper.scrape_url` — fetch one page and return its text, links and extracted data.
fn scrape_url() -> ActionSpec {
    spec(
        "webscraper.scrape_url",
        "/api/scrape/url",
        120_000,
        scrape_input,
    )
}
inventory::submit! { ActionRegistration::new(scrape_url) }

/// `webscraper.scrape_bulk` — fetch a set of pages, optionally following links.
fn scrape_bulk() -> ActionSpec {
    spec(
        "webscraper.scrape_bulk",
        "/api/scrape/bulk",
        300_000,
        bulk_input,
    )
}
inventory::submit! { ActionRegistration::new(scrape_bulk) }
