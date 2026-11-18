//! The workbook model: sheets, typed cells, number formats and merged ranges.
//!
//! # The wire shape
//!
//! A cell is either a bare scalar — `"text"`, `12.5`, `true`, `null` — or a detailed object
//! `{"v": <scalar>, "f": "<formula>", "fmt": "date"}`. Scalars are the ergonomic form a plain
//! matrix needs; the detailed form is what a date, a formula or a number format requires. A
//! date always carries its format, so a detailed cell is also the only shape a date serializes
//! to — which keeps the JSON round-trip stable.
//!
//! # Tags
//!
//! The JSON form is a bare scalar or a detailed object; the tag is the **shape**, not a field
//! name. That is why [`Cell`] implements `Serialize`/`Deserialize` by hand rather than deriving
//! an internally tagged enum: `{"v": …}` is the object form and anything else is a scalar.

use std::borrow::Cow;

use schemars::{Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::a1;
use crate::error::{MAX_CELLS, MAX_COLUMNS, MAX_ROWS, MAX_SHEETS, MAX_TEXT_CHARS, SheetsError};

/// One workbook: its sheets, in workbook order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Workbook {
    /// At least one sheet. A workbook order is the sheet order.
    pub sheets: Vec<Sheet>,
}

/// One sheet: a name, its rows, and the presentation a package carries beside the values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    /// The sheet's tab name: at most 31 characters, none of `[]:*?/\`, not blank.
    pub name: String,
    /// The rows, top to bottom. A row is a list of cells, left to right; a short row is padded
    /// with empty cells when it is read back.
    #[serde(default)]
    pub rows: Vec<Vec<Cell>>,
    /// Column widths in character units, one per column from `A`. A missing entry is the
    /// default width; a value is clamped to 0–255 by the writer.
    #[serde(default)]
    pub column_widths: Vec<f32>,
    /// How many leading rows stay visible when the sheet scrolls.
    #[serde(default)]
    pub frozen_rows: u32,
    /// Merged ranges. Only the top-left cell of a merge carries a value.
    #[serde(default)]
    pub merged: Vec<MergeRange>,
}

/// One cell: its value, an optional formula whose cached result the value is, and an optional
/// number format.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// The value — or, when `formula` is set, the cached result of that formula.
    pub value: CellValue,
    /// The formula, without the leading `=`.
    pub formula: Option<String>,
    /// The number format applied to the cell.
    pub format: Option<NumberFormat>,
}

impl Cell {
    /// A plain scalar cell.
    pub fn new(value: CellValue) -> Self {
        Self {
            value,
            formula: None,
            format: None,
        }
    }

    /// True when the cell carries nothing at all.
    pub fn is_empty(&self) -> bool {
        self.value == CellValue::Empty && self.formula.is_none()
    }

    /// The value as JSON, without the cell's wrapper.
    fn value_json(&self) -> Value {
        match &self.value {
            CellValue::Empty => Value::Null,
            CellValue::Text(text) => Value::String(text.clone()),
            CellValue::Number(number) => {
                serde_json::Number::from_f64(*number).map_or(Value::Null, Value::Number)
            }
            CellValue::Bool(flag) => Value::Bool(*flag),
            CellValue::Date(date) => Value::String(date.to_string()),
            CellValue::DateTime(date_time) => Value::String(date_time.to_string()),
        }
    }

