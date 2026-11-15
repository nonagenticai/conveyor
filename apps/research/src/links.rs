//! Link discovery and relevance scoring.
//!
//! A port of `services/link_scoring.py` (itself adapted from crawl4ai at
//! `7e80152`, Apache-2.0). The score is intrinsic — anchor-text quality, path
//! depth, query noise, same-domain, `nofollow`, non-HTML target — with an
//! optional caller query blended in at equal weight. Scores are rounded to four
//! decimals, exactly as the reference does, because `links[].score` is part of
//! the captured response.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::extract::text_with_separator;
use crate::models::{ImageInfo, LinkInfo};

const MIN_USEFUL_TEXT: usize = 3;
const MAX_USEFUL_TEXT: usize = 60;

const LOW_VALUE_ANCHORS: [&str; 17] = [
    "click here",
    "here",
    "read more",
    "more",
    "learn more",
    "continue",
    "continue reading",
    "next",
    "previous",
    "prev",
    "back",
    "top",
    "link",
    "this",
    "view",
    "see more",
    "details",
];

const NOISE_QUERY_PARAMS: [&str; 8] = [
    "utm_",
    "fbclid",
    "gclid",
    "msclkid",
    "mc_cid",
    "mc_eid",
    "ref",
    "sessionid",
];

const NON_HTML_EXTENSIONS: [&str; 35] = [
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "zip", "gz", "tar", "rar", "jpg", "jpeg",
    "png", "gif", "svg", "webp", "ico", "bmp", "tiff", "mp3", "mp4", "avi", "mov", "wmv", "webm",
    "wav", "ogg", "css", "js", "json", "xml", "rss", "atom", "exe",
];

const STOPWORDS: [&str; 39] = [
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "has", "have", "how",
    "in", "is", "it", "its", "of", "on", "or", "our", "that", "the", "their", "this", "to", "was",
    "we", "were", "what", "when", "where", "which", "who", "will", "with", "you", "your",
];

const INTRINSIC_WEIGHTS: [(&str, f64); 6] = [
    ("text_quality", 0.30),
    ("path_depth", 0.20),
    ("query_noise", 0.10),
    ("same_domain", 0.20),
    ("followable", 0.10),
    ("html_target", 0.10),
];

/// Extract every `<a href>`, resolve it against `base_url`, classify it by
/// netloc and score it. Document order is preserved unless `sort_by_score`.
pub fn extract_links(
    document: &Html,
    base_url: &str,
    link_query: Option<&str>,
    sort_by_score: bool,
) -> Vec<LinkInfo> {
    let selector = Selector::parse("a[href]").expect("static selector");
    let base_netloc = Url::parse(base_url)
        .ok()
        .map(|url| netloc(&url))
        .unwrap_or_default();
    let query_tokens = tokenize(link_query.unwrap_or_default());

    let mut links: Vec<LinkInfo> = document
        .select(&selector)
        .map(|anchor| {
            let href = anchor.attr("href").unwrap_or_default();
            let url = resolve(base_url, href);
            let link_netloc = Url::parse(&url)
                .ok()
                .map(|url| netloc(&url))
                .unwrap_or_default();
            let link_type = if link_netloc == base_netloc {
                "internal"
            } else {
                "external"
            };
            let rel = anchor.attr("rel").filter(|rel| !rel.is_empty());
            let text = text_with_separator(&anchor, " ");
            let score = score(&url, &text, rel, Some(link_type), &query_tokens);
            LinkInfo {
                text,
                url,
                rel: rel.map(str::to_string),
                link_type: Some(link_type.to_string()),
                score,
            }
        })
        .collect();

    if sort_by_score {
        // `sort_by` is stable, so equal scores keep document order.
        links.sort_by(|a, b| b.score.total_cmp(&a.score));
    }
    links
}

fn resolve(base_url: &str, href: &str) -> String {
    if let Ok(base) = Url::parse(base_url)
        && let Ok(joined) = base.join(href)
    {
        return joined.to_string();
    }
    href.to_string()
}

