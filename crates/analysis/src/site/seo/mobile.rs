//! Mobile checks: the viewport tag and font sizes below 12px.

use scraper::Html;

use crate::model::{Finding, Severity};
use crate::site::seo::{all, attr, text_trimmed};

pub fn check_viewport(html: &Html, url: &str) -> Vec<Finding> {
    let present = all(html, "meta")
        .iter()
        .any(|element| attr(element, "name") == Some("viewport"));
    let mut findings = Vec::new();
    if !present {
        findings.push(
            Finding::seo(
                "seo-viewport-missing",
                Severity::Medium,
                "Missing viewport meta tag",
                "No viewport meta tag found. Google uses mobile-first indexing, so pages without \
                 a viewport tag may rank lower.",
            )
            .with_url(url)
            .with_recommendation(
                "Add <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"> to \
                 the page head.",
            )
            .with_selector("meta[name=\"viewport\"]"),
        );
    }
    findings
}

/// Flag `font-size` values below 12px in inline styles and `<style>` blocks.
pub fn check_mobile_font_sizes(html: &Html, url: &str) -> Vec<Finding> {
    static FONT_SIZE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let font_size = FONT_SIZE.get_or_init(|| {
        regex::Regex::new(r"(?i)font-size\s*:\s*(\d+(?:\.\d+)?)\s*px").expect("static pattern")
    });

    let mut small: Vec<String> = Vec::new();

    for element in all(html, "[style]") {
        if let Some(style) = attr(&element, "style") {
            for captures in font_size.captures_iter(style) {
                if let Ok(size) = captures[1].parse::<f64>()
                    && size < 12.0
                {
                    small.push(format!("{size}px (inline)"));
                }
            }
        }
    }

    for element in all(html, "style") {
        let css = text_trimmed(&element);
        for captures in font_size.captures_iter(&css) {
            if let Ok(size) = captures[1].parse::<f64>()
                && size < 12.0
            {
                small.push(format!("{size}px (style block)"));
            }
        }
    }

    let mut findings = Vec::new();
    if !small.is_empty() {
        let count = small.len();
        let sample = small.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
        let extra = if count > 5 {
            format!(" (and {} more)", count - 5)
        } else {
            String::new()
        };
        findings.push(
            Finding::seo(
                "seo-mobile-font-size",
                Severity::Low,
                format!("{count} font-size value(s) below 12px"),
                "Font sizes below 12px are hard to read on mobile devices and may trigger a \
                 mobile usability warning in Google Search Console.",
            )
            .with_url(url)
            .with_recommendation(
                "Use a minimum font-size of 12px (ideally 16px) for body text to ensure \
                 readability on mobile devices.",
            )
            .with_evidence(format!("Sizes: {sample}{extra}")),
        );
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_viewport_is_medium() {
        let document = Html::parse_document("<html><head></head></html>");
        let findings = check_viewport(&document, "");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Medium);
    }

    #[test]
    fn small_fonts_are_counted_across_inline_and_style_blocks() {
        let document = Html::parse_document(
            r#"<div style="font-size: 10px">a</div>
               <style>.x { FONT-SIZE : 9.5px } .y { font-size: 16px }</style>"#,
        );
        let findings = check_mobile_font_sizes(&document, "");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].title, "2 font-size value(s) below 12px");
        assert_eq!(
            findings[0].evidence.as_deref(),
            Some("Sizes: 10px (inline), 9.5px (style block)")
        );
    }
}