    /// Resolve the one ambiguity a JSON scalar cannot carry: a date is a string until its
    /// format says otherwise. Also refuses a format that contradicts the value.
    pub fn canonical(mut self, index: usize, reference: &str) -> Result<Self, SheetsError> {
        let cell = || reference.to_string();
        // Midnight is a date: a workbook cannot tell `2026-11-18` from `2026-11-18T00:00:00`,
        // so the model keeps one representation for that instant and the round-trip is exact.
        if let CellValue::DateTime(value) = self.value
            && value.hour == 0
            && value.minute == 0
            && value.second == 0
        {
            self.value = CellValue::Date(Date {
                year: value.year,
                month: value.month,
                day: value.day,
            });
            if self.format == Some(NumberFormat::DateTime) {
                self.format = Some(NumberFormat::Date);
            }
        }
        match (self.format, &self.value) {
            (None, CellValue::Date(_) | CellValue::DateTime(_)) => {
                self.format = Some(match self.value {
                    CellValue::Date(_) => NumberFormat::Date,
                    _ => NumberFormat::DateTime,
                });
            }
            (Some(NumberFormat::Date), CellValue::Text(text)) => {
                let parsed = parse_date(text).ok_or_else(|| SheetsError::InvalidDate {
                    index,
                    cell: cell(),
                    value: text.clone(),
                    kind: "date",
                })?;
                self.value = CellValue::Date(parsed);
            }
            (Some(NumberFormat::DateTime), CellValue::Text(text)) => {
                let parsed = parse_datetime(text).ok_or_else(|| SheetsError::InvalidDate {
                    index,
                    cell: cell(),
                    value: text.clone(),
                    kind: "date-time",
                })?;
                self.value = CellValue::DateTime(parsed);
            }
            (Some(NumberFormat::Date), CellValue::Date(_))
            | (Some(NumberFormat::DateTime), CellValue::DateTime(_))
            | (Some(NumberFormat::Date), CellValue::DateTime(_)) => {
                if matches!(self.format, Some(NumberFormat::Date))
                    && matches!(self.value, CellValue::DateTime(_))
                {
                    // A date format over a date-time value truncates: say so rather than
                    // write a value the package would silently display differently.
                    self.format = Some(NumberFormat::DateTime);
                }
            }
            (Some(NumberFormat::Date | NumberFormat::DateTime), _) => {
                return Err(SheetsError::InvalidDate {
                    index,
                    cell: cell(),
                    value: self.value.display(),
                    kind: "date",
                });
            }
            _ => {}
        }
        if self.formula.is_some()
            && matches!(self.value, CellValue::Date(_) | CellValue::DateTime(_))
        {
            return Err(SheetsError::FormulaCachedInvalid {
                index,
                cell: cell(),
            });
        }
        if let CellValue::Text(text) = &self.value {
            let chars = text.chars().count();
            if chars > MAX_TEXT_CHARS {
                return Err(SheetsError::TextTooLong {
                    location: format!("sheet {index} cell {reference}"),
                    chars,
                });
            }
        }
        if let CellValue::Number(number) = self.value
            && !number.is_finite()
        {
            return Err(SheetsError::UnsupportedCell {
                index,
                cell: cell(),
                kind: "a non-finite number",
            });
        }
        Ok(self)
    }
}

impl CellValue {
    /// The value as a caller sees it in a message.
    fn display(&self) -> String {
        match self {
            Self::Empty => "empty".to_string(),
            Self::Text(text) => text.clone(),
            Self::Number(number) => number.to_string(),
            Self::Bool(flag) => flag.to_string(),
            Self::Date(date) => date.to_string(),
            Self::DateTime(date_time) => date_time.to_string(),
        }
    }
}

impl Serialize for Cell {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.formula.is_none() && self.format.is_none() {
            return self.value_json().serialize(serializer);
        }
        let mut map = Map::new();
        map.insert("v".to_string(), self.value_json());
        if let Some(formula) = &self.formula {
            map.insert("f".to_string(), Value::String(formula.clone()));
        }
        if let Some(format) = self.format {
            map.insert(
                "fmt".to_string(),
                serde_json::to_value(format).map_err(serde::ser::Error::custom)?,
            );
        }
        Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Cell {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Cell::from_json(value).map_err(serde::de::Error::custom)
    }
}

impl Cell {
    /// The wire form, parsed. Public so a caller holding raw JSON can use the same rule.
    pub fn from_json(value: Value) -> Result<Self, SheetsError> {
        match value {
            Value::Object(mut map) => {
                let Some(raw) = map.remove("v") else {
                    return Err(SheetsError::Unreadable {
                        reason: "a detailed cell must carry `v`".to_string(),
                    });
                };
                let formula = match map.remove("f") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(formula)) => Some(formula),
                    Some(other) => {
                        return Err(SheetsError::Unreadable {
                            reason: format!("cell `f` must be a string, not {other}"),
                        });
                    }
                };
                let format = match map.remove("fmt") {
                    None | Some(Value::Null) => None,
                    Some(value) => Some(serde_json::from_value::<NumberFormat>(value).map_err(
                        |error| SheetsError::Unreadable {
                            reason: format!("cell `fmt` is not a known format: {error}"),
                        },
                    )?),
                };
                if let Some(unknown) = map.keys().next() {
                    return Err(SheetsError::Unreadable {
                        reason: format!("cell has an unknown field `{unknown}`"),
                    });
                }
                Ok(Self {
                    value: CellValue::from_json(raw)?,
                    formula,
                    format,
                })
            }
            other => Ok(Self {
                value: CellValue::from_json(other)?,
                formula: None,
                format: None,
            }),
        }
    }
}