/// Extract every `<img>`, preferring lazy-load attributes and resolving relative
/// sources against the page URL. Dimensions are read from the HTML attributes,
/// which is what the static backend does (no browser layout).
pub fn extract_images(document: &Html, base_url: &str) -> Vec<ImageInfo> {
    let selector = Selector::parse("img").expect("static selector");
    let mut images = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for image in document.select(&selector) {
        let src = first_attr(
            &image,
            &["data-src", "data-original", "data-lazy-src", "src"],
        );
        let src = absolutise(base_url, &src);
        seen.insert(src.clone());
        images.push(ImageInfo {
            src,
            alt: image.attr("alt").unwrap_or_default().to_string(),
            title: image.attr("title").unwrap_or_default().to_string(),
            width: parse_int(image.attr("width")),
            height: parse_int(image.attr("height")),
            srcset: non_empty(image.attr("srcset")),
            content_base64: None,
            mime_type: None,
            byte_size: None,
        });
    }

    let picture_selector = Selector::parse("picture").expect("static selector");
    let source_selector = Selector::parse("source").expect("static selector");
    for picture in document.select(&picture_selector) {
        let Some(image) = picture.select(&selector).next() else {
            continue;
        };
        let src = match image.attr("data-src").or_else(|| image.attr("src")) {
            Some(src) if !src.is_empty() => absolutise(base_url, src),
            _ => continue,
        };
        let best_srcset = picture
            .select(&source_selector)
            .filter_map(|source| non_empty(source.attr("srcset")))
            .next();
        if seen.contains(&src) {
            if let Some(srcset) = best_srcset
                && let Some(existing) = images.iter_mut().find(|image| image.src == src)
                && existing.srcset.is_none()
            {
                existing.srcset = Some(srcset);
            }
            continue;
        }
        seen.insert(src.clone());
        images.push(ImageInfo {
            src,
            alt: image.attr("alt").unwrap_or_default().to_string(),
            title: image.attr("title").unwrap_or_default().to_string(),
            width: parse_int(image.attr("width")),
            height: parse_int(image.attr("height")),
            srcset: best_srcset,
            content_base64: None,
            mime_type: None,
            byte_size: None,
        });
    }

    images
}

fn first_attr(image: &ElementRef<'_>, names: &[&str]) -> String {
    for name in names {
        if let Some(value) = image.attr(name)
            && !value.is_empty()
        {
            return value.to_string();
        }
    }
    String::new()
}

fn absolutise(base_url: &str, src: &str) -> String {
    if src.is_empty()
        || src.starts_with("http://")
        || src.starts_with("https://")
        || src.starts_with("data:")
    {
        return src.to_string();
    }
    resolve(base_url, src)
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty()).map(str::to_string)
}

fn parse_int(value: Option<&str>) -> Option<i64> {
    value?.trim().parse::<i64>().ok()
}

/// Python's `urlparse(url).netloc`, which is what the reference compares.
fn netloc(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    }
}

fn token_regex() -> &'static Regex {
    static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    TOKEN_RE.get_or_init(|| Regex::new(r"[a-z0-9]+").expect("regex"))
}

/// Lowercase alphanumeric tokens, minus stopwords and one-character noise.
pub fn tokenize(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    token_regex()
        .find_iter(&text.to_lowercase())
        .map(|found| found.as_str().to_string())
        .filter(|token| token.len() > 1 && !STOPWORDS.contains(&token.as_str()))
        .collect()
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.chars()
        .zip(b.chars())
        .take_while(|(left, right)| left == right)
        .count()
}

fn tokens_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let shorter = a.len().min(b.len()) as f64;
    let required = 4_usize.max((0.6 * shorter).round() as usize);
    common_prefix_len(a, b) >= required
}

fn text_quality(text: &str) -> f64 {
    let normalized = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if normalized.is_empty() {
        return 0.0;
    }
    if LOW_VALUE_ANCHORS.contains(&normalized.as_str()) {
        return 0.1;
    }
    if normalized
        .chars()
        .all(|character| character.is_ascii_digit())
    {
        return 0.1;
    }

    let length = normalized.chars().count();
    if length < MIN_USEFUL_TEXT {
        0.2
    } else if length <= MAX_USEFUL_TEXT {
        1.0
    } else {
        (MAX_USEFUL_TEXT as f64 / length as f64).max(0.3)
    }
}

fn path_depth_score(path: &str) -> f64 {
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .count();
    if segments == 0 {
        0.9
    } else {
        (1.0 - 0.15 * (segments as f64 - 1.0)).max(0.1)
    }
}

