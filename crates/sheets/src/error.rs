//! Every refusal the sheet model, its writer and its reader make, each with a stable slug.

use thiserror::Error;

/// The most sheets one workbook may carry.
pub const MAX_SHEETS: usize = 64;
/// The most rows one sheet may carry.
pub const MAX_ROWS: usize = 100_000;
/// The most columns one sheet may carry.
pub const MAX_COLUMNS: usize = 1_024;
/// The most non-empty cells one workbook may carry.
///
/// This is what keeps a **stored**-ZIP package under [`MAX_PACKAGE_BYTES`]: stored entries do
/// not compress, so cells are the honest unit to cap rather than a byte count discovered after
/// the package is built.
pub const MAX_CELLS: usize = 250_000;
/// The most characters one text cell may carry (the SpreadsheetML limit).
pub const MAX_TEXT_CHARS: usize = 32_767;
/// The largest package this crate will produce.
pub const MAX_PACKAGE_BYTES: usize = 10 * 1024 * 1024;
/// The largest input this crate will read.
pub const MAX_INPUT_BYTES: usize = 10 * 1024 * 1024;

/// Why a workbook was refused.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum SheetsError {
    /// A workbook with no sheets is not a workbook.
    #[error("the workbook has no sheets")]
    NoSheets,
    /// More sheets than [`MAX_SHEETS`].
    #[error("the workbook has {count} sheets; the limit is {MAX_SHEETS}")]
    TooManySheets {
        /// How many sheets the workbook carries.
        count: usize,
    },
    /// A sheet name Excel cannot carry.
    #[error("sheet {index} name `{name}`: {reason}")]
    SheetNameInvalid {
        /// The sheet's 1-based position.
        index: usize,
        /// The name as given.
        name: String,
        /// Which rule it breaks.
        reason: &'static str,
    },
    /// Two sheets sharing a name (Excel matches names case-insensitively).
    #[error("sheet {index} name `{name}` is already used by an earlier sheet")]
    DuplicateSheetName {
        /// The sheet's 1-based position.
        index: usize,
        /// The repeated name.
        name: String,
    },
    /// A count above one of the caps.
    #[error("{location}: {count} {what}; the limit is {limit}")]
    TooLarge {
        /// Sheet and cell, or the workbook itself.
        location: String,
        /// What was counted.
        what: &'static str,
        /// How many there were.
        count: u64,
        /// The cap.
        limit: u64,
    },
    /// A text value over [`MAX_TEXT_CHARS`].
    #[error("{location}: {chars} characters; the limit is {MAX_TEXT_CHARS}")]
    TextTooLong {
        /// Sheet and cell.
        location: String,
        /// How many characters the value carries.
        chars: usize,
    },
    /// Two merged ranges that share a cell.
    #[error("sheet {index} merged range `{range}` overlaps another merged range")]
    MergeOverlaps {
        /// The sheet's 1-based position.
        index: usize,
        /// The offending range in A1 notation.
        range: String,
    },
    /// A merged range whose end precedes its start.
    #[error("sheet {index} merged range `{range}`: the range ends before it starts")]
    MergeReversed {
        /// The sheet's 1-based position.
        index: usize,
        /// The offending range in A1 notation.
        range: String,
    },
    /// A formula whose cached value is not a value.
    #[error(
        "sheet {index} cell {cell}: a formula's cached value must be a number, text, boolean or empty"
    )]
    FormulaCachedInvalid {
        /// The sheet's 1-based position.
        index: usize,
        /// The cell in A1 notation.
        cell: String,
    },
    /// A value the format cannot carry.
    #[error("sheet {index} cell {cell}: {kind}")]
    UnsupportedCell {
        /// The sheet's 1-based position.
        index: usize,
        /// The cell in A1 notation.
        cell: String,
        /// What could not be written.
        kind: &'static str,
    },
    /// A date or date-time whose text is not ISO-8601.
    #[error("sheet {index} cell {cell}: `{value}` is not an ISO-8601 {kind}")]
    InvalidDate {
        /// The sheet's 1-based position.
        index: usize,
        /// The cell in A1 notation.
        cell: String,
        /// The value as given.
        value: String,
        /// `date` or `date-time`.
        kind: &'static str,
    },
    /// A package over [`MAX_PACKAGE_BYTES`].
    #[error("the workbook is {bytes} bytes; the package ceiling is {MAX_PACKAGE_BYTES}")]
    PackageTooLarge {
        /// The produced size.
        bytes: usize,
    },
    /// A part that does not fit ZIP32.
    #[error("part `{part}` exceeds the 4 GiB limit of the ZIP32 format")]
    PackagePartTooLarge {
        /// The part that did not fit.
        part: String,
    },
    /// An input over [`MAX_INPUT_BYTES`].
    #[error("the input is {bytes} bytes; the import ceiling is {MAX_INPUT_BYTES}")]
    InputTooLarge {
        /// The received size.
        bytes: usize,
    },
    /// Bytes the reader cannot parse.
    #[error("the workbook could not be read: {reason}")]
    Unreadable {
        /// The reader's own message.
        reason: String,
    },
    /// A cell whose content has no lossless representation in this model.
    #[error("sheet {index} cell {cell}: {kind} cells are not supported")]
    ImportRefused {
        /// The sheet's 1-based position.
        index: usize,
        /// The cell in A1 notation.
        cell: String,
        /// What the reader found.
        kind: &'static str,
    },
}

impl SheetsError {
    /// The stable slug for logs, problem documents and rows.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoSheets => "no_sheets",
            Self::TooManySheets { .. } => "too_many_sheets",
            Self::SheetNameInvalid { .. } => "invalid_sheet_name",
            Self::DuplicateSheetName { .. } => "duplicate_sheet_name",
            Self::TooLarge { .. } => "too_large",
            Self::TextTooLong { .. } => "text_too_long",
            Self::MergeOverlaps { .. } => "merge_overlaps",
            Self::MergeReversed { .. } => "merge_reversed",
            Self::FormulaCachedInvalid { .. } => "formula_cached_invalid",
            Self::UnsupportedCell { .. } => "unsupported_cell",
            Self::InvalidDate { .. } => "invalid_date",
            Self::PackageTooLarge { .. } => "package_too_large",
            Self::PackagePartTooLarge { .. } => "package_part_too_large",
            Self::InputTooLarge { .. } => "input_too_large",
            Self::Unreadable { .. } => "unreadable",
            Self::ImportRefused { .. } => "import_refused",
        }
    }
}