impl CellValue {
    /// The scalar wire form, parsed. A string is text here; a date needs a format to say so.
    pub fn from_json(value: Value) -> Result<Self, SheetsError> {
        match value {
            Value::Null => Ok(Self::Empty),
            Value::Bool(flag) => Ok(Self::Bool(flag)),
            Value::Number(number) => number
                .as_f64()
                .filter(|number| number.is_finite())
                .map(Self::Number)
                .ok_or_else(|| SheetsError::Unreadable {
                    reason: "a number cell must be a finite f64".to_string(),
                }),
            Value::String(text) => Ok(Self::Text(text)),
            other => Err(SheetsError::Unreadable {
                reason: format!("a cell value must be a scalar, not {other}"),
            }),
        }
    }
}

impl schemars::JsonSchema for Cell {
    fn schema_name() -> Cow<'static, str> {
        "Cell".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        CellWire::json_schema(generator)
    }
}

/// The documented union: a scalar, or a detailed object. `null` is accepted for an empty cell
/// and is intentionally not part of the schema's scalar list.
#[derive(schemars::JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum CellWire {
    /// A bare scalar: text, a number or a boolean.
    Scalar(CellScalar),
    /// A value with a formula or a number format.
    Detailed {
        /// The value (or a formula's cached result).
        v: CellScalar,
        /// The formula, without the leading `=`.
        f: Option<String>,
        /// The number format.
        fmt: Option<NumberFormat>,
    },
}

/// The scalars a cell carries outside its wrapper.
#[derive(schemars::JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum CellScalar {
    /// A text value; an ISO-8601 date when the format says so.
    Text(String),
    /// A numeric value.
    Number(f64),
    /// A boolean value.
    Bool(bool),
}

/// A calendar date, ISO-8601 on the wire (`2026-11-18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct Date {
    /// The year, four digits.
    pub year: i32,
    /// The month, 1–12.
    pub month: u8,
    /// The day, 1–31.
    pub day: u8,
}

/// A date and a wall-clock time, ISO-8601 on the wire (`2026-11-18T13:45:00`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct DateTime {
    /// The year, four digits.
    pub year: i32,
    /// The month, 1–12.
    pub month: u8,
    /// The day, 1–31.
    pub day: u8,
    /// The hour, 0–23.
    pub hour: u8,
    /// The minute, 0–59.
    pub minute: u8,
    /// The second, 0–59.
    pub second: u8,
}

/// A cell value.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum CellValue {
    /// Nothing; the cell is not written.
    Empty,
    /// A boolean.
    Bool(bool),
    /// A number.
    Number(f64),
    /// Text. A date-looking string is text unless its format says date.
    Text(String),
    /// A calendar date.
    Date(Date),
    /// A date and a time.
    DateTime(DateTime),
}

/// The number formats a package can carry, each mapped onto a built-in SpreadsheetML format id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NumberFormat {
    /// No explicit format; the value is displayed as the reader chooses.
    General,
    /// A whole number (`0`).
    Integer,
    /// Two decimal places (`0.00`).
    Decimal,
    /// A percentage (`0%`).
    Percent,
    /// A date (`mm-dd-yy`).
    Date,
    /// A date and a time (`m/d/yy h:mm`).
    DateTime,
    /// Text (`@`).
    Text,
}

impl NumberFormat {
    /// The built-in SpreadsheetML format id this maps to (all under 164, so no `numFmts` entry).
    pub fn builtin_id(self) -> u32 {
        match self {
            Self::General => 0,
            Self::Integer => 1,
            Self::Decimal => 2,
            Self::Percent => 9,
            Self::Date => 14,
            Self::DateTime => 22,
            Self::Text => 49,
        }
    }

