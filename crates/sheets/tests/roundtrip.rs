//! The round-trip property: what `to_xlsx` writes, `from_xlsx` reads back.
//!
//! Presentation is deliberately outside the property. A foreign workbook sets column widths and
//! frozen rows in ways this model does not model, so an import returns defaults for them; the
//! values, formulas, cached results, dates, merged ranges, names and order are what must survive.

use conveyor_sheets::{
    Cell, CellValue, Date, DateTime, MergeRange, NumberFormat, Sheet, Workbook, from_xlsx, to_xlsx,
};
use proptest::prelude::*;

/// One cell as the property compares it: its value, its formula and its format.
type CellParts = (CellValue, Option<String>, Option<NumberFormat>);

/// One sheet as the property compares it: its name, its cells and its merged ranges.
type SheetParts = (String, Vec<Vec<CellParts>>, Vec<MergeRange>);

/// The part of a workbook the round-trip promises: everything but presentation.
fn values(workbook: &Workbook) -> Vec<SheetParts> {
    workbook
        .sheets
        .iter()
        .map(|sheet| {
            (
                sheet.name.clone(),
                sheet
                    .rows
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|cell| (cell.value.clone(), cell.formula.clone(), cell.format))
                            .collect()
                    })
                    .collect(),
                sheet.merged.clone(),
            )
        })
        .collect()
}

fn date() -> impl Strategy<Value = Date> {
    (1900i32..2100, 1u8..=12, 1u8..=28).prop_map(|(year, month, day)| Date { year, month, day })
}

fn date_time() -> impl Strategy<Value = DateTime> {
    (1900i32..2100, 1u8..=12, 1u8..=28, 0u8..24, 0u8..60, 0u8..60).prop_map(
        |(year, month, day, hour, minute, second)| DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        },
    )
}

fn cell() -> impl Strategy<Value = Cell> {
    prop_oneof![
        Just(Cell::new(CellValue::Empty)),
        any::<bool>().prop_map(|flag| Cell::new(CellValue::Bool(flag))),
        (-999_999i64..999_999).prop_map(|number| Cell::new(CellValue::Number(number as f64))),
        "[a-zA-Z0-9 -]{0,12}".prop_map(|text| Cell::new(CellValue::Text(text))),
        date().prop_map(|value| Cell::new(CellValue::Date(value))),
        date_time().prop_map(|value| Cell::new(CellValue::DateTime(value))),
    ]
}

