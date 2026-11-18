//! The OOXML spreadsheet package: parts, styles, shared strings and the cell bodies.
//!
//! The package is deliberately minimal rather than a clone of an Excel template. It carries
//! the workbook, one worksheet part per sheet, one stylesheet with the seven built-in number
//! formats, and one shared-string table. Nothing else — no theme, no document properties, no
//! calculation chain — because every part is bytes the determinism gate would otherwise have to
//! reason about.
//!
//! Everything is ordered by construction: parts are appended in a fixed sequence, rows and
//! cells ascend, styles are a fixed table, and shared strings are numbered first-seen. There is
//! no clock and no map iteration in the path.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::a1;
use crate::error::{MAX_PACKAGE_BYTES, SheetsError};
use crate::model::{Cell, CellValue, NumberFormat, Sheet, Workbook};
use crate::serial;

const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const NS_CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const REL_WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const REL_STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const REL_SHARED_STRINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";

const CT_WORKBOOK: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
const CT_WORKSHEET: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const CT_STYLES: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
const CT_SHARED_STRINGS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";

/// Write `workbook` as an OOXML `.xlsx`.
///
/// The workbook is canonicalized first, so the same cells in a different JSON shape (a scalar
/// versus its one-field object form) produce the same bytes. The result is checked against
/// [`MAX_PACKAGE_BYTES`]: the stored-ZIP writer does not compress, so the ceiling is stated here
/// rather than discovered as a 413 in a document upload.
pub fn to_xlsx(workbook: &Workbook) -> Result<Vec<u8>, SheetsError> {
    let workbook = workbook.clone().canonical()?;
    let strings = SharedStrings::collect(&workbook);
    let parts = package(&workbook, &strings)?;
    let entries: Vec<(&str, &[u8])> = parts
        .iter()
        .map(|(name, data)| (name.as_str(), data.as_slice()))
        .collect();
    let bytes = conveyor_zipstore::archive(&entries)
        .map_err(|part| SheetsError::PackagePartTooLarge { part })?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(SheetsError::PackageTooLarge { bytes: bytes.len() });
    }
    Ok(bytes)
}

/// The shared-string table: first-seen order, with the occurrence count Excel records.
struct SharedStrings {
    /// The distinct strings, in first-seen order — the order the part is written in.
    unique: Vec<String>,
    /// Where each string sits in `unique`. A lookup only: its iteration order never reaches
    /// the bytes, but it keeps the writer linear in the number of text cells.
    index: HashMap<String, usize>,
    /// How many text cells in total referenced them.
    total: usize,
}

impl SharedStrings {
    fn collect(workbook: &Workbook) -> Self {
        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut unique: Vec<String> = Vec::new();
        let mut total = 0usize;
        for sheet in &workbook.sheets {
            for row in &sheet.rows {
                for cell in row {
                    if let CellValue::Text(text) = &cell.value {
                        // A formula whose cached result is text is an inline `str` cell, not a
                        // shared string: it is text the package computed, not text an author
                        // typed.
                        if cell.formula.is_some() {
                            continue;
                        }
                        total += 1;
                        if index.insert(text.as_str(), unique.len()).is_none() {
                            unique.push(text.clone());
                        }
                    }
                }
            }
        }
        let index: HashMap<String, usize> = unique
            .iter()
            .enumerate()
            .map(|(position, text)| (text.clone(), position))
            .collect();
        debug_assert_eq!(index.len(), unique.len());
        Self {
            unique,
            index,
            total,
        }
    }

    /// The index of `text` in the table.
    fn position(&self, text: &str) -> usize {
        *self.index.get(text).expect("every text cell was collected")
    }
}

/// The package parts, in the order they are archived.
fn package(
    workbook: &Workbook,
    strings: &SharedStrings,
) -> Result<Vec<(String, Vec<u8>)>, SheetsError> {
    let count = workbook.sheets.len();
    let mut parts: Vec<(String, Vec<u8>)> = Vec::with_capacity(count + 5);
    let mut push = |name: String, xml: String| parts.push((name, xml.into_bytes()));

    push("[Content_Types].xml".to_string(), content_types(count));
    push("_rels/.rels".to_string(), root_relationships());
    push("xl/workbook.xml".to_string(), workbook_xml(workbook));
    push(
        "xl/_rels/workbook.xml.rels".to_string(),
        workbook_relationships(count),
    );
    push("xl/styles.xml".to_string(), styles());
    push("xl/sharedStrings.xml".to_string(), shared_strings(strings));
    for (position, sheet) in workbook.sheets.iter().enumerate() {
        let number = position + 1;
        push(
            format!("xl/worksheets/sheet{number}.xml"),
            worksheet(sheet, position + 1, strings)?,
        );
    }
    Ok(parts)
}

