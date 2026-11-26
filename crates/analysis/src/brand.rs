//! `analyse.brand` — deterministic brand extraction over supplied evidence, and the comparison of
//! two brand snapshots.
//!
//! The source's brand pipeline fetched a site and ran DOM/stage analysis before this module. The
//! library has no transport: a caller supplies the page facts it captured, and this module runs
//! the deterministic half — name/domain/logo/colour extraction and the field/colour/font
//! comparison. No clock, no network, no storage.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::{Category, Document, Finding, Report, Scorecard, Severity};
use crate::rubric::{Rubric, grade};

/// One font the page declared.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontInfo {
    pub family: String,
}

/// A brand identity as captured. Field names and defaults are the source's `BrandResponse`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrandSnapshot {
    pub name: String,
    pub domain: String,
    pub logo_url: String,
    pub primary_color: String,
    pub secondary_color: String,
    pub has_logo: bool,
    pub tagline: String,
    pub brand_voice: String,
    pub fonts: Vec<FontInfo>,
}

impl Default for BrandSnapshot {
    fn default() -> Self {
        // The source's schema defaults: `#2563EB` / `#1E40AF`, empty strings elsewhere.
        Self {
            name: String::new(),
            domain: String::new(),
            logo_url: String::new(),
            primary_color: "#2563EB".to_owned(),
            secondary_color: "#1E40AF".to_owned(),
            has_logo: false,
            tagline: String::new(),
            brand_voice: String::new(),
            fonts: Vec::new(),
        }
    }
}

/// One row of the comparison: the field, both values, and whether they matched.
#[derive(Clone, Debug, Serialize)]
pub struct FieldComparison {
    pub field: String,
    pub value_a: String,
    pub value_b: String,
    pub matched: bool,
}

/// Two brands compared. `overall_similarity` is the source's weighted blend (field matches 60%,
/// colour 20%, font Jaccard 20%); `color_distance` is the Euclidean RGB distance.
#[derive(Clone, Debug, Serialize)]
pub struct Comparison {
    pub domain_a: String,
    pub domain_b: String,
    pub overall_similarity: f64,
    pub field_comparisons: Vec<FieldComparison>,
    pub color_distance: f64,
}

impl Comparison {
    /// The attributes whose values differ — the rows the source marked unmatched for a reason
    /// other than both sides being absent. An attribute empty on both brands is not a change.
    pub fn changed_fields(&self) -> Vec<&str> {
        self.field_comparisons
            .iter()
            .filter(|comparison| comparison.value_a != comparison.value_b)
            .map(|comparison| comparison.field.as_str())
            .collect()
    }
}

/// Compare two snapshots exactly as the source's `BrandComparator.compare` did.
pub fn compare(brand_a: &BrandSnapshot, brand_b: &BrandSnapshot) -> Comparison {
    let mut field_comparisons = Vec::new();
    let mut match_count = 0_usize;
    let mut total_fields = 0_usize;

    for field in [
        "name",
        "primary_color",
        "secondary_color",
        "tagline",
        "brand_voice",
    ] {
        let value_a = value_of(brand_a, field);
        let value_b = value_of(brand_b, field);
        let matched = value_a == value_b && !value_a.is_empty();
        field_comparisons.push(FieldComparison {
            field: field.to_owned(),
            value_a: value_a.clone(),
            value_b: value_b.clone(),
            matched,
        });
        if !value_a.is_empty() || !value_b.is_empty() {
            total_fields += 1;
            if matched {
                match_count += 1;
            }
        }
    }

    field_comparisons.push(FieldComparison {
        field: "has_logo".to_owned(),
        value_a: brand_a.has_logo.to_string(),
        value_b: brand_b.has_logo.to_string(),
        matched: brand_a.has_logo == brand_b.has_logo,
    });
    total_fields += 1;
    if brand_a.has_logo == brand_b.has_logo {
        match_count += 1;
    }

    let fonts_a: BTreeSet<String> = brand_a
        .fonts
        .iter()
        .filter(|font| !font.family.is_empty())
        .map(|font| font.family.to_lowercase())
        .collect();
    let fonts_b: BTreeSet<String> = brand_b
        .fonts
        .iter()
        .filter(|font| !font.family.is_empty())
        .map(|font| font.family.to_lowercase())
        .collect();
    let overlap = fonts_a.intersection(&fonts_b).count();
    let union = fonts_a.union(&fonts_b).count();
    let font_similarity = if union == 0 {
        0.0
    } else {
        overlap as f64 / union as f64
    };
    field_comparisons.push(FieldComparison {
        field: "fonts".to_owned(),
        value_a: fonts_a.into_iter().collect::<Vec<_>>().join(", "),
        value_b: fonts_b.into_iter().collect::<Vec<_>>().join(", "),
        matched: font_similarity > 0.5,
    });
    total_fields += 1;
    if font_similarity > 0.5 {
        match_count += 1;
    }

    let color_distance = color_distance(&brand_a.primary_color, &brand_b.primary_color);
    let field_ratio = if total_fields == 0 {
        0.0
    } else {
        match_count as f64 / total_fields as f64
    };
    let color_similarity = (1.0 - (color_distance / 441.67)).max(0.0);
    let overall = field_ratio * 0.6 + color_similarity * 0.2 + font_similarity * 0.2;

    Comparison {
        domain_a: brand_a.domain.clone(),
        domain_b: brand_b.domain.clone(),
        overall_similarity: round_4dp(overall),
        field_comparisons,
        color_distance: round_4dp(color_distance),
    }
}

