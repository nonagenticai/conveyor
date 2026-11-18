//! Reading a workbook this crate did not write.
//!
//! The fixture is built here, by hand and with **deflated** parts, exactly so the reader's
//! inflate/relationship/style path is exercised by a producer other than `conveyor-sheets`'
//! own stored-ZIP writer. Nothing binary is checked in.

use std::io::Write;

use conveyor_sheets::{CellValue, Date, MergeRange, from_xlsx};
use flate2::Compression;
use flate2::write::DeflateEncoder;

/// One archive entry: its name and its uncompressed bytes.
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    for (name, data) in entries {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).expect("deflate writes");
        let compressed = encoder.finish().expect("deflate finishes");
        let crc = conveyor_zipstore::crc32(data);
        let offset = out.len() as u32;
        let name_len = name.len() as u16;

        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&8u16.to_le_bytes()); // deflate
        out.extend_from_slice(&0u16.to_le_bytes()); // time
        out.extend_from_slice(&0x0021u16.to_le_bytes()); // 1980-01-01
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&compressed);

        directory.extend_from_slice(b"PK\x01\x02");
        directory.extend_from_slice(&20u16.to_le_bytes());
        directory.extend_from_slice(&20u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&8u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0x0021u16.to_le_bytes());
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_le_bytes());
        directory.extend_from_slice(&name_len.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u32.to_le_bytes());
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }
    let count = entries.len() as u16;
    let directory_offset = out.len() as u32;
    let directory_size = directory.len() as u32;
    out.extend_from_slice(&directory);
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#;

const WORKBOOK: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets><sheet name="Foreign" sheetId="1" r:id="rId1"/><sheet name="Empty" sheetId="2" r:id="rId2"/></sheets>
</workbook>"#;

const WORKBOOK_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
</Relationships>"#;

/// Two cell formats: general (0) and the built-in date format 14. A cell that carries `s="1"`
/// is a date because the *style* says so, not because of its number.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<numFmts count="1"><numFmt numFmtId="164" formatCode="0.00&quot; kg&quot;"/></numFmts>
<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>
<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>
<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>
<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
<cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs>
</styleSheet>"#;

const SHARED_STRINGS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="2" uniqueCount="2">
<si><t xml:space="preserve">label</t></si><si><t xml:space="preserve">value</t></si>
</sst>"#;

/// A2 is a number, B2 a date (serial 45292 = 2024-01-01), C2 a formula with a cached result,
/// and A3:B3 is merged with no values under it — what a real report looks like.
const SHEET1: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
<row r="2"><c r="A2"><v>42</v></c><c r="B2" s="1"><v>45292</v></c><c r="C2"><f>SUM(A2:A2)</f><v>42</v></c><c r="D2" s="2"><v>12.5</v></c></row>
</sheetData>
<mergeCells count="1"><mergeCell ref="A3:B3"/></mergeCells>
</worksheet>"#;

const SHEET2: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#;

/// A workbook as another producer writes it: deflated parts, shared strings, a style-resolved
/// date, a formula and a merged range.
fn foreign_workbook() -> Vec<u8> {
    zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", WORKBOOK.as_bytes()),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS.as_bytes()),
        ("xl/styles.xml", STYLES.as_bytes()),
        ("xl/sharedStrings.xml", SHARED_STRINGS.as_bytes()),
        ("xl/worksheets/sheet1.xml", SHEET1.as_bytes()),
        ("xl/worksheets/sheet2.xml", SHEET2.as_bytes()),
    ])
}

#[test]
fn imports_formulas_dates_and_merged_cells() {
    let workbook = from_xlsx(&foreign_workbook()).expect("a foreign workbook reads");
    assert_eq!(
        workbook
            .sheets
            .iter()
            .map(|sheet| sheet.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Foreign", "Empty"]
    );

    let rows = &workbook.sheets[0].rows;
    assert_eq!(rows[0][0].value, CellValue::Text("label".into()));
    assert_eq!(rows[0][1].value, CellValue::Text("value".into()));
    assert_eq!(rows[1][0].value, CellValue::Number(42.0));
    assert_eq!(
        rows[1][1].value,
        CellValue::Date(Date {
            year: 2024,
            month: 1,
            day: 1
        }),
        "the date is read from the style, not from the number"
    );
    assert_eq!(rows[1][2].value, CellValue::Number(42.0));
    assert_eq!(rows[1][2].formula.as_deref(), Some("SUM(A2:A2)"));
    assert_eq!(
        workbook.sheets[0].merged,
        vec![MergeRange {
            start_row: 3,
            start_column: 1,
            end_row: 3,
            end_column: 2
        }]
    );
    assert!(
        workbook.sheets[1].rows.is_empty(),
        "an empty sheet stays empty"
    );
}

/// A custom format id is a documented loss: the cell reads as General, never as an invented
/// nearest format, and the workbook still opens.
#[test]
fn an_unknown_format_id_is_general() {
    let workbook = from_xlsx(&foreign_workbook()).expect("a foreign workbook reads");
    let cell = &workbook.sheets[0].rows[1][3];
    assert_eq!(cell.value, CellValue::Number(12.5));
    assert_eq!(cell.format, None, "an unmapped id is General, not a guess");
}