fn content_types(sheets: usize) -> String {
    let mut xml = String::from(XML_DECL);
    xml.push_str(&format!("<Types xmlns=\"{NS_CT}\">"));
    xml.push_str(
        "<Default Extension=\"rels\" \
         ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
    );
    xml.push_str("<Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    xml.push_str(&format!(
        "<Override PartName=\"/xl/workbook.xml\" ContentType=\"{CT_WORKBOOK}\"/>"
    ));
    xml.push_str(&format!(
        "<Override PartName=\"/xl/styles.xml\" ContentType=\"{CT_STYLES}\"/>"
    ));
    xml.push_str(&format!(
        "<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"{CT_SHARED_STRINGS}\"/>"
    ));
    for number in 1..=sheets {
        xml.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{number}.xml\" ContentType=\"{CT_WORKSHEET}\"/>"
        ));
    }
    xml.push_str("</Types>");
    xml
}

fn root_relationships() -> String {
    relationships_document(&relationship(
        "rId1",
        REL_OFFICE_DOCUMENT,
        "xl/workbook.xml",
    ))
}

fn workbook_xml(workbook: &Workbook) -> String {
    let mut sheets = String::new();
    for (position, sheet) in workbook.sheets.iter().enumerate() {
        let number = position + 1;
        let _ = write!(
            sheets,
            "<sheet name=\"{}\" sheetId=\"{number}\" r:id=\"rId{number}\"/>",
            escape(&sheet.name)
        );
    }
    format!(
        "{XML_DECL}<workbook xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_R}\">\
         <bookViews><workbookView/></bookViews>\
         <sheets>{sheets}</sheets></workbook>"
    )
}

fn workbook_relationships(sheets: usize) -> String {
    let mut body = String::new();
    for number in 1..=sheets {
        body.push_str(&relationship(
            &format!("rId{number}"),
            REL_WORKSHEET,
            &format!("worksheets/sheet{number}.xml"),
        ));
    }
    body.push_str(&relationship(
        &format!("rId{}", sheets + 1),
        REL_STYLES,
        "styles.xml",
    ));
    body.push_str(&relationship(
        &format!("rId{}", sheets + 2),
        REL_SHARED_STRINGS,
        "sharedStrings.xml",
    ));
    relationships_document(&body)
}

/// The one stylesheet: the seven built-in number formats, in [`NumberFormat::style_index`]
/// order, and the minimum fonts/fills/borders a workbook needs to be readable.
fn styles() -> String {
    let mut xfs = String::new();
    for format in NumberFormat::ALL {
        let id = format.builtin_id();
        if format == NumberFormat::General {
            xfs.push_str(
                "<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>",
            );
        } else {
            let _ = write!(
                xfs,
                "<xf numFmtId=\"{id}\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" \
                 applyNumberFormat=\"1\"/>"
            );
        }
    }
    format!(
        "{XML_DECL}<styleSheet xmlns=\"{NS_MAIN}\">\
         <fonts count=\"1\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts>\
         <fills count=\"2\">\
         <fill><patternFill patternType=\"none\"/></fill>\
         <fill><patternFill patternType=\"gray125\"/></fill>\
         </fills>\
         <borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
         <cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
         <cellXfs count=\"7\">{xfs}</cellXfs>\
         <cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>\
         </styleSheet>"
    )
}

fn shared_strings(strings: &SharedStrings) -> String {
    let mut body = String::new();
    for text in &strings.unique {
        let _ = write!(
            body,
            "<si><t xml:space=\"preserve\">{}</t></si>",
            escape(text)
        );
    }
    format!(
        "{XML_DECL}<sst xmlns=\"{NS_MAIN}\" count=\"{}\" uniqueCount=\"{}\">{body}</sst>",
        strings.total,
        strings.unique.len()
    )
}