fn value_of(brand: &BrandSnapshot, field: &str) -> String {
    match field {
        "name" => brand.name.clone(),
        "primary_color" => brand.primary_color.clone(),
        "secondary_color" => brand.secondary_color.clone(),
        "tagline" => brand.tagline.clone(),
        "brand_voice" => brand.brand_voice.clone(),
        _ => String::new(),
    }
}

/// Euclidean RGB distance; an unparseable colour is `(0, 0, 0)`, as the source's `except` was.
fn color_distance(hex_a: &str, hex_b: &str) -> f64 {
    let (r1, g1, b1) = hex_to_rgb(hex_a).unwrap_or((0, 0, 0));
    let (r2, g2, b2) = hex_to_rgb(hex_b).unwrap_or((0, 0, 0));
    let dr = f64::from(r1) - f64::from(r2);
    let dg = f64::from(g1) - f64::from(g2);
    let db = f64::from(b1) - f64::from(b2);
    (dr * dr + dg * dg + db * db).sqrt()
}

/// `hex_to_rgb`: 3/4/6/8 hex digits, alpha dropped, everything else `None` (the source's
/// `ValueError`).
fn hex_to_rgb(hex_color: &str) -> Option<(u8, u8, u8)> {
    if hex_color.is_empty() {
        return None;
    }
    let cleaned = hex_color.strip_prefix('#').unwrap_or(hex_color);
    let expanded = match cleaned.len() {
        3 => cleaned.chars().flat_map(|c| [c, c]).collect::<String>(),
        4 => cleaned[..3]
            .chars()
            .flat_map(|c| [c, c])
            .collect::<String>(),
        6 => cleaned.to_owned(),
        8 => cleaned[..6].to_owned(),
        _ => return None,
    };
    if expanded.len() != 6 || !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let red = u8::from_str_radix(&expanded[0..2], 16).ok()?;
    let green = u8::from_str_radix(&expanded[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&expanded[4..6], 16).ok()?;
    Some((red, green, blue))
}

/// Python's `round(x, 4)`: a correctly-rounded four-decimal rendering of the binary value.
fn round_4dp(value: f64) -> f64 {
    format!("{value:.4}")
        .parse()
        .expect("a four-decimal rendering of a finite f64 parses back")
}

// ---------------------------------------------------------------------------------------------
// Extraction over supplied evidence
// ---------------------------------------------------------------------------------------------

/// One captured page the extraction reads.
#[derive(Clone, Debug, Default)]
pub struct PageFacts {
    pub url: String,
    pub title: String,
    /// Logo candidates the page analyser found, in its order.
    pub logo_candidates: Vec<String>,
    /// The page's `og:image`, when it had one.
    pub og_image: Option<String>,
}

/// Site-level facts the analysis produced.
#[derive(Clone, Debug, Default)]
pub struct SiteFacts {
    pub description: String,
    /// The candidate primary colour, if the site analysis produced one.
    pub primary_color: Option<String>,
}

/// What the source's `BrandExtractor.extract_brand` returned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrandExtraction {
    pub name: String,
    pub domain: String,
    pub logo_url: String,
    pub primary_color: String,
    pub secondary_color: String,
}

