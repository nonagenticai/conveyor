//! Seven-signal quality scoring, ported from the reference's `services/quality.py`.
//!
//! Every number in `contract/media/golden/{items_get_score,items_score,score_*}.*` comes out of
//! this module, so the arithmetic is reproduced rather than approximated: each component is
//! rounded to four places **before** the weighted mean, and the mean is rounded to four afterwards.
//!
//! `round` is Python's: correct rounding of the exact binary value with ties to even. Rust's
//! `{:.4}` formatting is the same operation, so the port formats rather than multiplying and
//! rounding — `(x * 10_000.0).round() / 10_000.0` rounds halves away from zero and would differ on
//! a value like `0.73335`, which is exactly the shape the captured session average has.

use chrono::{DateTime, Utc};
use serde_json::Number;
use serde_json::Value;

use super::db::items::Row;
use super::models::Components;

/// Signal weights, from the reference's settings defaults.
const FRESHNESS: f64 = 0.15;
const ENGAGEMENT: f64 = 0.20;
const AUTHORITY: f64 = 0.15;
const COMPLETENESS: f64 = 0.15;
const DURATION: f64 = 0.10;
const COMMUNITY: f64 = 0.15;
const SOURCE: f64 = 0.10;

/// Source baselines. Keys are adapter `.name` values — `podcast` and `web_scraper`, never
/// `podcast_rss` or `web`; an unknown source falls through to 0.5 rather than failing.
fn source_reliability(source: &str) -> f64 {
    match source {
        "youtube" => 0.8,
        "vimeo" => 0.85,
        "podcast" => 0.7,
        "wikimedia" => 0.9,
        "web_scraper" => 0.5,
        _ => 0.5,
    }
}

/// Python's `round(x, 4)`, as a value that serialises the way pydantic's does.
fn round4(value: f64) -> f64 {
    format!("{value:.4}").parse().unwrap_or(value)
}

/// Exponential decay from `published_at`, 365-day half-life. Neutral 0.3 when unknown.
fn freshness(published_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> f64 {
    let Some(published_at) = published_at else {
        return 0.3;
    };
    let age_days = ((now - published_at).num_seconds() as f64 / 86_400.0).max(0.0);
    (-0.693 * age_days / 365.0).exp()
}

/// Log-normalised view count against a 10 000-view median.
fn engagement(view_count: Option<i32>) -> f64 {
    let views = view_count.unwrap_or(0);
    if views <= 0 {
        return 0.0;
    }
    ((views as f64 + 1.0).ln() / 10_001.0f64.ln()).min(1.0)
}

/// Half for a named author, half for a channel URL.
fn authority(author_name: Option<&str>, author_channel_url: Option<&str>) -> f64 {
    let mut score = 0.0;
    if author_name.is_some_and(|name| !name.is_empty()) {
        score += 0.5;
    }
    if author_channel_url.is_some_and(|url| !url.is_empty()) {
        score += 0.5;
    }
    score
}

/// Four presence checks: description, tags, thumbnail, a positive duration.
fn completeness(item: &Row) -> f64 {
    let checks = [
        item.description
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
        item.tags.0.as_array().is_some_and(|tags| !tags.is_empty()),
        item.thumbnail_url.is_some(),
        item.duration_seconds.is_some_and(|duration| duration > 0),
    ];
    checks.iter().filter(|present| **present).count() as f64 / checks.len() as f64
}

/// Penalises under a minute and over three hours; the 5–60 minute band scores 1.0.
fn duration(duration_seconds: Option<i32>) -> f64 {
    let Some(seconds) = duration_seconds else {
        return 0.3;
    };
    let seconds = seconds as f64;
    if seconds <= 0.0 {
        return 0.3;
    }
    if seconds < 60.0 {
        return seconds / 60.0 * 0.5;
    }
    if seconds <= 300.0 {
        return 0.5 + (seconds - 60.0) / 240.0 * 0.5;
    }
    if seconds <= 3600.0 {
        return 1.0;
    }
    if seconds <= 10_800.0 {
        return 1.0 - (seconds - 3600.0) / 7200.0 * 0.5;
    }
    0.5
}

/// Like-to-view ratio, capped at a 5 % ratio.
fn community(view_count: Option<i32>, like_count: Option<i32>) -> f64 {
    let views = view_count.unwrap_or(0).max(1) as f64;
    let likes = like_count.unwrap_or(0) as f64;
    (likes / views / 0.05).min(1.0)
}

/// The seven signals, each rounded to four places.
pub fn components(item: &Row, now: DateTime<Utc>) -> Components {
    let mut map = Components::new();
    let signals = [
        ("freshness", freshness(item.published_at, now)),
        ("engagement", engagement(item.view_count)),
        (
            "authority",
            authority(
                item.author_name.as_deref(),
                item.author_channel_url.as_deref(),
            ),
        ),
        ("completeness", completeness(item)),
        ("duration", duration(item.duration_seconds)),
        ("community", community(item.view_count, item.like_count)),
        ("source_reliability", source_reliability(&item.source)),
    ];
    for (name, value) in signals {
        map.insert(
            name.to_string(),
            Value::Number(Number::from_f64(round4(value)).unwrap_or_else(|| Number::from(0))),
        );
    }
    map
}

/// The weighted mean of the components, clamped to `[0, 1]` and rounded to four places.
pub fn score(components: &Components) -> f64 {
    let weights = [
        ("freshness", FRESHNESS),
        ("engagement", ENGAGEMENT),
        ("authority", AUTHORITY),
        ("completeness", COMPLETENESS),
        ("duration", DURATION),
        ("community", COMMUNITY),
        ("source_reliability", SOURCE),
    ];
    let total_weight: f64 = weights.iter().map(|(_, weight)| weight).sum();
    if total_weight == 0.0 {
        return 0.0;
    }
    let mut weighted = 0.0;
    for (name, weight) in weights {
        let value = components.get(name).and_then(Value::as_f64).unwrap_or(0.0);
        weighted += value * weight;
    }
    round4((weighted / total_weight).clamp(0.0, 1.0))
}

/// Score one row. Returns the pair the response carries.
pub fn score_item(item: &Row, now: DateTime<Utc>) -> (f64, Components) {
    let components = components(item, now);
    let score = score(&components);
    (score, components)
}

/// Python's `round(x, 4)` on the session average.
pub fn round_average(value: f64) -> f64 {
    round4(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round4_matches_python_on_the_captured_average() {
        // (0.7646 + 0.7021) / 2 — the captured `score_report.avg_score`.
        let average = (0.7646 + 0.7021) / 2.0;
        assert_eq!(format!("{:.4}", average), "0.7333");
        assert_eq!(round_average(average), 0.7333);
    }

    #[test]
    fn duration_band_edges() {
        assert_eq!(round4(duration(Some(120))), 0.625);
        assert_eq!(duration(Some(600)), 1.0);
        assert_eq!(duration(Some(30)), 0.25);
        assert_eq!(duration(None), 0.3);
    }

    #[test]
    fn engagement_matches_the_captured_value() {
        assert_eq!(round4(engagement(Some(1000))), 0.7501);
        assert_eq!(round4(engagement(Some(500))), 0.675);
    }
}
