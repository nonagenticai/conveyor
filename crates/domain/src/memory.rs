#![forbid(unsafe_code)]
//! S15's work-package memory rules: the two closed vocabularies, the hot-row write contract and
//! the digest a consolidation job sends to `strata_rag`.
//!
//! Hot rows are the record (they live in `work_package_memory` and are never deleted while the
//! card exists); a digest is derived from them. This module owns the pure part — what a valid row
//! is and exactly what the digest body says — so the store and the job cannot drift apart.
//!
//! ⚠️ The `work-package: <card_id>` header is load-bearing. `strata_rag` deduplicates identical
//! normalized text across sources: without a per-card header, two cards with the same templated
//! note collide and the second gets the first's document, which pruning then deletes.

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

/// The fixed first line of every digest body. `strata_rag` dedupe makes this mandatory.
pub const DIGEST_HEADER_PREFIX: &str = "work-package: ";

/// The largest single memory body a caller may write (characters, not bytes).
pub const MAX_BODY_CHARS: usize = 16 * 1024;

/// The largest digest body, in bytes. The upstream document limit is higher; this keeps one
/// consolidation a bounded unit of work and of embedding spend.
pub const MAX_DIGEST_BYTES: usize = 64 * 1024;

/// What a memory row records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryKind {
    /// Something that is true about the work.
    Fact,
    /// Something that was tried (and may have failed).
    Attempt,
    /// A choice, with its reason.
    Decision,
    /// Something that stops progress.
    Blocker,
    /// The derived consolidation row. Only the `memory.consolidate` job writes this kind.
    Digest,
}

impl MemoryKind {
    /// The value the `kind` column CHECK accepts.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Attempt => "attempt",
            Self::Decision => "decision",
            Self::Blocker => "blocker",
            Self::Digest => "digest",
        }
    }

    /// Parse a stored value. `None` is impossible under the column CHECK.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "fact" => Some(Self::Fact),
            "attempt" => Some(Self::Attempt),
            "decision" => Some(Self::Decision),
            "blocker" => Some(Self::Blocker),
            "digest" => Some(Self::Digest),
            _ => None,
        }
    }
}

/// Who wrote a row. An agent's row is auditable as an agent's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemorySource {
    /// A model-driven executor.
    Agent,
    /// A person.
    Human,
    /// The system (a job, the runner, the conveyor itself).
    System,
}

impl MemorySource {
    /// The value the `source` column CHECK accepts.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Human => "human",
            Self::System => "system",
        }
    }

    /// Parse a stored value. `None` is impossible under the column CHECK.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "agent" => Some(Self::Agent),
            "human" => Some(Self::Human),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// Why a memory write or a digest build was refused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryError {
    /// A row with no text is not a memory.
    #[error("the memory body is empty")]
    EmptyBody,
    /// A body over [`MAX_BODY_CHARS`].
    #[error("the memory body is {len} characters; the maximum is {max}")]
    BodyTooLong {
        /// The supplied length.
        len: usize,
        /// The cap.
        max: usize,
    },
    /// A stored `kind` the vocabulary does not know.
    #[error("unknown memory kind {0:?}")]
    UnknownKind(String),
    /// A stored `source` the vocabulary does not know.
    #[error("unknown memory source {0:?}")]
    UnknownSource(String),
    /// A digest is written only by the consolidation job, never by `memory.remember`.
    #[error("the digest kind is derived and cannot be written by an action")]
    DerivedKind,
    /// Consolidation was asked for with nothing to consolidate.
    #[error("there are no un-consolidated memory rows")]
    EmptyDigest,
    /// A single row could not fit the digest cap (impossible while `MAX_BODY_CHARS` is far
    /// below `MAX_DIGEST_BYTES`; kept typed rather than panicking).
    #[error("a memory row is larger than the digest cap")]
    DigestTooLarge,
}

