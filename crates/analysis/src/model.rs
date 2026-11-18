//! The finding model and the wire shapes an `analyse.*` operation returns.
//!
//! The field names are the source contract's field names (`category`, `severity`, `check_id`,
//! `title`, `description`, `recommendation`, `selector`, `evidence`, `source_tool`), so a caller
//! that already parses the old payload parses this one. Two fields are deliberately absent:
//! `finding_id` (a `uuid7` the source minted per finding — see the crate docs: identity belongs
//! to the row that persists a finding, and a time-based id would break the determinism
//! contract), and any field derived from the current time.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The document an analysis runs over: a URL and the bytes that were fetched for it.
///
/// The library never fetches. `apps/research` owns the static fetch and `apps/media` owns the
/// browser capture; both hand a `Document` to this crate. `html` is exactly what the caller
/// captured — the scan does not re-request a missing `robots.txt` or `sitemap.xml` either.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Document {
    /// The URL the bytes came from. Used by the checks that read URL structure, and recorded on
    /// every finding the scan produces.
    pub url: String,
    /// The page's HTML.
    pub html: String,
}

impl Document {
    pub fn new(url: impl Into<String>, html: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            html: html.into(),
        }
    }
}

/// A check's severity, ordered so the highest severity sorts first.
///
/// The numeric rank is the source's `_SEVERITY_RANK`: `critical 0 … info 4`. Dedupe keeps the
/// finding with the **lowest** rank, which is the reason the ordering is descending by severity
/// rather than alphabetically.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    #[default]
    Info,
}

impl Severity {
    /// The source's rank: lower is more severe. The penalty a rank is worth lives in
    /// [`crate::rubric::SeverityWeights`] — one table, so an overridden weight cannot disagree
    /// with a dedupe decision.
    pub fn rank(self) -> u8 {
        match self {
            Severity::Critical => 0,
            Severity::High => 1,
            Severity::Medium => 2,
            Severity::Low => 3,
            Severity::Info => 4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Critical => "critical",
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
            Severity::Info => "info",
        }
    }
}

/// The category a finding belongs to. A scorecard is computed per category, exactly as the
/// source's `compute_category_score(findings, category)` did.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    #[default]
    Seo,
    Accessibility,
    Security,
    Performance,
    Ux,
    Cro,
    Marketing,
    Brand,
    Content,
    CodeQuality,
    Video,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Seo => "seo",
            Category::Accessibility => "accessibility",
            Category::Security => "security",
            Category::Performance => "performance",
            Category::Ux => "ux",
            Category::Cro => "cro",
            Category::Marketing => "marketing",
            Category::Brand => "brand",
            Category::Content => "content",
            Category::CodeQuality => "code_quality",
            Category::Video => "video",
        }
    }
}

/// One finding. Construct with [`Finding::seo`] and the `with_` setters, or build the struct
/// directly with `..Finding::default()`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    /// Stable identifier of the *check*, not of this occurrence (`seo-missing-title`). The rubric
    /// groups findings by the prefix of this value, so it is load-bearing, not decorative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_id: Option<String>,
    #[serde(default)]
    pub category: Category,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub recommendation: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_tool: Option<String>,
    /// How many scanners confirmed this finding after dedupe. `1` for a single-scanner run.
    #[serde(default = "one")]
    pub scanner_count: usize,
    /// The scanners that confirmed it, sorted. Empty until dedupe runs.
    #[serde(default)]
    pub confirmed_by: Vec<String>,
}

fn one() -> usize {
    1
}

impl Finding {
    /// An SEO finding with the source's defaults (`source_tool = "seo_scanner"`).
    pub fn seo(
        check_id: impl Into<String>,
        severity: Severity,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            check_id: Some(check_id.into()),
            category: Category::Seo,
            severity,
            title: title.into(),
            description: description.into(),
            recommendation: String::new(),
            url: String::new(),
            selector: None,
            evidence: None,
            source_tool: Some("seo_scanner".to_owned()),
            scanner_count: 1,
            confirmed_by: Vec::new(),
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    pub fn with_recommendation(mut self, recommendation: impl Into<String>) -> Self {
        self.recommendation = recommendation.into();
        self
    }

    pub fn with_selector(mut self, selector: impl Into<String>) -> Self {
        self.selector = Some(selector.into());
        self
    }

    pub fn with_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence = Some(evidence.into());
        self
    }

    pub fn check_id(&self) -> &str {
        self.check_id.as_deref().unwrap_or_default()
    }
}

/// A letter grade, mapped from the score by [`rubric::grade`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Grade {
    #[serde(rename = "A+")]
    APlus,
    #[serde(rename = "A")]
    A,
    #[serde(rename = "B")]
    B,
    #[serde(rename = "C")]
    C,
    #[serde(rename = "D")]
    D,
    #[serde(rename = "F")]
    F,
}

impl Grade {
    pub fn as_str(self) -> &'static str {
        match self {
            Grade::APlus => "A+",
            Grade::A => "A",
            Grade::B => "B",
            Grade::C => "C",
            Grade::D => "D",
            Grade::F => "F",
        }
    }
}

/// One scorecard: an overall score, its grade, and the per-bucket breakdown that produced it.
///
/// `buckets` is a `BTreeMap` so serialisation order is stable — a `HashMap` here would make two
/// identical reports differ byte for byte.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Scorecard {
    pub overall_score: f64,
    pub grade: Grade,
    pub buckets: BTreeMap<String, f64>,
}

/// The result of one `analyse.*` operation.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub url: String,
    pub findings: Vec<Finding>,
    pub scorecard: Scorecard,
}
