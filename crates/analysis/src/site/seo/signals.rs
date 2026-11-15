//! Markup and structured-data signal extractors for the SEO audit groups.
//!
//! Where [`super::content_utils`] reasons over prose, this module reasons over markup: JSON-LD
//! `@type`s, author signals, publish/modify dates and outbound links. It is shared by the
//! E-E-A-T, schema-coverage and geo groups. All reads are non-mutating; no network, no model.
//!
//! Ported from the source service's `signals` at the captured revision.

use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use regex::Regex;
use scraper::Html;
use serde_json::Value;

use super::structural::json_ld_blocks;
use super::{all, attr, host_of, selector};

/// Every JSON-LD object on the page, with `@graph` arrays flattened.
///
/// A block that fails to parse contributes nothing here — the base structured-data check is what
/// reports it.
pub fn ld_objects(html: &Html) -> Vec<Value> {
    let mut objects = Vec::new();
    for block in json_ld_blocks(html) {
        let Ok(value) = block else {
            continue;
        };
        for item in items_of(&value) {
            let Value::Object(map) = item else {
                continue;
            };
            match map.get("@graph") {
                Some(Value::Array(graph)) => {
                    for node in graph {
                        if node.is_object() {
                            objects.push(node.clone());
                        }
                    }
                }
                _ => objects.push(item.clone()),
            }
        }
    }
    objects
}

fn items_of(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    }
}

/// The set of JSON-LD `@type` values present on the page.
pub fn extract_schema_types(html: &Html) -> BTreeSet<String> {
    let mut types = BTreeSet::new();
    for object in ld_objects(html) {
        match object.get("@type") {
            Some(Value::String(name)) => {
                types.insert(name.clone());
            }
            Some(Value::Array(names)) => {
                for name in names {
                    if let Value::String(name) = name {
                        types.insert(name.clone());
                    }
                }
            }
            _ => {}
        }
    }
    types
}

/// Every JSON-LD object whose `@type` matches `type_name`.
pub fn schema_objects_of_type(html: &Html, type_name: &str) -> Vec<Value> {
    ld_objects(html)
        .into_iter()
        .filter(|object| match object.get("@type") {
            Some(Value::String(name)) => name == type_name,
            Some(Value::Array(names)) => names.iter().any(|name| name.as_str() == Some(type_name)),
            _ => false,
        })
        .collect()
}

/// True if the page exposes any author attribution signal.
///
/// Checks: `<meta name="author">`, `rel="author"`, an `author`/`byline` CSS class, or an
/// `author` property in JSON-LD.
pub fn has_author_signal(html: &Html) -> bool {
    if all(html, "meta")
        .iter()
        .any(|meta| attr(meta, "name") == Some("author"))
    {
        return true;
    }
    if html
        .select(&selector(r#"[rel~="author"]"#))
        .next()
        .is_some()
    {
        return true;
    }
    let class_pattern = Regex::new(r"(?i)author|byline").expect("static pattern");
    if all(html, "*")
        .iter()
        .any(|element| attr(element, "class").is_some_and(|class| class_pattern.is_match(class)))
    {
        return true;
    }
    ld_objects(html)
        .iter()
        .any(|object| object.get("author").is_some_and(is_truthy))
}

/// True if authorship is a structured Person/Organization entity, not a bare string.
pub fn has_author_entity(html: &Html) -> bool {
    if !schema_objects_of_type(html, "Person").is_empty() {
        return true;
    }
    ld_objects(html).iter().any(|object| {
        let author = object.get("author");
        match author {
            Some(Value::Object(map)) => is_author_type(map.get("@type")),
            Some(Value::Array(items)) => items.iter().any(|item| match item {
                Value::Object(map) => is_author_type(map.get("@type")),
                _ => false,
            }),
            _ => false,
        }
    })
}

fn is_author_type(value: Option<&Value>) -> bool {
    matches!(
        value.and_then(Value::as_str),
        Some("Person") | Some("Organization")
    )
}

/// A JSON value's truthiness, as Python's `if author:` means it.
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// Best-effort publish/modify date, preferring the most recent one found.
///
/// Sources: `article:modified_time` / `article:published_time` meta, `<time datetime>`, and
/// JSON-LD `dateModified` / `datePublished`. Naive values are treated as UTC, as the source did.
///
/// The accepted formats are the subset the source's `fromisoformat` call needs in practice: an
/// RFC 3339 timestamp, a naive `YYYY-MM-DDTHH:MM:SS[.ffffff]`, a bare `YYYY-MM-DD`, and (as the
/// source's own fallback) the first `YYYY-MM-DD` found inside a longer string.
pub fn extract_content_date(html: &Html) -> Option<DateTime<Utc>> {
    let mut candidates: Vec<String> = Vec::new();
    for property in ["article:modified_time", "article:published_time"] {
        // The source used `find`, not `find_all`: the first matching meta per property only.
        for meta in all(html, "meta") {
            if attr(&meta, "property") == Some(property) {
                if let Some(content) = attr(&meta, "content")
                    && !content.is_empty()
                {
                    candidates.push(content.to_owned());
                }
                break;
            }
        }
    }
    for time in all(html, "time") {
        if let Some(datetime) = attr(&time, "datetime")
            && !datetime.is_empty()
        {
            candidates.push(datetime.to_owned());
        }
    }
    for object in ld_objects(html) {
        for key in ["dateModified", "datePublished"] {
            if let Some(Value::String(value)) = object.get(key) {
                candidates.push(value.clone());
            }
        }
    }

    candidates.iter().filter_map(|raw| parse_date(raw)).max()
}

fn parse_date(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }
    for pattern in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, pattern) {
            return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Some(midnight_utc(date));
    }
    let iso_date = Regex::new(r"\d{4}-\d{2}-\d{2}").expect("static pattern");
    let found = iso_date.find(raw)?;
    let date = NaiveDate::parse_from_str(found.as_str(), "%Y-%m-%d").ok()?;
    Some(midnight_utc(date))
}