/// Validate a `memory.remember` write. A digest is written only by the job, never by the action.
///
/// A self-supersede needs no check here: the row being written does not exist yet, so the id the
/// caller names can only be an earlier row (the store refuses one from another card).
pub fn validate_remember(
    kind: MemoryKind,
    _source: MemorySource,
    body: &str,
) -> Result<(), MemoryError> {
    if kind == MemoryKind::Digest {
        return Err(MemoryError::DerivedKind);
    }
    let body = body.trim();
    if body.is_empty() {
        return Err(MemoryError::EmptyBody);
    }
    if body.chars().count() > MAX_BODY_CHARS {
        return Err(MemoryError::BodyTooLong {
            len: body.chars().count(),
            max: MAX_BODY_CHARS,
        });
    }
    Ok(())
}

/// One hot row, as the consolidation query returns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryLine {
    /// The row id.
    pub id: Uuid,
    /// What it records.
    pub kind: MemoryKind,
    /// Who wrote it.
    pub source: MemorySource,
    /// The text.
    pub body: String,
    /// When it was written.
    pub created_at: DateTime<Utc>,
}

/// The digest a consolidation job ingests, and the row facts the caller records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Digest {
    /// The exact document body: the header line, then one line per included row.
    pub body: String,
    /// The rows the digest covers, oldest first.
    pub row_ids: Vec<Uuid>,
    /// The newest included row — the caller's dedupe identity.
    pub last_row_id: Uuid,
    /// True when older rows were dropped to stay under [`MAX_DIGEST_BYTES`].
    pub truncated: bool,
    /// Every row the query offered, including dropped ones.
    pub rows_offered: usize,
}

/// Build the digest body for one work package.
///
/// Rows are rendered oldest first. If the body would exceed [`MAX_DIGEST_BYTES`], the **oldest**
/// rows are dropped first: recent memory is what the next run needs. The result is deterministic
/// for the same rows, which is what makes a replay write no second document.
pub fn digest(card_id: Uuid, rows: &[MemoryLine]) -> Result<Digest, MemoryError> {
    if rows.is_empty() {
        return Err(MemoryError::EmptyDigest);
    }

    let mut ordered: Vec<&MemoryLine> = rows.iter().collect();
    ordered.sort_by_key(|row| (row.created_at, row.id));

    let header = format!("{DIGEST_HEADER_PREFIX}{card_id}");
    let mut lines: Vec<String> = Vec::with_capacity(ordered.len());
    lines.push(header.clone());

    // Walk newest first so the truncation drops the oldest.
    let mut kept: Vec<&MemoryLine> = Vec::with_capacity(ordered.len());
    let mut bytes = header.len() + 1;
    for row in ordered.iter().rev() {
        let line = render_line(row);
        let added = line.len() + 1;
        if bytes + added > MAX_DIGEST_BYTES {
            break;
        }
        bytes += added;
        kept.push(row);
    }
    // `header + 1` always fits, and one body is capped far below the digest cap, so this arm is
    // unreachable in practice; it is typed rather than a panic.
    if kept.is_empty() {
        return Err(MemoryError::DigestTooLarge);
    }
    kept.reverse();
    for row in &kept {
        lines.push(render_line(row));
    }

    let truncated = kept.len() < ordered.len();
    let last_row_id = kept.last().expect("kept is non-empty").id;
    let body = lines.join("\n");
    Ok(Digest {
        body,
        row_ids: kept.iter().map(|row| row.id).collect(),
        last_row_id,
        truncated,
        rows_offered: ordered.len(),
    })
}

/// One digest line. Newlines are escaped so a multi-line body stays one record per row.
fn render_line(row: &MemoryLine) -> String {
    let body = row
        .body
        .trim()
        .replace("\r\n", "\\n")
        .replace(['\n', '\r'], "\\n");
    format!("- [{}/{}] {body}", row.kind.as_str(), row.source.as_str())
}

/// The upstream `source_id` every digest for this card uses.
pub fn digest_source_id(card_id: Uuid) -> String {
    format!("conveyor:work:{card_id}")
}

