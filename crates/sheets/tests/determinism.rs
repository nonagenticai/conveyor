//! Determinism: the package bytes are a pure function of the workbook.

use conveyor_sheets::{Cell, CellValue, Sheet, Workbook, to_xlsx};

fn workbook() -> Workbook {
    Workbook {
        sheets: vec![
            Sheet {
                name: "First".to_string(),
                rows: vec![
                    vec![
                        Cell::new(CellValue::Text("region".into())),
                        Cell::new(CellValue::Text("region".into())),
                        Cell::new(CellValue::Number(1.5)),
                    ],
                    vec![
                        Cell::new(CellValue::Bool(true)),
                        Cell::new(CellValue::Text("north".into())),
                        Cell::new(CellValue::Empty),
                    ],
                ],
                column_widths: vec![12.0, 8.5],
                frozen_rows: 1,
                merged: Vec::new(),
            },
            Sheet {
                name: "Second".to_string(),
                rows: vec![vec![Cell::new(CellValue::Text("region".into()))]],
                column_widths: Vec::new(),
                frozen_rows: 0,
                merged: Vec::new(),
            },
        ],
    }
}

/// The entry names, walked from the local file headers: stored entries put the name at a fixed
/// offset after each 30-byte header.
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
        names.push(String::from_utf8(bytes[at + 30..at + 30 + name_len].to_vec()).expect("utf-8"));
        // Method 0 (stored) is what the writer emits, so the body is `size` bytes.
        at += 30 + name_len + extra_len + size;
    }
    names
}

#[test]
fn two_exports_are_byte_identical() {
    let first = to_xlsx(&workbook()).expect("first export");
    let second = to_xlsx(&workbook()).expect("second export");
    assert_eq!(first, second);
    // Two *values* built separately, not two clones of one value.
    let third = to_xlsx(&workbook()).expect("third export");
    assert_eq!(first, third);
}

#[test]
fn part_order_and_zip_timestamps_are_fixed() {
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
            "xl/worksheets/sheet2.xml",
        ]
    );
    // Every local header carries DOS date 0x0021 (1980-01-01) and time 0.
    let mut at = 0usize;
    while at + 30 <= bytes.len() && bytes[at..at + 4] == *b"PK\x03\x04" {
        let time = u16::from_le_bytes([bytes[at + 10], bytes[at + 11]]);
        let date = u16::from_le_bytes([bytes[at + 12], bytes[at + 13]]);
        assert_eq!((time, date), (0, 0x0021), "entry at {at}");
        let name_len = u16::from_le_bytes([bytes[at + 26], bytes[at + 27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
        let size = u32::from_le_bytes([
            bytes[at + 18],
            bytes[at + 19],
            bytes[at + 20],
            bytes[at + 21],
        ]) as usize;
        at += 30 + name_len + extra_len + size;
    }
}

#[test]
fn shared_strings_are_first_seen_and_deduped() {
    let bytes = to_xlsx(&workbook()).expect("the workbook writes");
    let text = String::from_utf8_lossy(&bytes);
    let strings = &text[text.find("xl/sharedStrings.xml").expect("the part name")..];
    let region = strings.find("<si><t xml:space=\"preserve\">region</t></si>");
    let north = strings.find("<si><t xml:space=\"preserve\">north</t></si>");
    assert!(
        region.is_some() && north.is_some(),
        "both strings are present"
    );
    assert!(region < north, "first-seen order is workbook order");
    assert_eq!(
        strings.matches(">region<").count(),
        1,
        "the repeated string is stored once"
    );
    assert!(strings.contains("uniqueCount=\"2\""));
    // Four text cells referenced the two strings.
    assert!(strings.contains("count=\"4\""));
}
