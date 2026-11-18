#![forbid(unsafe_code)]
//! The spreadsheet toolkit: one workbook model, written as deterministic OOXML, read back.
//!
//! Two halves with two different contracts:
//!
//! - [`to_xlsx`] **produces** a `.xlsx` whose bytes are a pure function of the model. The
//!   archive is the workspace's stored-ZIP writer (`conveyor-zipstore`), rows and cells ascend,
//!   styles are a fixed table and shared strings are numbered first-seen — so the same workbook
//!   exports to the same bytes, and a caller can verify an artefact by its hash.
//! - [`from_xlsx`] **consumes** a workbook someone else produced. It reads real files —
//!   deflated, shared-stringed, style-formatted — through `calamine`, and maps what it finds
//!   into the same model, refusing anything the model cannot represent losslessly rather than
//!   dropping it.
//!
//! The model is the wire shape as well as the library shape: a cell is a bare JSON scalar
//! (`"text"`, `12.5`, `true`, `null`) or a detailed object `{"v": …, "f": …, "fmt": …}`. A date
//! always carries its format, which is what keeps the two forms unambiguous.
//!
//! # Determinism
//!
//! [`to_xlsx`] canonicalizes first: trailing empty rows and cells are trimmed and every cell's
//! format is resolved, so `from_xlsx(&to_xlsx(&w)?)? == w.clone().canonical()?`. There is no
//! clock, no random id and no map iteration in the writer.

pub mod a1;
mod error;
pub mod model;
mod reader;
mod serial;
mod xlsx;

pub use error::{
    MAX_CELLS, MAX_COLUMNS, MAX_INPUT_BYTES, MAX_PACKAGE_BYTES, MAX_ROWS, MAX_SHEETS,
    MAX_TEXT_CHARS, SheetsError,
};
pub use model::{Cell, CellValue, Date, DateTime, MergeRange, NumberFormat, Sheet, Workbook};
pub use reader::from_xlsx;
pub use xlsx::to_xlsx;

/// The `.xlsx` MIME type — the media type a produced workbook carries.
pub const XLSX_MEDIA_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