/// The upstream `source_run` a consolidation writes under. The id is the consolidation's own,
/// so `except_run` can keep the new document and prune every older one.
pub fn digest_source_run(consolidation_id: Uuid) -> String {
    consolidation_id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).single().expect("valid")
    }

    fn line(id: u8, kind: MemoryKind, source: MemorySource, body: &str, at_s: i64) -> MemoryLine {
        MemoryLine {
            id: Uuid::from_u128(id as u128),
            kind,
            source,
            body: body.to_owned(),
            created_at: at(at_s),
        }
    }

    #[test]
    fn vocabularies_round_trip_and_reject_unknown_values() {
        for kind in [
            MemoryKind::Fact,
            MemoryKind::Attempt,
            MemoryKind::Decision,
            MemoryKind::Blocker,
            MemoryKind::Digest,
        ] {
            assert_eq!(MemoryKind::parse(kind.as_str()), Some(kind));
        }
        for source in [
            MemorySource::Agent,
            MemorySource::Human,
            MemorySource::System,
        ] {
            assert_eq!(MemorySource::parse(source.as_str()), Some(source));
        }
        assert_eq!(MemoryKind::parse("opinion"), None);
        assert_eq!(MemorySource::parse("robot"), None);
    }

    #[test]
    fn a_remember_write_needs_text_and_refuses_the_derived_kind() {
        assert_eq!(
            validate_remember(MemoryKind::Fact, MemorySource::Agent, "   "),
            Err(MemoryError::EmptyBody)
        );
        assert_eq!(
            validate_remember(MemoryKind::Digest, MemorySource::System, "x"),
            Err(MemoryError::DerivedKind)
        );
        assert!(validate_remember(MemoryKind::Fact, MemorySource::Agent, "ok").is_ok());
    }

    #[test]
    fn the_header_is_the_first_line_and_rows_are_oldest_first() {
        let card = Uuid::from_u128(7);
        let rows = vec![
            line(2, MemoryKind::Decision, MemorySource::Human, "chose B", 200),
            line(1, MemoryKind::Fact, MemorySource::Agent, "found A", 100),
        ];
        let digest = digest(card, &rows).expect("digest");
        assert_eq!(
            digest.body,
            format!("work-package: {card}\n- [fact/agent] found A\n- [decision/human] chose B")
        );
        assert_eq!(digest.row_ids, vec![rows[1].id, rows[0].id]);
        assert_eq!(digest.last_row_id, rows[0].id);
        assert!(!digest.truncated);
        assert_eq!(digest.rows_offered, 2);
    }

    #[test]
    fn a_multiline_body_stays_one_line() {
        let rows = vec![line(
            1,
            MemoryKind::Blocker,
            MemorySource::System,
            "line one\nline two",
            100,
        )];
        let digest = digest(Uuid::from_u128(7), &rows).expect("digest");
        assert!(
            digest
                .body
                .ends_with("- [blocker/system] line one\\nline two")
        );
        assert_eq!(digest.body.lines().count(), 2);
    }

    #[test]
    fn truncation_drops_the_oldest_rows_and_says_so() {
        let card = Uuid::from_u128(7);
        let big = "x".repeat(MAX_DIGEST_BYTES / 2);
        let rows = vec![
            line(1, MemoryKind::Fact, MemorySource::Agent, &big, 100),
            line(2, MemoryKind::Fact, MemorySource::Agent, &big, 200),
            line(3, MemoryKind::Fact, MemorySource::Agent, &big, 300),
        ];
        let digest = digest(card, &rows).expect("digest");
        assert!(digest.truncated);
        assert_eq!(digest.rows_offered, 3);
        // 32 KiB per row: the header plus two rows exceeds the 64 KiB cap, so only the newest
        // row survives.
        assert_eq!(digest.row_ids, vec![rows[2].id]);
        assert!(digest.body.starts_with(&format!("work-package: {card}\n")));
    }

    #[test]
    fn an_empty_set_is_not_a_digest() {
        assert_eq!(
            digest(Uuid::from_u128(7), &[]),
            Err(MemoryError::EmptyDigest)
        );
    }

    #[test]
    fn source_identifiers_are_scoped_to_the_card_and_the_run() {
        let card = Uuid::from_u128(7);
        let consolidation = Uuid::from_u128(9);
        assert_eq!(digest_source_id(card), format!("conveyor:work:{card}"));
        assert_eq!(digest_source_run(consolidation), consolidation.to_string());
    }
}