fn worksheet(sheet: &Sheet, index: usize, strings: &SharedStrings) -> Result<String, SheetsError> {
    let mut xml = String::from(XML_DECL);
    let _ = write!(xml, "<worksheet xmlns=\"{NS_MAIN}\" xmlns:r=\"{NS_R}\">");
    let _ = write!(xml, "<dimension ref=\"{}\"/>", dimension(sheet));
    if sheet.frozen_rows > 0 {
        let split = sheet.frozen_rows.min(sheet.rows.len() as u32);
        if split > 0 {
            let _ = write!(
                xml,
                "<sheetViews><sheetView workbookViewId=\"0\">\
                 <pane ySplit=\"{split}\" topLeftCell=\"A{}\" activePane=\"bottomLeft\" state=\"frozen\"/>\
                 </sheetView></sheetViews>",
                split + 1
            );
        }
    }
    xml.push_str("<sheetFormatPr defaultRowHeight=\"15\"/>");
    if !sheet.column_widths.is_empty() {
        xml.push_str("<cols>");
        for (position, width) in sheet.column_widths.iter().enumerate() {
            let column = position + 1;
            let width = width.clamp(0.0, 255.0);
            let _ = write!(
                xml,
                "<col min=\"{column}\" max=\"{column}\" width=\"{width}\" customWidth=\"1\"/>"
            );
        }
        xml.push_str("</cols>");
    }
    xml.push_str("<sheetData>");
    for (position, row) in sheet.rows.iter().enumerate() {
        let number = position as u32 + 1;
        if row.iter().all(Cell::is_empty) {
            // A gap is a valid row index: nothing is written for an empty row.
            continue;
        }
        let _ = write!(xml, "<row r=\"{number}\">");
        for (column, cell) in row.iter().enumerate() {
            let column = column as u32 + 1;
            if cell.is_empty() {
                continue;
            }
            let reference = a1::cell_ref(number, column);
            xml.push_str(&cell_xml(cell, &reference, index, strings)?);
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData>");
    if !sheet.merged.is_empty() {
        let _ = write!(xml, "<mergeCells count=\"{}\">", sheet.merged.len());
        for merge in &sheet.merged {
            let _ = write!(xml, "<mergeCell ref=\"{}\"/>", merge.text());
        }
        xml.push_str("</mergeCells>");
    }
    xml.push_str("</worksheet>");
    Ok(xml)
}

/// The used range, extended by every merged range: a merge over empty cells still occupies
/// them, and `dimension` must not understate the sheet. `A1` for an empty sheet.
fn dimension(sheet: &Sheet) -> String {
    let mut last_row = 0u32;
    let mut last_column = 0u32;
    for (position, row) in sheet.rows.iter().enumerate() {
        if let Some(last) = row.iter().rposition(|cell| !cell.is_empty()) {
            last_row = last_row.max(position as u32 + 1);
            last_column = last_column.max(last as u32 + 1);
        }
    }
    for merge in &sheet.merged {
        last_row = last_row.max(merge.end_row);
        last_column = last_column.max(merge.end_column);
    }
    if last_row == 0 || last_column == 0 {
        return "A1".to_string();
    }
    a1::merge_ref((1, 1), (last_row, last_column))
}

/// One `<c>` element.
fn cell_xml(
    cell: &Cell,
    reference: &str,
    index: usize,
    strings: &SharedStrings,
) -> Result<String, SheetsError> {
    let style = cell.format.unwrap_or(NumberFormat::General);
    let style_attribute = if style == NumberFormat::General {
        String::new()
    } else {
        format!(" s=\"{}\"", style.style_index())
    };

    let refuse = |kind: &'static str| SheetsError::UnsupportedCell {
        index,
        cell: reference.to_string(),
        kind,
    };

    // The cached result of a formula is written as a value beside the formula, never as
    // another formula.
    let value = match &cell.value {
        CellValue::Date(date) => {
            let serial = serial::date_serial(*date).ok_or_else(|| {
                refuse("a date before 1900-01-01, which the 1900 date system cannot carry")
            })?;
            Value::Element(format!("<v>{serial}</v>"))
        }
        CellValue::DateTime(value) => {
            let serial = serial::date_time_serial(*value).ok_or_else(|| {
                refuse("a date-time before 1900-01-01, which the 1900 date system cannot carry")
            })?;
            Value::Element(format!("<v>{serial}</v>"))
        }
        CellValue::Number(number) => {
            if !number.is_finite() {
                return Err(refuse("a non-finite number"));
            }
            Value::Element(format!("<v>{number}</v>"))
        }
        CellValue::Bool(flag) => Value::Typed("b", format!("<v>{}</v>", u8::from(*flag))),
        CellValue::Text(text) => {
            if cell.formula.is_some() {
                Value::Typed("str", format!("<v>{}</v>", escape(text)))
            } else {
                let position = strings.position(text);
                Value::Typed("s", format!("<v>{position}</v>"))
            }
        }
        CellValue::Empty => Value::None,
    };

    let type_attribute = match &value {
        Value::Typed(kind, _) => format!(" t=\"{kind}\""),
        _ => String::new(),
    };
    let mut body = String::new();
    if let Some(formula) = &cell.formula {
        let formula = formula.trim().trim_start_matches('=');
        let _ = write!(body, "<f>{}</f>", escape(formula));
    }
    if let Value::Element(element) | Value::Typed(_, element) = &value {
        body.push_str(element);
    }
    Ok(format!(
        "<c r=\"{reference}\"{type_attribute}{style_attribute}>{body}</c>"
    ))
}

/// The body a cell writes.
enum Value {
    /// A typed value with its `t` attribute.
    Typed(&'static str, String),
    /// An untyped value (a number or a serial date).
    Element(String),
    /// Nothing, which is how a formula with an empty cached result is written.
    None,
}

fn relationships_document(body: &str) -> String {
    format!("{XML_DECL}<Relationships xmlns=\"{NS_REL}\">{body}</Relationships>")
}

fn relationship(id: &str, kind: &str, target: &str) -> String {
    format!("<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{target}\"/>")
}

/// Escape text for an XML text node or attribute.
///
/// Control characters XML 1.0 forbids are written in SpreadsheetML's `_xHHHH_` form, and a
/// literal `_x` is itself escaped so a cell that reads `_x000D_` cannot be mistaken for a
/// carriage return.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(character),
            '_' if characters.peek() == Some(&'x') => out.push_str("_x005F_"),
            control if (control as u32) < 0x20 => {
                let _ = write!(out, "_x{:04X}_", control as u32);
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cell, CellValue, Sheet, Workbook};

    fn workbook() -> Workbook {
        Workbook {
            sheets: vec![Sheet {
                name: "Data".to_string(),
                rows: vec![
                    vec![
                        Cell::new(CellValue::Text("name".into())),
                        Cell::new(CellValue::Number(42.0)),
                    ],
                    vec![
                        Cell::new(CellValue::Text("x".into())),
                        Cell::new(CellValue::Bool(true)),
                    ],
                ],
                column_widths: vec![18.5],
                frozen_rows: 1,
                merged: Vec::new(),
            }],
        }
    }

    /// The entry names, walked from the local file headers. The archive is stored, so a name is
    /// literal bytes at a fixed offset after each 30-byte header — no central-directory parsing
    /// is needed to answer "in what order were the parts written".
    fn entry_names(bytes: &[u8]) -> Vec<String> {
        let mut names = Vec::new();
        let mut at = 0usize;
        while at + 30 <= bytes.len() && bytes[at..at + 4] == *b"PK\x03\x04" {
            let name_len = u16::from_le_bytes([bytes[at + 26], bytes[at + 27]]) as usize;
            let extra_len = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
            let size = u32::from_le_bytes([
                bytes[at + 18],
                bytes[at + 19],
                bytes[at + 20],
                bytes[at + 21],
            ]) as usize;
            let name = String::from_utf8(bytes[at + 30..at + 30 + name_len].to_vec())
                .expect("entry names are UTF-8");
            names.push(name);
            at += 30 + name_len + extra_len + size;
        }
        names
    }

    #[test]
    fn the_parts_are_written_in_a_fixed_order() {
        let bytes = to_xlsx(&workbook()).expect("the workbook writes");
        assert_eq!(
            entry_names(&bytes),
            vec![
                "[Content_Types].xml",
                "_rels/.rels",
                "xl/workbook.xml",
                "xl/_rels/workbook.xml.rels",
                "xl/styles.xml",
                "xl/sharedStrings.xml",
                "xl/worksheets/sheet1.xml",
            ]
        );
    }

    #[test]
    fn a_text_cell_becomes_a_shared_string_once() {
        let bytes = to_xlsx(&workbook()).expect("the workbook writes");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("<si><t xml:space=\"preserve\">name</t></si>"));
        assert_eq!(text.matches("<si>").count(), 2);
        assert!(text.contains("<c r=\"A1\" t=\"s\"><v>0</v></c>"));
    }

    #[test]
    fn control_characters_are_escaped_and_a_literal_escape_is_neutralised() {
        assert_eq!(escape("a\u{0001}b"), "a_x0001_b");
        assert_eq!(escape("_x0041_"), "_x005F_x0041_");
        assert_eq!(escape("a\tb"), "a\tb");
    }

    #[test]
    fn a_formula_writes_its_cached_value() {
        let cell = Cell {
            value: CellValue::Number(3.0),
            formula: Some("=SUM(A1:A2)".to_string()),
            format: None,
        };
        let strings = SharedStrings {
            unique: Vec::new(),
            index: HashMap::new(),
            total: 0,
        };
        let xml = cell_xml(&cell, "B1", 1, &strings).expect("the cell writes");
        assert_eq!(xml, "<c r=\"B1\"><f>SUM(A1:A2)</f><v>3</v></c>");
    }
}