fn workbook() -> impl Strategy<Value = Workbook> {
    (1usize..4)
        .prop_flat_map(|sheets| {
            prop::collection::vec(
                (
                    "[A-Za-z][A-Za-z0-9 ]{0,8}",
                    prop::collection::vec(prop::collection::vec(cell(), 0..5), 0..5),
                )
                    .prop_map(|(name, rows)| {
                        // Distinct names: Excel matches tabs case-insensitively.
                        let name = name.trim().to_string();
                        Sheet {
                            name: if name.is_empty() {
                                "S".to_string()
                            } else {
                                name
                            },
                            rows,
                            column_widths: Vec::new(),
                            frozen_rows: 0,
                            merged: Vec::new(),
                        }
                    }),
                sheets,
            )
        })
        .prop_map(|sheets| {
            let mut seen: Vec<String> = Vec::new();
            let mut unique = Vec::new();
            for sheet in sheets {
                let mut name = sheet.name.clone();
                let mut suffix = 1;
                while seen.iter().any(|other| other.eq_ignore_ascii_case(&name)) {
                    name = format!("{}{suffix}", sheet.name);
                    suffix += 1;
                }
                seen.push(name.clone());
                unique.push(Sheet { name, ..sheet });
            }
            Workbook { sheets: unique }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn export_import_equals_workbook(input in workbook()) {
        let canonical = input.clone().canonical().expect("generated workbooks are valid");
        let bytes = to_xlsx(&input).expect("the workbook writes");
        let read = from_xlsx(&bytes).expect("our own package reads");
        prop_assert_eq!(values(&read), values(&canonical));
    }
}

#[test]
fn formula_cached_results_survive() {
    let workbook = Workbook {
        sheets: vec![Sheet {
            name: "Calc".to_string(),
            rows: vec![vec![
                Cell::new(CellValue::Number(2.0)),
                Cell::new(CellValue::Number(3.0)),
                Cell {
                    value: CellValue::Number(5.0),
                    formula: Some("SUM(A1:B1)".to_string()),
                    format: None,
                },
                Cell {
                    value: CellValue::Text("ok".to_string()),
                    formula: Some("IF(C1=5,\"ok\",\"no\")".to_string()),
                    format: None,
                },
            ]],
            column_widths: Vec::new(),
            frozen_rows: 0,
            merged: Vec::new(),
        }],
    };
    let bytes = to_xlsx(&workbook).expect("the workbook writes");
    let read = from_xlsx(&bytes).expect("it reads back");
    let cells = &read.sheets[0].rows[0];
    assert_eq!(cells[2].value, CellValue::Number(5.0));
    assert_eq!(cells[2].formula.as_deref(), Some("SUM(A1:B1)"));
    assert_eq!(cells[3].value, CellValue::Text("ok".to_string()));
    assert_eq!(cells[3].formula.as_deref(), Some("IF(C1=5,\"ok\",\"no\")"));
}

#[test]
fn dates_and_number_formats_survive() {
    let workbook = Workbook {
        sheets: vec![Sheet {
            name: "Dates".to_string(),
            rows: vec![
                vec![
                    Cell::new(CellValue::Date(Date {
                        year: 2026,
                        month: 11,
                        day: 18,
                    })),
                    Cell::new(CellValue::DateTime(DateTime {
                        year: 2026,
                        month: 11,
                        day: 18,
                        hour: 13,
                        minute: 45,
                        second: 59,
                    })),
                    Cell {
                        value: CellValue::Number(0.25),
                        formula: None,
                        format: Some(NumberFormat::Percent),
                    },
                ],
                vec![Cell::new(CellValue::Date(Date {
                    year: 1900,
                    month: 1,
                    day: 1,
                }))],
            ],
            column_widths: Vec::new(),
            frozen_rows: 0,
            merged: Vec::new(),
        }],
    };
    let bytes = to_xlsx(&workbook).expect("the workbook writes");
    let read = from_xlsx(&bytes).expect("it reads back");
    assert_eq!(
        read.sheets[0].rows[0][0].value,
        CellValue::Date(Date {
            year: 2026,
            month: 11,
            day: 18
        })
    );
    assert_eq!(
        read.sheets[0].rows[0][1].value,
        CellValue::DateTime(DateTime {
            year: 2026,
            month: 11,
            day: 18,
            hour: 13,
            minute: 45,
            second: 59
        })
    );
    assert_eq!(
        read.sheets[0].rows[0][2].format,
        Some(NumberFormat::Percent),
        "a percentage is the number plus its format"
    );
    assert_eq!(read.sheets[0].rows[0][2].value, CellValue::Number(0.25));
    // A date in the 1900 bug window (before March) still names the right day.
    assert_eq!(
        read.sheets[0].rows[1][0].value,
        CellValue::Date(Date {
            year: 1900,
            month: 1,
            day: 1
        })
    );
}

#[test]
fn merges_and_sheet_order_survive() {
    let sheet = |name: &str| Sheet {
        name: name.to_string(),
        rows: vec![vec![Cell::new(CellValue::Text("title".into())); 3]; 2],
        column_widths: Vec::new(),
        frozen_rows: 0,
        merged: vec![MergeRange {
            start_row: 1,
            start_column: 1,
            end_row: 1,
            end_column: 2,
        }],
    };
    let workbook = Workbook {
        sheets: vec![sheet("Second"), sheet("First")],
    };
    let bytes = to_xlsx(&workbook).expect("the workbook writes");
    let read = from_xlsx(&bytes).expect("it reads back");
    assert_eq!(
        read.sheets
            .iter()
            .map(|sheet| sheet.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Second", "First"]
    );
    assert_eq!(read.sheets[0].merged, workbook.sheets[0].merged);
}

/// Every format the writer can emit survives the round trip, and the second export is the same
/// bytes: a format is part of the artefact, not a display hint applied afterwards.
#[test]
fn formats_survive_export_and_import() {
    let workbook = Workbook {
        sheets: vec![Sheet {
            name: "Formats".to_string(),
            rows: vec![vec![
                Cell {
                    value: CellValue::Number(3.0),
                    formula: None,
                    format: Some(NumberFormat::Integer),
                },
                Cell {
                    value: CellValue::Number(1234.5678),
                    formula: None,
                    format: Some(NumberFormat::Decimal),
                },
                Cell {
                    value: CellValue::Number(0.25),
                    formula: None,
                    format: Some(NumberFormat::Percent),
                },
                Cell {
                    value: CellValue::Text("as text".to_string()),
                    formula: None,
                    format: Some(NumberFormat::Text),
                },
                Cell::new(CellValue::Date(Date {
                    year: 2026,
                    month: 11,
                    day: 18,
                })),
                Cell::new(CellValue::DateTime(DateTime {
                    year: 2026,
                    month: 11,
                    day: 18,
                    hour: 6,
                    minute: 30,
                    second: 0,
                })),
            ]],
            column_widths: Vec::new(),
            frozen_rows: 0,
            merged: Vec::new(),
        }],
    };
    let bytes = to_xlsx(&workbook).expect("the workbook writes");
    let read = from_xlsx(&bytes).expect("it reads back");
    let formats: Vec<Option<NumberFormat>> = read.sheets[0].rows[0]
        .iter()
        .map(|cell| cell.format)
        .collect();
    assert_eq!(
        formats,
        vec![
            Some(NumberFormat::Integer),
            Some(NumberFormat::Decimal),
            Some(NumberFormat::Percent),
            Some(NumberFormat::Text),
            Some(NumberFormat::Date),
            Some(NumberFormat::DateTime),
        ]
    );
    let again = to_xlsx(&read).expect("the imported workbook writes");
    assert_eq!(again, bytes, "the round trip is byte-identical");
}
