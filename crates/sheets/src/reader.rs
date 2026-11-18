//! Reading a workbook somebody else produced.
//!
//! `calamine` does the format work — inflating deflated parts, resolving relationships, folding
//! shared strings, applying styles to dates — and this module does the one thing a general
//! reader cannot: decide what the model can carry *losslessly*. A duration, an error cell or an
//! ISO date string the model cannot reproduce is refused by name rather than dropped, because a
//! silent drop is the failure mode a spreadsheet reader must not have.
//!
//! # What is not preserved on import
//!
//! Column widths and frozen rows are presentation a foreign workbook is free to set in ways the
//! model does not model, so they read back as "default width" and "not frozen". A number format
//! outside the seven built-ins (a custom currency code, say) reads as General: a documented loss,
//! never an invented format. The values, formulas, cached results, the seven formats, dates,
//! merged ranges, sheet names and sheet order are preserved.

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;

use calamine::{Data, Dimensions, Reader, StyleRange, Xlsx, open_workbook_from_rs};

use crate::a1;
use crate::error::{MAX_CELLS, MAX_COLUMNS, MAX_INPUT_BYTES, MAX_ROWS, MAX_SHEETS, SheetsError};
use crate::model::{
    Cell, CellValue, Date, DateTime, MergeRange, NumberFormat, Sheet, Workbook, parse_date,
    parse_datetime,
};

/// Read a `.xlsx` into the workbook model.
///
/// The result is canonicalized, so it equals what [`crate::to_xlsx`] would accept unchanged.
pub fn from_xlsx(bytes: &[u8]) -> Result<Workbook, SheetsError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(SheetsError::InputTooLarge { bytes: bytes.len() });
    }
    let mut workbook: Xlsx<_> = open_workbook_from_rs::<Xlsx<Cursor<Vec<u8>>>, _>(Cursor::new(
        bytes.to_vec(),
    ))
    .map_err(|error| SheetsError::Unreadable {
        reason: error.to_string(),
    })?;

    let names = workbook.sheet_names();
    if names.is_empty() {
        return Err(SheetsError::NoSheets);
    }
    if names.len() > MAX_SHEETS {
        return Err(SheetsError::TooManySheets { count: names.len() });
    }

    let mut sheets = Vec::with_capacity(names.len());
    for (position, name) in names.iter().enumerate() {
        let index = position + 1;
        let merges =
            workbook
                .merge_cells_by_sheet_name(name)
                .map_err(|error| SheetsError::Unreadable {
                    reason: format!("sheet {index} merged ranges: {error}"),
                })?;
        let range = workbook
            .worksheet_range(name)
            .map_err(|error| SheetsError::Unreadable {
                reason: format!("sheet {index}: {error}"),
            })?;
        let formulas = workbook.worksheet_formula(name).ok();
        let styles = workbook.worksheet_style(name).ok();
        sheets.push(read_sheet(
            index,
            name,
            &range,
            formulas.as_ref(),
            &merges,
            styles.as_ref(),
        )?);
    }

    Workbook { sheets }.canonical()
}

/// One sheet, from calamine's range and formula range.
fn read_sheet(
    index: usize,
    name: &str,
    range: &calamine::Range<Data>,
    formulas: Option<&calamine::Range<String>>,
    merges: &[Dimensions],
    styles: Option<&StyleRange>,
) -> Result<Sheet, SheetsError> {
    let (start_row, start_column) = range.start().unwrap_or((0, 0));

    // The used extent, extended by every merged range: a merge over empty cells is legal and
    // its cells must exist in the model for the merge to be writable again.
    let mut height = 0usize;
    let mut width = 0usize;
    for (row, column, _) in range.used_cells() {
        // `used_cells` is relative to the range's start, and a range starts at its first used
        // cell — so a leading empty row is only visible through the offset.
        height = height.max(start_row as usize + row + 1);
        width = width.max(start_column as usize + column + 1);
    }
    let mut merged = Vec::with_capacity(merges.len());
    for dimensions in merges {
        let (end_row, end_column) = dimensions.end;
        height = height.max((end_row as usize) + 1);
        width = width.max((end_column as usize) + 1);
        merged.push(MergeRange {
            start_row: dimensions.start.0 + 1,
            start_column: dimensions.start.1 + 1,
            end_row: end_row + 1,
            end_column: end_column + 1,
        });
    }
    if height > MAX_ROWS {
        return Err(SheetsError::TooLarge {
            location: format!("sheet {index} `{name}`"),
            what: "rows",
            count: height as u64,
            limit: MAX_ROWS as u64,
        });
    }
    if width > MAX_COLUMNS {
        return Err(SheetsError::TooLarge {
            location: format!("sheet {index} `{name}`"),
            what: "columns",
            count: width as u64,
            limit: MAX_COLUMNS as u64,
        });
    }

    // The formats, recovered from each cell's style: a percentage is a number plus a format, and
    // the format lives in the style table, not in the value.
    let mut formats: HashMap<(u32, u32), NumberFormat> = HashMap::new();
    if let Some(styles) = styles {
        for (row, column, style) in styles.cells() {
            let id = style
                .number_format
                .as_ref()
                .and_then(|format| format.format_id);
            if let Some(format) = id.and_then(format_for_id) {
                formats.insert((row, column), format);
            }
        }
    }

    // Sparse first, rectangular never: a sheet whose only content is at row 100,000 must not
    // allocate a 100,000 × 1,024 grid to hold one cell. Rows are materialised only to the width
    // their own last cell needs, and a row with no cells stays empty.
    let mut sparse: BTreeMap<(usize, usize), Cell> = BTreeMap::new();
    let mut used = 0u64;
    for (row, column, data) in range.used_cells() {
        used += 1;
        if used > MAX_CELLS as u64 {
            return Err(SheetsError::TooLarge {
                location: format!("sheet {index} `{name}`"),
                what: "non-empty cells",
                count: used,
                limit: MAX_CELLS as u64,
            });
        }
        let absolute_row = start_row as usize + row;
        let absolute_column = start_column as usize + column;
        let reference = a1::cell_ref(absolute_row as u32 + 1, absolute_column as u32 + 1);
        let absolute = (absolute_row as u32, absolute_column as u32);
        let formula = formulas
            .and_then(|formulas| formulas.get_value(absolute))
            .map(|formula| formula.trim().trim_start_matches('=').to_string())
            .filter(|formula| !formula.is_empty());
        let value = read_value(index, &reference, data)?;
        // A date format over a value the reader typed as a number is the workbook's own
        // inconsistency: fall back to General rather than refuse a cell we can carry.
        let format = formats.get(&absolute).copied().filter(|format| {
            !matches!(format, NumberFormat::Date | NumberFormat::DateTime)
                || matches!(value, CellValue::Date(_) | CellValue::DateTime(_))
        });
        sparse.insert(
            (absolute_row, absolute_column),
            Cell {
                value,
                formula,
                format,
            },
        );
    }
    let rows = materialise(sparse, height, width, index, name)?;

    Ok(Sheet {
        name: name.to_string(),
        rows,
        column_widths: Vec::new(),
        frozen_rows: 0,
        merged,
    })
}