    /// The `cellXfs` index this format occupies in the workbook's one style sheet.
    pub fn style_index(self) -> usize {
        match self {
            Self::General => 0,
            Self::Integer => 1,
            Self::Decimal => 2,
            Self::Percent => 3,
            Self::Date => 4,
            Self::DateTime => 5,
            Self::Text => 6,
        }
    }

    /// Every format, in style-index order.
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Integer,
        Self::Decimal,
        Self::Percent,
        Self::Date,
        Self::DateTime,
        Self::Text,
    ];
}

/// One merged range, `A1:B2` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MergeRange {
    /// The top row, 1-based.
    pub start_row: u32,
    /// The left column, 1-based.
    pub start_column: u32,
    /// The bottom row, 1-based.
    pub end_row: u32,
    /// The right column, 1-based.
    pub end_column: u32,
}

impl MergeRange {
    /// The A1 spelling.
    pub fn text(self) -> String {
        a1::merge_ref(
            (self.start_row, self.start_column),
            (self.end_row, self.end_column),
        )
    }
}

impl TryFrom<String> for MergeRange {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let ((start_row, start_column), (end_row, end_column)) =
            a1::parse_merge_ref(&text).ok_or_else(|| format!("`{text}` is not an A1:B2 range"))?;
        Ok(Self {
            start_row,
            start_column,
            end_row,
            end_column,
        })
    }
}

impl From<MergeRange> for String {
    fn from(range: MergeRange) -> Self {
        range.text()
    }
}

impl TryFrom<String> for Date {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        parse_date(&text).ok_or_else(|| format!("`{text}` is not an ISO-8601 date"))
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

impl From<Date> for String {
    fn from(date: Date) -> Self {
        date.to_string()
    }
}

impl TryFrom<String> for DateTime {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        parse_datetime(&text).ok_or_else(|| format!("`{text}` is not an ISO-8601 date-time"))
    }
}

impl std::fmt::Display for DateTime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

impl From<DateTime> for String {
    fn from(value: DateTime) -> Self {
        value.to_string()
    }
}

/// Parse `YYYY-MM-DD`, refusing an impossible calendar date.
pub(crate) fn parse_date(text: &str) -> Option<Date> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i32 = text.get(0..4)?.parse().ok()?;
    let month: u8 = text.get(5..7)?.parse().ok()?;
    let day: u8 = text.get(8..10)?.parse().ok()?;
    if !valid_calendar(year, month, day) {
        return None;
    }
    Some(Date { year, month, day })
}

/// Parse `YYYY-MM-DDTHH:MM:SS`.
pub(crate) fn parse_datetime(text: &str) -> Option<DateTime> {
    let (date, time) = text.split_once('T')?;
    let date = parse_date(date)?;
    let mut parts = time.split(':');
    let hour: u8 = parts.next()?.parse().ok()?;
    let minute: u8 = parts.next()?.parse().ok()?;
    let second: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some(DateTime {
        year: date.year,
        month: date.month,
        day: date.day,
        hour,
        minute,
        second,
    })
}

/// Whether a year is a leap year in the Gregorian calendar.
pub(crate) fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The days in a month, 1–12.
pub(crate) fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn valid_calendar(year: i32, month: u8, day: u8) -> bool {
    (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)
}

impl Workbook {
    /// Trim trailing empty rows and cells, and resolve every cell's format. The result is the
    /// canonical form a reader produces, so `from_xlsx(to_xlsx(w)) == w.canonical()?`.
    pub fn canonical(self) -> Result<Self, SheetsError> {
        let mut sheets = Vec::with_capacity(self.sheets.len());
        for (position, sheet) in self.sheets.into_iter().enumerate() {
            let index = position + 1;
            let mut rows = Vec::with_capacity(sheet.rows.len());
            for (row_position, row) in sheet.rows.into_iter().enumerate() {
                let row_number = row_position + 1;
                let mut cells = Vec::with_capacity(row.len());
                for (column_position, cell) in row.into_iter().enumerate() {
                    let reference = a1::cell_ref(row_number as u32, column_position as u32 + 1);
                    cells.push(cell.canonical(index, &reference)?);
                }
                while cells.last().is_some_and(|cell: &Cell| cell.is_empty()) {
                    cells.pop();
                }
                rows.push(cells);
            }
            while rows.last().is_some_and(|row: &Vec<Cell>| row.is_empty()) {
                rows.pop();
            }
            sheets.push(Sheet {
                name: sheet.name,
                rows,
                column_widths: sheet.column_widths,
                frozen_rows: sheet.frozen_rows,
                merged: sheet.merged,
            });
        }
        let workbook = Workbook { sheets };
        workbook.validate()?;
        Ok(workbook)
    }