/// Extract the deterministic brand fields from captured pages and site analysis.
pub fn extract(pages: &[PageFacts], site: Option<&SiteFacts>) -> BrandExtraction {
    let domain = pages
        .first()
        .map(|page| {
            url::Url::parse(&page.url)
                .ok()
                .map(|parsed| {
                    parsed
                        .host_str()
                        .unwrap_or_default()
                        .trim_start_matches("www.")
                        .to_owned()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let name = extract_name(pages, site)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| domain.clone());
    let logo_url = pages
        .iter()
        .find_map(|page| {
            page.logo_candidates
                .first()
                .cloned()
                .or_else(|| page.og_image.clone().filter(|image| !image.is_empty()))
        })
        .unwrap_or_default();
    let (primary_color, secondary_color) = extract_colors(site);
    BrandExtraction {
        name,
        domain,
        logo_url,
        primary_color,
        secondary_color,
    }
}

fn extract_name(pages: &[PageFacts], site: Option<&SiteFacts>) -> Option<String> {
    if let Some(description) = site
        .filter(|site| !site.description.is_empty() && site.description.chars().count() < 60)
        .map(|site| site.description.trim().to_owned())
    {
        return Some(description);
    }
    pages.first().map(|page| {
        let title = page.title.clone();
        let split = regex::Regex::new(r"(?i)\s*[|\-—–]\s*(?:Home|Welcome|Homepage).*$")
            .expect("the homepage-suffix pattern is valid");
        split
            .split(&title)
            .next()
            .unwrap_or(&title)
            .trim()
            .to_owned()
    })
}

fn extract_colors(site: Option<&SiteFacts>) -> (String, String) {
    let Some(site) = site else {
        return (String::new(), String::new());
    };
    let Some(candidate) = site.primary_color.as_deref() else {
        return (String::new(), String::new());
    };
    let color = candidate.trim();
    if !is_hex_color(color) {
        return (String::new(), String::new());
    }
    (color.to_owned(), darken_hex(color))
}

fn is_hex_color(color: &str) -> bool {
    let Some(digits) = color.strip_prefix('#') else {
        return false;
    };
    matches!(digits.len(), 3..=8) && digits.chars().all(|c| c.is_ascii_hexdigit())
}

/// Darken a hex colour by 20% for the secondary colour, in the source's integer arithmetic.
fn darken_hex(hex_color: &str) -> String {
    let cleaned = hex_color.strip_prefix('#').unwrap_or(hex_color);
    let expanded = if cleaned.len() == 3 {
        cleaned.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        cleaned.to_owned()
    };
    let Ok(red) = u8::from_str_radix(&expanded[0..2], 16) else {
        return cleaned.to_owned();
    };
    let Ok(green) = u8::from_str_radix(&expanded[2..4], 16) else {
        return cleaned.to_owned();
    };
    let Ok(blue) = u8::from_str_radix(&expanded[4..6], 16) else {
        return cleaned.to_owned();
    };
    format!(
        "#{:02x}{:02x}{:02x}",
        (f64::from(red) * 0.8) as u8,
        (f64::from(green) * 0.8) as u8,
        (f64::from(blue) * 0.8) as u8
    )
}

/// A brand's declared voice: the terms the copy must use and the terms it must not.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrandVoice {
    #[serde(default)]
    pub required_terms: Vec<String>,
    #[serde(default)]
    pub forbidden_terms: Vec<String>,
}

/// The brand facts a caller asserts, checked against one page.
///
/// An empty list means "this dimension is not declared", so nothing in it is checked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrandFacts {
    /// Hexadecimal colours the brand owns.
    #[serde(default)]
    pub palette: Vec<String>,
    /// Font families the brand owns.
    #[serde(default)]
    pub typography: Vec<String>,
    /// The vocabulary the brand's copy must use or avoid.
    #[serde(default)]
    pub voice: BrandVoice,
}

/// Measure one page against the supplied brand facts.
///
/// The page's colours are every hex literal in its markup and styles, its families are the
/// `font-family` declarations, and its voice is the visible text. Each declared fact the page does
/// not express is a finding; observed facts the brand does not declare are named once in
/// aggregate. A dimension the caller leaves empty is not checked.
pub fn analyse(document: &Document, brand: &BrandFacts) -> Report {
    let mut findings = Vec::new();

    let observed_colours = observed_colours(&document.html);
    let mut declared_colours = BTreeSet::new();
    for colour in &brand.palette {
        match normalise_colour(colour) {
            Some(colour) => {
                declared_colours.insert(colour.clone());
                if !observed_colours.contains(&colour) {
                    findings.push(brand_finding(
                        "brand-palette-missing",
                        Severity::Medium,
                        format!("the declared colour {colour} is not on the page"),
                        "A colour the brand declares does not occur in the page's markup or styles.",
                        colour,
                    ));
                }
            }
            None => findings.push(brand_finding(
                "brand-palette-missing",
                Severity::Medium,
                format!("the declared colour {colour:?} is not a hexadecimal colour"),
                "A palette entry is not a hexadecimal colour and cannot be observed on a page.",
                colour.clone(),
            )),
        }
    }
    if !brand.palette.is_empty() {
        let undeclared: Vec<String> = observed_colours
            .difference(&declared_colours)
            .cloned()
            .collect();
        if !undeclared.is_empty() {
            findings.push(brand_finding(
                "brand-palette-undeclared",
                Severity::Info,
                "colours outside the declared palette",
                "The page uses colours the brand palette does not declare.",
                undeclared.join(", "),
            ));
        }
    }

    let observed_fonts = observed_fonts(&document.html);
    let mut declared_fonts = BTreeSet::new();
    for family in &brand.typography {
        let family = family.trim().to_ascii_lowercase();
        if family.is_empty() {
            continue;
        }
        declared_fonts.insert(family.clone());
        if !observed_fonts.contains(&family) {
            findings.push(brand_finding(
                "brand-typography-missing",
                Severity::Medium,
                format!("the declared typeface {family} is not used"),
                "A font family the brand declares is not used by any font-family declaration on the page.",
                family,
            ));
        }
    }
    if !brand.typography.is_empty() {
        let undeclared: Vec<String> = observed_fonts
            .difference(&declared_fonts)
            .cloned()
            .collect();
        if !undeclared.is_empty() {
            findings.push(brand_finding(
                "brand-typography-undeclared",
                Severity::Info,
                "typefaces outside the declared set",
                "The page declares families the brand typography does not.",
                undeclared.join(", "),
            ));
        }
    }

    let text = visible_text(&document.html);
    for term in &brand.voice.required_terms {
        let term = term.trim();
        if term.is_empty() {
            continue;
        }
        if !text.contains(&term.to_ascii_lowercase()) {
            findings.push(brand_finding(
                "brand-voice-term-missing",
                Severity::Medium,
                format!("the required voice term {term:?} is missing"),
                "A term the brand voice requires does not occur in the page's visible text.",
                term.to_owned(),
            ));
        }
    }
    for term in &brand.voice.forbidden_terms {
        let term = term.trim();
        if term.is_empty() {
            continue;
        }
        if text.contains(&term.to_ascii_lowercase()) {
            findings.push(brand_finding(
                "brand-voice-term-forbidden",
                Severity::High,
                format!("the forbidden voice term {term:?} occurs"),
                "A term the brand voice forbids occurs in the page's visible text.",
                term.to_owned(),
            ));
        }
    }

    // No dedupe: each missing palette entry, unused family or voice term is a distinct item, and
    // the rubric's dedupe keys on `check_id`, so it would erase all but one of each.
    let scorecard = brand_scorecard(&findings);
    Report {
        url: document.url.clone(),
        findings,
        scorecard,
    }
}

fn brand_finding(
    check_id: &str,
    severity: Severity,
    title: impl Into<String>,
    description: impl Into<String>,
    evidence: impl Into<String>,
) -> Finding {
    Finding {
        check_id: Some(check_id.to_owned()),
        category: Category::Brand,
        severity,
        title: title.into(),
        description: description.into(),
        evidence: Some(evidence.into()),
        source_tool: Some("brand_scan".to_owned()),
        scanner_count: 1,
        ..Finding::default()
    }
}

fn normalise_colour(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    let hex = value.strip_prefix('#')?;
    if !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        3 => {
            let expanded: String = hex
                .chars()
                .flat_map(|character| [character, character])
                .collect();
            Some(format!("#{expanded}"))
        }
        6 => Some(format!("#{hex}")),
        _ => None,
    }
}