fn midnight_utc(date: NaiveDate) -> DateTime<Utc> {
    DateTime::from_naive_utc_and_offset(date.and_hms_opt(0, 0, 0).expect("a valid midnight"), Utc)
}

/// Whole days between `date` and `as_of` (UTC), floored as Python's `timedelta.days` floors.
///
/// `chrono`'s `num_days` truncates toward zero, which differs for a future date; the floor is
/// what the source computed.
pub fn days_since(as_of: DateTime<Utc>, date: DateTime<Utc>) -> i64 {
    (as_of - date).num_seconds().div_euclid(86_400)
}

/// Absolute http(s) links whose host differs from the page's host.
pub fn external_link_hrefs(html: &Html, page_url: &str) -> Vec<String> {
    let page_host = host_of(page_url);
    let mut hrefs = Vec::new();
    for anchor in all(html, "a") {
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        let href = href.trim();
        let Some((scheme, host)) = split_href(href) else {
            continue;
        };
        if (scheme == "http" || scheme == "https")
            && !host.is_empty()
            && (page_host.is_empty() || host != page_host)
        {
            hrefs.push(href.to_owned());
        }
    }
    hrefs
}

/// Count (internal, external) navigable http(s)/relative anchor links.
pub fn internal_and_external_links(html: &Html, page_url: &str) -> (usize, usize) {
    let page_host = host_of(page_url);
    let mut internal = 0usize;
    let mut external = 0usize;
    for anchor in all(html, "a") {
        let Some(href) = attr(&anchor, "href") else {
            continue;
        };
        let href = href.trim();
        if href.is_empty()
            || href.starts_with('#')
            || href.starts_with("mailto:")
            || href.starts_with("tel:")
            || href.starts_with("javascript:")
        {
            continue;
        }
        match split_href(href) {
            Some((scheme, host)) => {
                if !scheme.is_empty() && scheme != "http" && scheme != "https" {
                    continue;
                }
                if !host.is_empty() && !page_host.is_empty() && host != page_host {
                    external += 1;
                } else {
                    internal += 1;
                }
            }
            // A relative href, which `urlparse` reports with an empty scheme and netloc.
            None => internal += 1,
        }
    }
    (internal, external)
}

/// `(scheme, host[:port])` for an absolute or protocol-relative href — the `urlparse` view.
fn split_href(href: &str) -> Option<(String, String)> {
    let absolute = if href.starts_with("//") {
        format!("https:{href}")
    } else {
        href.to_owned()
    };
    let parsed = url::Url::parse(&absolute).ok()?;
    Some((parsed.scheme().to_lowercase(), netloc(&parsed)))
}