    /// Check every count, name and merged range without producing a package.
    pub fn validate(&self) -> Result<(), SheetsError> {
        if self.sheets.is_empty() {
            return Err(SheetsError::NoSheets);
        }
        if self.sheets.len() > MAX_SHEETS {
            return Err(SheetsError::TooManySheets {
                count: self.sheets.len(),
            });
        }
        let mut seen: Vec<String> = Vec::with_capacity(self.sheets.len());
        let mut cells = 0u64;
        for (position, sheet) in self.sheets.iter().enumerate() {
            let index = position + 1;
            check_sheet_name(index, &sheet.name)?;
            let lower = sheet.name.to_lowercase();
            if seen.contains(&lower) {
                return Err(SheetsError::DuplicateSheetName {
                    index,
                    name: sheet.name.clone(),
                });
            }
            seen.push(lower);

            if sheet.rows.len() > MAX_ROWS {
                return Err(SheetsError::TooLarge {
                    location: format!("sheet {index} `{}`", sheet.name),
                    what: "rows",
                    count: sheet.rows.len() as u64,
                    limit: MAX_ROWS as u64,
                });
            }
            if sheet.column_widths.len() > MAX_COLUMNS {
                return Err(SheetsError::TooLarge {
                    location: format!("sheet {index} `{}`", sheet.name),
                    what: "column widths",
                    count: sheet.column_widths.len() as u64,
                    limit: MAX_COLUMNS as u64,
                });
            }
            let mut used_columns = 0usize;
            for row in &sheet.rows {
                used_columns = used_columns.max(row.len());
                cells += row.iter().filter(|cell| !cell.is_empty()).count() as u64;
            }
            if used_columns > MAX_COLUMNS {
                return Err(SheetsError::TooLarge {
                    location: format!("sheet {index} `{}`", sheet.name),
                    what: "columns",
                    count: used_columns as u64,
                    limit: MAX_COLUMNS as u64,
                });
            }
            check_merges(index, sheet)?;
        }
        if cells > MAX_CELLS as u64 {
            return Err(SheetsError::TooLarge {
                location: "the workbook".to_string(),
                what: "non-empty cells",
                count: cells,
                limit: MAX_CELLS as u64,
            });
        }
        Ok(())
    }
}

/// Excel's sheet-name rules: 1–31 characters, none of `[]:*?/\`, not blank, not a leading or
/// trailing apostrophe.
fn check_sheet_name(index: usize, name: &str) -> Result<(), SheetsError> {
    let refuse = |reason: &'static str| {
        Err(SheetsError::SheetNameInvalid {
            index,
            name: name.to_string(),
            reason,
        })
    };
    if name.trim().is_empty() {
        return refuse("a sheet name must not be blank");
    }
    if name.chars().count() > 31 {
        return refuse("a sheet name is at most 31 characters");
    }
    if name.contains(['[', ']', ':', '*', '?', '/', '\\']) {
        return refuse("a sheet name may not contain any of `[ ] : * ? / \\`");
    }
    if name.starts_with('\'') || name.ends_with('\'') {
        return refuse("a sheet name may not start or end with an apostrophe");
    }
    Ok(())
}