fn query_noise_score(query: &str) -> f64 {
    if query.is_empty() {
        return 1.0;
    }
    let params = query.split('&').filter(|param| !param.is_empty()).count();
    let lowered = query.to_lowercase();
    let noise = NOISE_QUERY_PARAMS
        .iter()
        .filter(|marker| lowered.contains(*marker))
        .count();
    (1.0 - 0.15 * params as f64 - 0.15 * noise as f64).max(0.1)
}

fn html_target_score(path: &str) -> f64 {
    let tail = path.rsplit_once('/').map(|(_, tail)| tail).unwrap_or(path);
    match tail.rsplit_once('.') {
        Some((_, extension)) => {
            let extension = extension.to_ascii_lowercase();
            if NON_HTML_EXTENSIONS.contains(&extension.as_str()) {
                0.4
            } else {
                1.0
            }
        }
        None => 1.0,
    }
}

fn same_domain_score(link_type: Option<&str>) -> f64 {
    match link_type {
        Some("internal") => 1.0,
        Some("external") => 0.4,
        _ => 0.7,
    }
}

fn contextual_score(query_tokens: &[String], text: &str, path: &str) -> f64 {
    if query_tokens.is_empty() {
        return 0.0;
    }

    let text_tokens = tokenize(text);
    let slug_source = percent_decode(path).replace(['-', '_'], " ").to_string();
    let slug_tokens = tokenize(&slug_source);

    let mut total = 0.0;
    for query in query_tokens {
        if slug_tokens.iter().any(|slug| tokens_match(query, slug)) {
            total += 1.0;
        } else if text_tokens
            .iter()
            .any(|candidate| tokens_match(query, candidate))
        {
            total += 0.8;
        }
    }
    (total / query_tokens.len() as f64).min(1.0)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn score(
    url: &str,
    text: &str,
    rel: Option<&str>,
    link_type: Option<&str>,
    query_tokens: &[String],
) -> f64 {
    if url.is_empty() {
        return 0.0;
    }
    let Ok(parsed) = Url::parse(url) else {
        return 0.0;
    };

    let followable = if rel.is_some_and(|rel| rel.to_lowercase().contains("nofollow")) {
        0.2
    } else {
        1.0
    };
    let signals = [
        text_quality(text),
        path_depth_score(parsed.path()),
        query_noise_score(parsed.query().unwrap_or_default()),
        same_domain_score(link_type),
        followable,
        html_target_score(parsed.path()),
    ];
    let total_weight: f64 = INTRINSIC_WEIGHTS.iter().map(|(_, weight)| weight).sum();
    let intrinsic = signals
        .iter()
        .zip(INTRINSIC_WEIGHTS.iter())
        .map(|(value, (_, weight))| weight * value)
        .sum::<f64>()
        / total_weight;

    let score = if query_tokens.is_empty() {
        intrinsic
    } else {
        let contextual = contextual_score(query_tokens, text, parsed.path());
        0.5 * intrinsic + 0.5 * contextual
    };
    (score * 10_000.0).round() / 10_000.0
}

/// The distinct query tokens, exposed for the parity test's scoring check.
pub fn query_token_set(query: &str) -> BTreeSet<String> {
    tokenize(query).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_golden_article_scores_are_reproduced() {
        assert_eq!(
            score(
                "http://127.0.0.1:18900/",
                "Home",
                None,
                Some("internal"),
                &[]
            ),
            0.98
        );
        assert_eq!(
            score(
                "http://127.0.0.1:18900/about.html",
                "About",
                None,
                Some("internal"),
                &[]
            ),
            1.0
        );
        assert_eq!(
            score(
                "https://example.com/external",
                "public link",
                None,
                Some("external"),
                &[]
            ),
            0.88
        );
    }

    #[test]
    fn query_matching_follows_the_reference_prefix_rule() {
        assert!(tokens_match("price", "pricing"));
        // The reference's rule is a 4-character prefix with 60% coverage of the
        // shorter token; "serv" is exactly 4 and therefore matches "servers".
        assert!(tokens_match("serv", "servers"));
        assert!(!tokens_match("price", "prince"));
        assert_eq!(query_token_set("pricing plans").len(), 2);
    }
}
