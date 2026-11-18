//! #366 — the spreadsheet toolkit: produce a workbook, read one back.
//!
//! `sheets.export` turns a [`conveyor_sheets::Workbook`] — or a plain matrix, through the cell
//! model's scalar form — into an OOXML `.xlsx`, and `sheets.import` reads one back. Both run
//! in-process: a workbook is CPU and memory, not a runtime, so it ships inside the conveyor's
//! own image rather than as a second service (ADR 0007 §1 puts deck/PPTX-style artefacts here,
//! and a spreadsheet is the same shape of work).
//!
//! # Two ways out, one cap each
//!
//! The inline form carries the artefact as base64 beside `bytes` and `sha256`, exactly as
//! `decks.render` does, and is capped at [`MAX_INLINE_BYTES`]. Past that — and for a data table
//! that is ordinary — `as: "document"` writes the bytes through the documents surface's one
//! writer (`create_from_bytes`), which owns object storage, the revision row and the 10 MiB
//! upload contract, and returns the document id instead of the bytes. A second storage path
//! would be a second rule about ordering, hashing and caps.
//!
//! # Refusals
//!
//! Every [`conveyor_sheets::SheetsError`] becomes `invalid_input` on the field that caused it,
//! with the model's own slug as the field code, so `too_large` in a problem document is
//! traceable to `SheetsError::TooLarge`. Nothing is stored before the workbook validates.

use base64::Engine as _;
use conveyor_sheets::{SheetsError, Workbook, XLSX_MEDIA_TYPE};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::documents::actions::{NewBytesDocument, create_from_bytes};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::{FieldError, sha256_hex};

/// The export action's name, as the broker and the registry spell it.
pub const SHEETS_EXPORT: &str = "sheets.export";
/// The import action's name.
pub const SHEETS_IMPORT: &str = "sheets.import";

/// The largest workbook this action will carry inline, in bytes.
///
/// The same figure `decks.render` uses. A matrix that exceeds it is not a bug — it is a file —
/// and the documents route is where files live.
pub const MAX_INLINE_BYTES: usize = 256 * 1024;

/// Where the produced workbook should go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportDestination {
    /// Base64 in the response, capped at [`MAX_INLINE_BYTES`].
    #[default]
    Inline,
    /// Stored as a tenant document; the response carries its id, not its bytes.
    Document,
}

/// The export input: one workbook and where to put it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetsExportIn {
    /// The workbook. A cell is a bare scalar (`"text"`, `12.5`, `true`, `null`) or a detailed
    /// object `{"v": …, "f": "<formula>", "fmt": "date"}`.
    pub workbook: Workbook,
    /// `inline` (default) or `document`.
    #[serde(rename = "as", default)]
    pub destination: ExportDestination,
}

/// The produced workbook: its identity, and its bytes in one of the two forms.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SheetsExportOut {
    /// The artefact's MIME type.
    pub media_type: String,
    /// The artefact's length before base64, in bytes.
    pub bytes: u64,
    /// SHA-256 of the artefact, lowercase hex.
    pub sha256: String,
    /// The number of sheets written.
    pub sheets: u64,
    /// The artefact itself, standard base64 (padded). Present in `inline` mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
    /// The stored document's id. Present in `document` mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
}

/// The import input: the workbook's bytes, and optionally which sheets to keep.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetsImportIn {
    /// The workbook's bytes, standard base64 (padded).
    pub content_base64: String,
    /// Keep only these sheets, in this order. Omit to keep every sheet in workbook order.
    #[serde(default)]
    pub sheets: Option<Vec<String>>,
}

/// The read workbook, in the same model the export takes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SheetsImportOut {
    /// The workbook. Dates are ISO-8601 strings carrying `fmt: "date"`; formulas carry their
    /// text in `f` and their cached result in `v`.
    pub workbook: Workbook,
    /// How many sheets were returned.
    pub sheets: u64,
}

