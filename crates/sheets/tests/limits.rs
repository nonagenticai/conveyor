//! The caps, each with its own typed refusal: a caller must be told what was too big.

use conveyor_sheets::{
    Cell, CellValue, MAX_INPUT_BYTES, MAX_TEXT_CHARS, Sheet, Workbook, from_xlsx, to_xlsx,
};

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
fn oversized_cell_count_is_a_typed_refusal() {
    // 1,000 rows × 300 columns = 300,000 cells: under the row and column caps, over the cell cap.
    let row: Vec<Cell> = (0..300)
        .map(|_| Cell::new(CellValue::Number(1.0)))
        .collect();
    let workbook = Workbook {
        sheets: vec![sheet("Wide", vec![row; 1_000])],
    };
    let error = to_xlsx(&workbook).expect_err("over the cell cap");
    assert_eq!(error.code(), "too_large");
    assert!(error.to_string().contains("non-empty cells"), "{error}");
}

#[test]
fn export_over_the_byte_ceiling_is_a_typed_refusal() {
    // 400 distinct texts of the maximum length: ~13 MiB of package, over the 10 MiB ceiling and
    // still under the cell cap.
    let rows: Vec<Vec<Cell>> = (0..400)
        .map(|row| {
            vec![Cell::new(CellValue::Text(format!(
                "{row:04}{}",
                "x".repeat(MAX_TEXT_CHARS - 4)
            )))]
        })
        .collect();
    let workbook = Workbook {
        sheets: vec![sheet("Big", rows)],
    };
    let error = to_xlsx(&workbook).expect_err("over the package ceiling");
    assert_eq!(error.code(), "package_too_large");
}

#[test]
fn import_over_byte_cap_is_refused() {
    let bytes = vec![0u8; MAX_INPUT_BYTES + 1];
    let error = from_xlsx(&bytes).expect_err("over the input cap");
    assert_eq!(error.code(), "input_too_large");
}

#[test]
fn bytes_that_are_not_a_workbook_are_refused() {
    let error = from_xlsx(b"this is not a zip").expect_err("not a package");
    assert_eq!(error.code(), "unreadable");
}

#[test]
fn duplicate_sheet_name_is_refused() {
    let workbook = Workbook {
        sheets: vec![sheet("Data", Vec::new()), sheet("DATA", Vec::new())],
    };
    let error = to_xlsx(&workbook).expect_err("two tabs, one name");
    assert_eq!(error.code(), "duplicate_sheet_name");
}