/// Turn the sparse cells into positional rows, one per index up to `height`.
///
/// A row is as wide as its rightmost cell, not as wide as the sheet: `Workbook::canonical` trims
/// trailing empty cells anyway, and a wide-but-empty row would only cost memory.
fn materialise(
    mut sparse: BTreeMap<(usize, usize), Cell>,
    height: usize,
    width: usize,
    index: usize,
    name: &str,
) -> Result<Vec<Vec<Cell>>, SheetsError> {
    let mut rows: Vec<Vec<Cell>> = Vec::with_capacity(height);
    for row in 0..height {
        let mut cells: Vec<Cell> = Vec::new();
        // `BTreeMap` is ordered, so the cells of one row are contiguous and already ascending.
        while let Some(entry) = sparse.first_entry()
            && entry.key().0 == row
        {
            let (_, column) = entry.key().to_owned();
            let cell = entry.remove();
            if column >= width {
                return Err(SheetsError::TooLarge {
                    location: format!("sheet {index} `{name}`"),
                    what: "columns",
                    count: (column + 1) as u64,
                    limit: width as u64,
                });
            }
            while cells.len() < column {
                cells.push(Cell::new(CellValue::Empty));
            }
            cells.push(cell);
        }
        rows.push(cells);
    }
    Ok(rows)
}

/// The model's format for a SpreadsheetML built-in id.
///
/// Only the seven the writer emits are mapped. A custom format (id 164 and above) returns `None`
/// — General — because inventing a nearest format would be a silent substitution, and refusing
/// the cell would reject an ordinary workbook.
fn format_for_id(id: u32) -> Option<NumberFormat> {
    match id {
        1 => Some(NumberFormat::Integer),
        2 => Some(NumberFormat::Decimal),
        9 => Some(NumberFormat::Percent),
        14 => Some(NumberFormat::Date),
        22 => Some(NumberFormat::DateTime),
        49 => Some(NumberFormat::Text),
        _ => None,
    }
}

/// One cell's value, refusing what the model has no lossless form for.
fn read_value(index: usize, reference: &str, data: &Data) -> Result<CellValue, SheetsError> {
    let refuse = |kind: &'static str| SheetsError::ImportRefused {
        index,
        cell: reference.to_string(),
        kind,
    };
    match data {
        Data::Empty => Ok(CellValue::Empty),
        Data::Int(number) => Ok(CellValue::Number(*number as f64)),
        Data::Float(number) => Ok(CellValue::Number(*number)),
        Data::String(text) => Ok(CellValue::Text(text.clone())),
        Data::Bool(flag) => Ok(CellValue::Bool(*flag)),
        Data::DateTime(value) => {
            if value.is_duration() {
                return Err(refuse("duration"));
            }
            let (year, month, day, hour, minute, second, _milli) = value.to_ymd_hms_milli();
            // A value with no fraction of a day is a date, not a date-time: the model keeps
            // the distinction so re-exporting does not invent a time.
            if value.as_f64().fract() == 0.0 {
                Ok(CellValue::Date(Date {
                    year: i32::from(year),
                    month,
                    day,
                }))
            } else {
                Ok(CellValue::DateTime(DateTime {
                    year: i32::from(year),
                    month,
                    day,
                    hour,
                    minute,
                    second,
                }))
            }
        }
        Data::DateTimeIso(text) => {
            if let Some(value) = parse_datetime(text) {
                Ok(CellValue::DateTime(value))
            } else if let Some(value) = parse_date(text) {
                Ok(CellValue::Date(value))
            } else {
                Err(refuse("an ISO-8601 date the model cannot parse"))
            }
        }
        Data::DurationIso(_) => Err(refuse("duration")),
        Data::Error(_) => Err(refuse("error")),
    }
}
