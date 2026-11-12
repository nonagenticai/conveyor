//! Shared, single-owner components. A screen never re-implements another's pill or age.

use chrono::{DateTime, Utc};
use leptos::prelude::*;

use crate::api::ApiError;

#[component]
pub fn StatusPill(status: String) -> impl IntoView {
    let tone = match status.as_str() {
        "succeeded" | "approved" | "active" | "ok" | "released" => "ok",
        "running" | "started" => "run",
        "waiting_approval" | "pending" | "queued" | "input" => "wait",
        "failed" | "rejected" | "expired" | "cancelled" => "bad",
        _ => "muted",
    };
    let label = status.replace('_', " ");
    view! { <span class=format!("pill {tone}")>{label}</span> }
}

#[component]
pub fn ErrorPanel(error: ApiError, on_retry: Callback<()>) -> impl IntoView {
    view! {
        <div class="panel problem" role="alert">
            <h2>"That did not load"</h2>
            <p>{error.to_string()}</p>
            <button class="btn" on:click=move |_| on_retry.run(())>"Try again"</button>
        </div>
    }
}

#[component]
pub fn Empty(message: String) -> impl IntoView {
    view! { <p class="empty">{message}</p> }
}

/// A stat tile. One component, so the measure screens cannot drift apart.
#[component]
pub fn Tile(label: String, value: String, detail: String) -> impl IntoView {
    view! {
        <div class="tile">
            <div class="tk">{label}</div>
            <div class="tv">{value}</div>
            <div class="td">{detail}</div>
        </div>
    }
}

/// A destination whose backend slice has not landed: named, explained, never a fake control.
#[component]
pub fn Planned(title: String, detail: String) -> impl IntoView {
    view! {
        <div class="panel planned-panel">
            <h2>{title}</h2>
            <p class="muted">{detail}</p>
        </div>
    }
}

/// The design's headline cache ratio: `hits ÷ (hits + misses)`. `None` when nothing was
/// cacheable — a write is a first fill, not a miss, and `unknown` is its own bucket.
pub fn hit_rate(hits: i64, misses: i64) -> Option<f64> {
    let cached = hits + misses;
    (cached > 0).then(|| hits as f64 / cached as f64)
}

pub fn fmt_pct(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{}%", (value * 100.0).round() as i64),
        None => "—".to_owned(),
    }
}

/// Thousands separators without a locale dependency.
pub fn fmt_int(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, byte) in digits.bytes().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(byte as char);
    }
    if value < 0 { format!("-{out}") } else { out }
}

/// `1.2k` / `4.2M` — the compact form the tiles and tables use.
pub fn fmt_compact(value: i64) -> String {
    let magnitude = value.unsigned_abs();
    if magnitude < 1_000 {
        return value.to_string();
    }
    let (scaled, unit) = if magnitude < 1_000_000 {
        (value as f64 / 1_000.0, "k")
    } else if magnitude < 1_000_000_000 {
        (value as f64 / 1_000_000.0, "M")
    } else {
        (value as f64 / 1_000_000_000.0, "G")
    };
    if scaled.abs() >= 100.0 {
        format!("{scaled:.0}{unit}")
    } else {
        format!("{scaled:.1}{unit}")
    }
}

/// A short identifier: eight characters, or `—` when absent.
pub fn short_id(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(8).collect())
        .unwrap_or_else(|| "—".to_owned())
}

/// A compact, human age. Approvals never expire, so age is the only pressure the UI applies.
pub fn age(at: Option<DateTime<Utc>>) -> String {
    let Some(at) = at else {
        return "—".to_owned();
    };
    let seconds = (Utc::now() - at).num_seconds().max(0);
    match seconds {
        0..=59 => "just now".to_owned(),
        60..=3_599 => format!("{} min", seconds / 60),
        3_600..=86_399 => {
            let hours = seconds / 3_600;
            let minutes = (seconds % 3_600) / 60;
            if minutes == 0 {
                format!("{hours} h")
            } else {
                format!("{hours} h {minutes} m")
            }
        }
        _ => {
            let days = seconds / 86_400;
            let hours = (seconds % 86_400) / 3_600;
            if hours == 0 {
                format!("{days} d")
            } else {
                format!("{days} d {hours} h")
            }
        }
    }
}
