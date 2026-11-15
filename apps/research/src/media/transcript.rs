//! Transcript lookup, reduced to what the contract pins.
//!
//! The reference dispatches on `item.source`: YouTube through `youtube-transcript-api`, podcast
//! feeds through the enclosure URL, and **everything else** answers
//! `{"transcript": null, "language": null, "source": "none"}`. The capture pins the last branch —
//! its seeded item is a `web_scraper` row — and that branch is fully ported.
//!
//! The two upstream branches are **not** ported: fetching a YouTube transcript means a vendor
//! client the contract never captured, and ADR 0006 §4 retires a capability with no live consumer
//! rather than guessing at it. They answer the shape the reference answers when the fetch yields
//! nothing — a null transcript under the honest source label — and `contract/media/README.md`
//! declares it.

use crate::media::db::items::Row;

/// What the service found.
pub struct Found {
    pub transcript: Option<String>,
    pub language: Option<String>,
    pub source: String,
}

/// The reference's three-way dispatch, with the two upstream branches answering "nothing found".
pub async fn get(item: &Row) -> Found {
    match item.source.as_str() {
        "youtube" => Found {
            transcript: None,
            language: None,
            source: "youtube".to_string(),
        },
        "podcast" | "podcast_rss" => Found {
            transcript: None,
            language: None,
            source: "podcast".to_string(),
        },
        _ => Found {
            transcript: None,
            language: None,
            source: "none".to_string(),
        },
    }
}