/// Produce a workbook and carry it inline.
#[crate::action(
    name = "sheets.export",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn sheets_export(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SheetsExportIn,
) -> Result<SheetsExportOut, ActionError> {
    match input.destination {
        ExportDestination::Inline => export_inline(&input.workbook),
        ExportDestination::Document => export_document(tx, &ctx, &input.workbook).await,
    }
}

/// Read a workbook's bytes.
#[crate::action(
    name = "sheets.import",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn sheets_import(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: SheetsImportIn,
) -> Result<SheetsImportOut, ActionError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input.content_base64.trim())
        .map_err(|error| {
            invalid(
                "content_base64",
                "invalid_base64",
                format!("the bytes are not standard base64: {error}"),
            )
        })?;
    // The model carries a size limit of its own; this is the same 10 MiB a document upload
    // would be refused at, so a caller cannot route a larger file in through this action.
    let workbook =
        conveyor_sheets::from_xlsx(&bytes).map_err(|error| sheet_error("content_base64", error))?;
    let workbook = select_sheets(workbook, input.sheets)?;
    let sheets = workbook.sheets.len() as u64;
    Ok(SheetsImportOut { workbook, sheets })
}

/// The inline body, split out so a unit test can exercise it without a database.
fn export_inline(workbook: &Workbook) -> Result<SheetsExportOut, ActionError> {
    let bytes =
        conveyor_sheets::to_xlsx(workbook).map_err(|error| sheet_error("workbook", error))?;
    if bytes.len() > MAX_INLINE_BYTES {
        return Err(invalid(
            "workbook",
            "too_large",
            format!(
                "the workbook is {} bytes; the inline cap is {MAX_INLINE_BYTES} — send \
                 `\"as\": \"document\"` to store it instead",
                bytes.len()
            ),
        ));
    }
    Ok(SheetsExportOut {
        media_type: XLSX_MEDIA_TYPE.to_owned(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        sheets: workbook.sheets.len() as u64,
        base64: Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
        document_id: None,
    })
}

/// Store the workbook through the documents surface's one bytes writer.
async fn export_document(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    workbook: &Workbook,
) -> Result<SheetsExportOut, ActionError> {
    let bytes =
        conveyor_sheets::to_xlsx(workbook).map_err(|error| sheet_error("workbook", error))?;
    let sha256 = sha256_hex(&bytes);
    let length = bytes.len() as u64;
    let title = format!(
        "{}.xlsx",
        workbook
            .sheets
            .first()
            .map(|sheet| sheet.name.as_str())
            .unwrap_or("workbook")
    );
    let (document, _size) = create_from_bytes(
        tx,
        ctx,
        NewBytesDocument {
            action: SHEETS_EXPORT,
            title: &title,
            content_type: XLSX_MEDIA_TYPE,
            kind: "export",
            project_id: None,
            parent_document_id: None,
            category: None,
            unit: None,
            meta: None,
        },
        bytes,
    )
    .await?;
    Ok(SheetsExportOut {
        media_type: XLSX_MEDIA_TYPE.to_owned(),
        bytes: length,
        sha256,
        sheets: workbook.sheets.len() as u64,
        base64: None,
        document_id: Some(document.id),
    })
}

/// Keep the requested sheets, in the requested order.
fn select_sheets(workbook: Workbook, names: Option<Vec<String>>) -> Result<Workbook, ActionError> {
    let Some(names) = names else {
        return Ok(workbook);
    };
    if names.is_empty() {
        return Err(invalid(
            "sheets",
            "empty_selection",
            "`sheets` was an empty list; omit it to keep every sheet",
        ));
    }
    let mut selected = Vec::with_capacity(names.len());
    for name in &names {
        let sheet = workbook
            .sheets
            .iter()
            .find(|sheet| &sheet.name == name)
            .ok_or_else(|| {
                invalid(
                    "sheets",
                    "unknown_sheet",
                    format!(
                        "the workbook has no sheet `{name}`; it has {}",
                        workbook
                            .sheets
                            .iter()
                            .map(|sheet| format!("`{}`", sheet.name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            })?;
        selected.push(sheet.clone());
    }
    Ok(Workbook { sheets: selected })
}

/// A typed refusal on one input field.
fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// Map the sheet model's refusal onto the action's field vocabulary.
fn sheet_error(field: &str, error: SheetsError) -> ActionError {
    invalid(field, error.code(), error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use conveyor_sheets::{Cell, CellValue, Sheet};
    use sha2::{Digest, Sha256};

    fn workbook() -> Workbook {
        Workbook {
            sheets: vec![Sheet {
                name: "Report".to_string(),
                rows: vec![
                    vec![
                        Cell::new(CellValue::Text("region".into())),
                        Cell::new(CellValue::Text("total".into())),
                    ],
                    vec![
                        Cell::new(CellValue::Text("north".into())),
                        Cell::new(CellValue::Number(1200.0)),
                    ],
                ],
                column_widths: Vec::new(),
                frozen_rows: 1,
                merged: Vec::new(),
            }],
        }
    }

    #[test]
    fn export_returns_verified_inline_artefact() {
        let out = export_inline(&workbook()).expect("the workbook exports");
        assert_eq!(out.media_type, XLSX_MEDIA_TYPE);
        assert_eq!(out.sheets, 1);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(
                out.base64
                    .as_deref()
                    .expect("inline mode carries the bytes"),
            )
            .expect("the artefact is base64");
        assert_eq!(bytes.len() as u64, out.bytes, "the reported length");
        assert_eq!(hex(&bytes), out.sha256, "the reported hash");
        assert_eq!(&bytes[..2], b"PK", "a package, not JSON");
        assert!(out.document_id.is_none());
    }

    #[test]
    fn export_over_inline_cap_is_refused() {
        // 4,000 distinct 100-character strings: ~400 KiB of text, well under the 250k-cell cap
        // and well over the 256 KiB inline cap.
        let rows: Vec<Vec<Cell>> = (0..4_000)
            .map(|row| {
                vec![Cell::new(CellValue::Text(format!(
                    "row-{row:04}-{}",
                    "x".repeat(90)
                )))]
            })
            .collect();
        let big = Workbook {
            sheets: vec![Sheet {
                name: "Big".to_string(),
                rows,
                column_widths: Vec::new(),
                frozen_rows: 0,
                merged: Vec::new(),
            }],
        };
        let error = export_inline(&big).expect_err("too large for inline");
        assert_eq!(error.status, 422);
        // The specific refusal is a field error; `detail` is the document's generic summary.
        let fields = format!("{:?}", error.errors);
        assert_eq!(error.errors.len(), 1, "{fields}");
        assert!(
            fields.contains("too_large") && fields.contains("document"),
            "the refusal is typed and names the document mode: {fields}"
        );
    }

    #[test]
    fn select_sheets_keeps_the_requested_order() {
        let two = Workbook {
            sheets: vec![
                Sheet {
                    name: "A".to_string(),
                    rows: Vec::new(),
                    column_widths: Vec::new(),
                    frozen_rows: 0,
                    merged: Vec::new(),
                },
                Sheet {
                    name: "B".to_string(),
                    rows: Vec::new(),
                    column_widths: Vec::new(),
                    frozen_rows: 0,
                    merged: Vec::new(),
                },
            ],
        };
        let selected = select_sheets(two.clone(), Some(vec!["B".into(), "A".into()]))
            .expect("both sheets exist");
        assert_eq!(selected.sheets[0].name, "B");
        assert_eq!(selected.sheets[1].name, "A");
        assert_eq!(
            select_sheets(two, Some(vec!["C".into()]))
                .expect_err("no such sheet")
                .status,
            422
        );
    }

    fn hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
