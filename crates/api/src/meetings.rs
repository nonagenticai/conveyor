#![forbid(unsafe_code)]
//! `GET /api/meetings/{id}/ics` — the calendar read of a meeting's attendance (T19, #606).
//!
//! The plan deliberately has no calendar dependency: `meeting_attendance` is the record, and
//! this feed is a derived read of it. It is a plain REST endpoint because its answer is
//! `text/calendar`, not a JSON action; it writes nothing and is read-only in the transaction.

use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::{Caller, RequireLicence};
use crate::state::AppState;

/// `GET /api/meetings/{id}/ics`.
pub async fn ics(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
) -> Response {
    let mut tx = match conveyor_store::begin_scoped_read_only(&state.pool, caller.tenant).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "open the meeting ICS read");
            return internal();
        }
    };
    let detail = match conveyor_store::meetings::detail_tx(&mut tx, caller.tenant, id).await {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            let _ = tx.rollback().await;
            return ApiError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                "no meeting with that id",
            )
            .into_response();
        }
        Err(error) => {
            tracing::error!(%error, %id, "read the meeting ICS");
            let _ = tx.rollback().await;
            return internal();
        }
    };
    let _ = tx.rollback().await;

    let meeting = &detail.meeting;
    let starts = meeting.opens_at.unwrap_or_else(Utc::now);
    let ends = meeting
        .decision_window_end
        .unwrap_or_else(|| starts + Duration::seconds(detail.meeting_type.timebox_seconds.max(60)));
    let stamp = |instant: DateTime<Utc>| instant.format("%Y%m%dT%H%M%SZ").to_string();
    let mut calendar = String::new();
    calendar.push_str("BEGIN:VCALENDAR\r\n");
    calendar.push_str("VERSION:2.0\r\n");
    calendar.push_str("PRODID:-//conveyor//meetings//EN\r\n");
    calendar.push_str("CALSCALE:GREGORIAN\r\n");
    calendar.push_str("METHOD:PUBLISH\r\n");
    calendar.push_str("BEGIN:VEVENT\r\n");
    calendar.push_str(&format!("UID:{}@conveyor\r\n", meeting.id));
    calendar.push_str(&format!("DTSTAMP:{}\r\n", stamp(Utc::now())));
    calendar.push_str(&format!("DTSTART:{}\r\n", stamp(starts)));
    calendar.push_str(&format!("DTEND:{}\r\n", stamp(ends)));
    calendar.push_str(&format!("SUMMARY:{}\r\n", ics_escape(&meeting.card_name)));
    calendar.push_str(&format!("STATUS:{}\r\n", ics_status(&meeting.status)));
    for seat in &detail.attendance {
        calendar.push_str(&format!(
            "ATTENDEE;CN=\"{}\";ROLE={}:mailto:{}@conveyor.invalid\r\n",
            ics_escape(&format!(
                "{} {}",
                seat.participant_kind, seat.participant_id
            )),
            ics_role(&seat.seat),
            seat.participant_id
        ));
    }
    calendar.push_str("END:VEVENT\r\n");
    calendar.push_str("END:VCALENDAR\r\n");

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/calendar; charset=utf-8")],
        calendar,
    )
        .into_response()
}

fn internal() -> Response {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "the meeting feed could not be read",
    )
    .into_response()
}

/// RFC 5545 §3.3.11: backslash, semicolon, comma and newline are escaped.
fn ics_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn ics_status(status: &str) -> &'static str {
    match status {
        "cancelled" => "CANCELLED",
        "closed" => "CONFIRMED",
        _ => "TENTATIVE",
    }
}

fn ics_role(seat: &str) -> &'static str {
    match seat {
        "chair" => "CHAIR",
        "observer" | "presenter" => "OPT-PARTICIPANT",
        _ => "REQ-PARTICIPANT",
    }
}