fn observed_colours(html: &str) -> BTreeSet<String> {
    let pattern = regex::Regex::new("#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})\\b")
        .expect("the colour pattern is valid");
    pattern
        .find_iter(html)
        .filter_map(|found| normalise_colour(found.as_str()))
        .collect()
}

fn observed_fonts(html: &str) -> BTreeSet<String> {
    let pattern = regex::Regex::new("font-family\\s*:\\s*([^;}\"]+)")
        .expect("the font-family pattern is valid");
    let mut families = BTreeSet::new();
    for captures in pattern.captures_iter(html) {
        let Some(value) = captures.get(1) else {
            continue;
        };
        for family in value.as_str().split(',') {
            let family = family
                .trim()
                .trim_matches(|character| character == '"' || character == '\'')
                .trim()
                .to_ascii_lowercase();
            if !family.is_empty() {
                families.insert(family);
            }
        }
    }
    families
}

fn visible_text(html: &str) -> String {
    let document = scraper::Html::parse_document(html);
    let text = document.root_element().text().collect::<Vec<_>>().join(" ");
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn brand_scorecard(findings: &[Finding]) -> Scorecard {
    let rubric = Rubric::default();
    let mut buckets = BTreeMap::new();
    let overall_score = if findings.is_empty() {
        100.0
    } else {
        let score = rubric.category_score(findings, Category::Brand);
        buckets.insert(Category::Brand.as_str().to_owned(), score);
        score
    };
    Scorecard {
        overall_score,
        grade: grade(overall_score),
        buckets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font(family: &str) -> FontInfo {
        FontInfo {
            family: family.to_owned(),
        }
    }

    fn snapshot(name: &str, primary: &str, fonts: &[&str]) -> BrandSnapshot {
        BrandSnapshot {
            name: name.to_owned(),
            domain: format!("{name}.test"),
            primary_color: primary.to_owned(),
            has_logo: true,
            fonts: fonts.iter().map(|family| font(family)).collect(),
            ..BrandSnapshot::default()
        }
    }

    #[test]
    fn changed_attributes_are_named_with_both_values() {
        let a = snapshot("acme", "#2563EB", &["Inter"]);
        let b = snapshot("globex", "#FF0000", &["Inter"]);
        let comparison = compare(&a, &b);
        let changed = comparison.changed_fields();
        assert!(changed.contains(&"name"), "name must be named as changed");
        assert!(
            changed.contains(&"primary_color"),
            "the colour must be named as changed"
        );
        let name_row = comparison
            .field_comparisons
            .iter()
            .find(|row| row.field == "name")
            .expect("the name row exists");
        assert_eq!(name_row.value_a, "acme");
        assert_eq!(name_row.value_b, "globex");
        assert!(!name_row.matched);
        // The direction is the pair itself: the value moved from `value_a` to `value_b`.
        assert_ne!(name_row.value_a, name_row.value_b);
    }

    #[test]
    fn identical_snapshots_are_fully_similar() {
        let a = snapshot("acme", "#2563EB", &["Inter"]);
        let comparison = compare(&a, &a);
        assert_eq!(comparison.overall_similarity, 1.0);
        assert_eq!(comparison.color_distance, 0.0);
        assert!(comparison.changed_fields().is_empty());
    }

    #[test]
    fn colour_distance_is_euclidean_rgb() {
        let comparison = compare(
            &snapshot("a", "#000000", &[]),
            &snapshot("b", "#FFFFFF", &[]),
        );
        let white = 255.0_f64 * 3.0_f64.sqrt();
        assert_eq!(comparison.color_distance, round_4dp(white));
    }

    #[test]
    fn an_unparseable_colour_is_black_like_the_source_except_clause() {
        let comparison = compare(
            &snapshot("a", "not-a-colour", &[]),
            &snapshot("b", "#000000", &[]),
        );
        assert_eq!(comparison.color_distance, 0.0);
    }

    #[test]
    fn extraction_uses_the_supplied_facts() {
        let pages = [PageFacts {
            url: "https://www.acme.test/home".to_owned(),
            title: "Acme — Home".to_owned(),
            logo_candidates: vec!["https://www.acme.test/logo.svg".to_owned()],
            og_image: None,
        }];
        let site = SiteFacts {
            description: String::new(),
            primary_color: Some("#336699".to_owned()),
        };
        let brand = extract(&pages, Some(&site));
        assert_eq!(brand.domain, "acme.test");
        assert_eq!(brand.name, "Acme");
        assert_eq!(brand.logo_url, "https://www.acme.test/logo.svg");
        assert_eq!(brand.primary_color, "#336699");
        assert_eq!(brand.secondary_color, darken_hex("#336699"));
    }

    #[test]
    fn a_short_site_description_wins_over_the_title() {
        let pages = [PageFacts {
            url: "https://acme.test/".to_owned(),
            title: "Ignored".to_owned(),
            ..PageFacts::default()
        }];
        let site = SiteFacts {
            description: "Acme research tools".to_owned(),
            primary_color: None,
        };
        assert_eq!(extract(&pages, Some(&site)).name, "Acme research tools");
    }
}
