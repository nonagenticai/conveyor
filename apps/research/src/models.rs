//! Request and response types for the captured webscraper contract.
//!
//! Field names, serde defaults and optionality are taken from
//! `contract/webscraper/openapi.lock.json` (17 schemas). The response structs
//! serialise every optional field as an explicit `null`, which is what the
//! reference serves (see `contract/webscraper/golden/`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

fn default_timeout() -> f64 {
    30.0
}

fn default_viewport_width() -> u32 {
    1920
}

fn default_viewport_height() -> u32 {
    1080
}

fn default_max_depth() -> u32 {
    1
}

fn default_schema_name() -> String {
    "extraction".to_string()
}

fn default_scroll_count() -> u32 {
    10
}

fn default_wait_ms() -> u32 {
    500
}

fn default_wait_timeout_ms() -> u64 {
    15000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScraperType {
    Playwright,
    Scrapy,
    Httpx,
    CurlCffi,
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScrapeStatus {
    #[default]
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    #[default]
    Form,
    StorageState,
    Bearer,
    Basic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionFieldType {
    #[default]
    Text,
    Attribute,
    Html,
    Nested,
    List,
    NestedList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transform {
    Lowercase,
    Uppercase,
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitUntil {
    #[serde(rename = "load")]
    Load,
    #[serde(rename = "domcontentloaded")]
    DomContentLoaded,
    #[serde(rename = "networkidle")]
    NetworkIdle,
    #[serde(rename = "commit")]
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualScrollConfig {
    #[serde(default)]
    pub container_selector: Option<String>,
    #[serde(default = "default_scroll_count")]
    pub scroll_count: u32,
    #[serde(default)]
    pub scroll_by: u32,
    #[serde(default = "default_wait_ms")]
    pub wait_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub credential_id: String,
    #[serde(default)]
    pub login_url: Option<String>,
    #[serde(default)]
    pub username_selector: Option<String>,
    #[serde(default)]
    pub password_selector: Option<String>,
    #[serde(default)]
    pub submit_selector: Option<String>,
    #[serde(default)]
    pub wait_for_selector: Option<String>,
    #[serde(default)]
    pub wait_for_url: Option<String>,
    #[serde(rename = "type", default)]
    pub r#type: AuthType,
    #[serde(default = "default_wait_timeout_ms")]
    pub wait_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionField {
    pub name: String,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(rename = "type", default)]
    pub r#type: ExtractionFieldType,
    #[serde(default)]
    pub attribute: Option<String>,
    #[serde(default)]
    pub transform: Option<Transform>,
    #[serde(rename = "default", default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub fields: Vec<ExtractionField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionSchema {
    /// crawler4ai wire compatibility: `baseSelector` is the documented spelling;
    /// `base_selector` is accepted as the alias the reference describes.
    #[serde(rename = "baseSelector", alias = "base_selector")]
    pub base_selector: String,
    #[serde(default = "default_schema_name")]
    pub name: String,
    pub fields: Vec<ExtractionField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeRequest {
    pub url: String,
    #[serde(default)]
    pub scraper_type: ScraperType,
    #[serde(default)]
    pub javascript_enabled: bool,
    #[serde(default = "default_timeout")]
    pub timeout: f64,
    #[serde(default)]
    pub wait_for_selector: Option<String>,
    #[serde(default)]
    pub wait_until: Option<WaitUntil>,
    #[serde(default)]
    pub screenshot: bool,
    #[serde(default = "default_viewport_width")]
    pub viewport_width: u32,
    #[serde(default = "default_viewport_height")]
    pub viewport_height: u32,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub cookies: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub link_query: Option<String>,
    #[serde(default)]
    pub sort_links_by_score: bool,
    #[serde(default)]
    pub stealth: bool,
    #[serde(default)]
    pub delay: f64,
    #[serde(default)]
    pub download_images: bool,
    #[serde(default)]
    pub extraction_schema: Option<ExtractionSchema>,
    #[serde(default)]
    pub no_cache: bool,
    #[serde(default)]
    pub virtual_scroll: Option<VirtualScrollConfig>,
    #[serde(default)]
    pub auth_config: Option<AuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkScrapeRequest {
    pub urls: Vec<String>,
    #[serde(default)]
    pub scraper_type: ScraperType,
    #[serde(default)]
    pub javascript_enabled: bool,
    #[serde(default = "default_timeout")]
    pub timeout: f64,
    #[serde(default)]
    pub follow_links: bool,
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    #[serde(default)]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(default)]
    pub link_query: Option<String>,
    #[serde(default)]
    pub sort_links_by_score: bool,
    #[serde(default)]
    pub delay: f64,
    #[serde(default)]
    pub download_images: bool,
    #[serde(default)]
    pub extraction_schema: Option<ExtractionSchema>,
    #[serde(default)]
    pub screenshot: bool,
    #[serde(default)]
    pub stealth: bool,
    #[serde(default)]
    pub virtual_scroll: Option<VirtualScrollConfig>,
    #[serde(default)]
    pub wait_for_selector: Option<String>,
    #[serde(default)]
    pub wait_until: Option<WaitUntil>,
    #[serde(default)]
    pub auth_config: Option<AuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkInfo {
    pub text: String,
    pub url: String,
    pub rel: Option<String>,
    #[serde(rename = "type")]
    pub link_type: Option<String>,
    #[serde(default)]
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub src: String,
    #[serde(default)]
    pub alt: String,
    #[serde(default)]
    pub title: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub srcset: Option<String>,
    pub content_base64: Option<String>,
    pub mime_type: Option<String>,
    pub byte_size: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeResponse {
    pub url: String,
    pub final_url: Option<String>,
    pub html_content: Option<String>,
    pub text_content: Option<String>,
    pub markdown_content: Option<String>,
    pub title: Option<String>,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub structured_data: BTreeMap<String, serde_json::Value>,
    pub extracted_data: Option<Vec<BTreeMap<String, serde_json::Value>>>,
    pub links: Vec<LinkInfo>,
    pub images: Vec<ImageInfo>,
    pub screenshot_base64: Option<String>,
    pub status: ScrapeStatus,
    pub error: Option<String>,
    pub scraper_used: String,
    pub processing_time_seconds: f64,
    pub storage_state: Option<BTreeMap<String, serde_json::Value>>,
}

impl ScrapeResponse {
    /// The shape every failed scrape has: only the fields the reference fills.
    pub fn failed(url: &str, error: String, processing_time_seconds: f64) -> Self {
        Self {
            url: url.to_string(),
            final_url: None,
            html_content: None,
            text_content: None,
            markdown_content: None,
            title: None,
            metadata: BTreeMap::new(),
            structured_data: BTreeMap::new(),
            extracted_data: None,
            links: Vec::new(),
            images: Vec::new(),
            screenshot_base64: None,
            status: ScrapeStatus::Failed,
            error: Some(error),
            scraper_used: "HttpxScraper".to_string(),
            processing_time_seconds,
            storage_state: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkScrapeResponse {
    pub total_urls: usize,
    pub completed: usize,
    pub failed: usize,
    pub results: Vec<ScrapeResponse>,
    pub processing_time_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
    #[serde(default = "default_service_name")]
    pub service: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub playwright_available: bool,
    #[serde(default)]
    pub browser_ready: bool,
}

fn default_service_name() -> String {
    "webscraper-service".to_string()
}

fn default_version() -> String {
    "1.0.0".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    pub status: u16,
    pub request_id: Option<String>,
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}