fn netloc(parsed: &url::Url) -> String {
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(head: &str, body: &str) -> Html {
        Html::parse_document(&format!(
            "<html><head>{head}</head><body>{body}</body></html>"
        ))
    }

    const URL: &str = "https://example.test/page";

    #[test]
    fn graph_arrays_are_flattened_and_types_collected() {
        let html = doc(
            r#"<script type="application/ld+json">
               {"@graph": [{"@type": "Organization"}, {"@type": ["Article", "NewsArticle"]}]}
               </script>"#,
            "",
        );
        assert_eq!(
            extract_schema_types(&html).into_iter().collect::<Vec<_>>(),
            ["Article", "NewsArticle", "Organization"]
        );
        assert_eq!(schema_objects_of_type(&html, "Organization").len(), 1);
    }

    #[test]
    fn a_broken_block_contributes_no_types() {
        let html = doc(
            r#"<script type="application/ld+json">{not json</script>"#,
            "",
        );
        assert!(extract_schema_types(&html).is_empty());
    }

    #[test]
    fn author_signal_finds_each_documented_form() {
        assert!(has_author_signal(&doc(
            r#"<meta name="author" content="A">"#,
            ""
        )));
        assert!(has_author_signal(&doc(
            "",
            r#"<a rel="author" href="/a">A</a>"#
        )));
        assert!(has_author_signal(&doc(
            "",
            r#"<p class="post-byline">A</p>"#
        )));
        assert!(has_author_signal(&doc(
            r#"<script type="application/ld+json">{"author": "A"}</script>"#,
            ""
        )));
        assert!(!has_author_signal(&doc("", "<p>No attribution</p>")));
    }

    #[test]
    fn author_entity_needs_a_typed_person_or_organization() {
        let plain = doc(
            r#"<script type="application/ld+json">{"author": "A"}</script>"#,
            "",
        );
        assert!(!has_author_entity(&plain));
        let typed = doc(
            r#"<script type="application/ld+json">{"author": {"@type": "Person", "name": "A"}}</script>"#,
            "",
        );
        assert!(has_author_entity(&typed));
        let person_object = doc(
            r#"<script type="application/ld+json">{"@type": "Person", "name": "A"}</script>"#,
            "",
        );
        assert!(has_author_entity(&person_object));
    }

    #[test]
    fn the_most_recent_date_wins() {
        let html = doc(
            r#"<meta property="article:published_time" content="2024-01-01T00:00:00Z">
               <meta property="article:modified_time" content="2024-06-01T00:00:00Z">"#,
            "",
        );
        let date = extract_content_date(&html).expect("a date");
        assert_eq!(date.to_rfc3339(), "2024-06-01T00:00:00+00:00");
    }

    #[test]
    fn a_bare_iso_date_is_accepted_as_utc_midnight() {
        let html = doc(r#"<time datetime="2024-03-04"></time>"#, "");
        let date = extract_content_date(&html).expect("a date");
        assert_eq!(date.to_rfc3339(), "2024-03-04T00:00:00+00:00");
    }

    #[test]
    fn days_since_floors_like_python() {
        let now = DateTime::parse_from_rfc3339("2024-06-01T12:00:00Z")
            .expect("valid")
            .with_timezone(&Utc);
        let future = DateTime::parse_from_rfc3339("2024-06-02T00:00:00Z")
            .expect("valid")
            .with_timezone(&Utc);
        // Python: (now - future).days == -1 (floor), not 0.
        assert_eq!(days_since(now, future), -1);
    }

    #[test]
    fn internal_and_external_links_are_counted() {
        let html = doc(
            "",
            r##"<a href="/a">a</a><a href="https://other.test/b">b</a><a href="#top">t</a>
               <a href="mailto:x@y.z">m</a><a href="//cdn.test/c">c</a>"##,
        );
        assert_eq!(internal_and_external_links(&html, URL), (1, 2));
        assert_eq!(
            external_link_hrefs(&html, URL),
            ["https://other.test/b", "//cdn.test/c"]
        );
    }
}