/// Merged ranges: ordered, and never overlapping.
///
/// A merge may span cells that carry no value — Excel does that routinely, and refusing it
/// would reject ordinary workbooks — so this checks geometry, not occupancy.
fn check_merges(index: usize, sheet: &Sheet) -> Result<(), SheetsError> {
    for (position, merge) in sheet.merged.iter().enumerate() {
        let range = merge.text();
        if merge.start_row == 0
            || merge.start_column == 0
            || merge.end_row < merge.start_row
            || merge.end_column < merge.start_column
        {
            return Err(SheetsError::MergeReversed { index, range });
        }
        for other in &sheet.merged[..position] {
            let overlaps = merge.start_row <= other.end_row
                && other.start_row <= merge.end_row
                && merge.start_column <= other.end_column
                && other.start_column <= merge.end_column;
            if overlaps {
                return Err(SheetsError::MergeOverlaps { index, range });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(name: &str, rows: Vec<Vec<Cell>>) -> Sheet {
        Sheet {
            name: name.to_string(),
            rows,
            column_widths: Vec::new(),
            frozen_rows: 0,
            merged: Vec::new(),
        }
    }

    #[test]
    fn a_scalar_cell_serializes_bare_and_a_detailed_cell_does_not() {
        let scalar = Cell::new(CellValue::Text("x".into()));
        assert_eq!(
            serde_json::to_value(&scalar).unwrap(),
            Value::String("x".into())
        );

        let detailed = Cell {
            value: CellValue::Number(3.5),
            formula: Some("SUM(A1:A2)".into()),
            format: Some(NumberFormat::Decimal),
        };
        let json = serde_json::to_value(&detailed).unwrap();
        assert_eq!(json["v"], Value::from(3.5));
        assert_eq!(json["f"], Value::String("SUM(A1:A2)".into()));
        assert_eq!(json["fmt"], Value::String("decimal".into()));
        assert_eq!(serde_json::from_value::<Cell>(json).unwrap(), detailed);
    }

    #[test]
    fn a_date_is_a_string_until_its_format_says_otherwise() {
        let cell: Cell = serde_json::from_value(serde_json::json!({
            "v": "2026-11-18", "fmt": "date"
        }))
        .expect("the detailed shape parses");
        let canonical = cell.canonical(1, "A1").expect("a real date");
        assert_eq!(
            canonical.value,
            CellValue::Date(Date {
                year: 2026,
                month: 11,
                day: 18
            })
        );
        assert_eq!(canonical.format, Some(NumberFormat::Date));

        let refused = Cell::from_json(serde_json::json!({"v": "2026-02-30", "fmt": "date"}))
            .expect("parsing is shape-only")
            .canonical(1, "A1");
        assert_eq!(refused.unwrap_err().code(), "invalid_date");
    }

    #[test]
    fn canonical_trims_trailing_empties_and_keeps_interior_ones() {
        let workbook = Workbook {
            sheets: vec![sheet(
                "S",
                vec![
                    vec![
                        Cell::new(CellValue::Text("a".into())),
                        Cell::new(CellValue::Empty),
                    ],
                    vec![Cell::new(CellValue::Empty)],
                ],
            )],
        };
        let canonical = workbook.canonical().expect("trims");
        assert_eq!(canonical.sheets[0].rows.len(), 1);
        assert_eq!(canonical.sheets[0].rows[0].len(), 1);
    }

    #[test]
    fn the_sheet_name_rules_are_excels() {
        let bad = Workbook {
            sheets: vec![sheet("a/b", Vec::new())],
        };
        assert_eq!(bad.validate().unwrap_err().code(), "invalid_sheet_name");
        let duplicate = Workbook {
            sheets: vec![sheet("Data", Vec::new()), sheet("data", Vec::new())],
        };
        assert_eq!(
            duplicate.validate().unwrap_err().code(),
            "duplicate_sheet_name"
        );
        assert!(
            Workbook {
                sheets: vec![sheet("Data", Vec::new())]
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn overlapping_merges_are_refused() {
        let mut s = sheet("S", vec![vec![Cell::new(CellValue::Number(1.0)); 3]; 3]);
        s.merged = vec![
            MergeRange {
                start_row: 1,
                start_column: 1,
                end_row: 2,
                end_column: 2,
            },
            MergeRange {
                start_row: 2,
                start_column: 2,
                end_row: 3,
                end_column: 3,
            },
        ];
        let workbook = Workbook { sheets: vec![s] };
        assert_eq!(workbook.validate().unwrap_err().code(), "merge_overlaps");
    }

    #[test]
    fn dates_parse_and_format_symmetrically() {
        let date = parse_date("2024-02-29").expect("a leap day");
        assert_eq!(String::from(date), "2024-02-29");
        assert_eq!(parse_date("2023-02-29"), None);
        assert_eq!(parse_date("2026-13-01"), None);
        let stamp = parse_datetime("2026-11-18T13:45:59").expect("a date-time");
        assert_eq!(String::from(stamp), "2026-11-18T13:45:59");
        assert_eq!(parse_datetime("2026-11-18T24:00:00"), None);
    }
}
