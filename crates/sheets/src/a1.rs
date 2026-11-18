//! A1 notation: the one spelling cells and merged ranges use in a package and on the wire.

/// The column letters for a 1-based column number: `1 -> "A"`, `26 -> "Z"`, `27 -> "AA"`.
pub fn column_name(column: u32) -> String {
    let mut name = String::new();
    let mut rest = column;
    while rest > 0 {
        let offset = ((rest - 1) % 26) as u8;
        name.push(char::from(b'A' + offset));
        rest = (rest - 1) / 26;
    }
    name.chars().rev().collect()
}

/// The A1 reference for a 1-based `(row, column)`.
pub fn cell_ref(row: u32, column: u32) -> String {
    format!("{}{row}", column_name(column))
}

/// Parse an A1 reference into a 1-based `(row, column)`.
///
/// Returns `None` for anything a writer would not emit: no letters, no digits, a zero row or
/// column, or trailing characters.
pub fn parse_cell_ref(text: &str) -> Option<(u32, u32)> {
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() || digits.is_empty() {
        return None;
    }
    if !letters.chars().all(|c| c.is_ascii_uppercase()) {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut column = 0u32;
    for letter in letters.chars() {
        column = column.checked_mul(26)?;
        column = column.checked_add(u32::from(letter as u8 - b'A') + 1)?;
    }
    let row: u32 = digits.parse().ok()?;
    if row == 0 || column == 0 {
        return None;
    }
    Some((row, column))
}

/// The `A1:B2` spelling of a merged range given its two corners.
pub fn merge_ref(start: (u32, u32), end: (u32, u32)) -> String {
    format!("{}:{}", cell_ref(start.0, start.1), cell_ref(end.0, end.1))
}

/// Parse an `A1:B2` merged range into its two 1-based corners.
pub fn parse_merge_ref(text: &str) -> Option<((u32, u32), (u32, u32))> {
    let (start, end) = text.split_once(':')?;
    Some((parse_cell_ref(start)?, parse_cell_ref(end)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_round_trip_through_their_letters() {
        for (column, name) in [
            (1, "A"),
            (2, "B"),
            (26, "Z"),
            (27, "AA"),
            (52, "AZ"),
            (53, "BA"),
            (702, "ZZ"),
            (703, "AAA"),
            (1024, "AMJ"),
        ] {
            assert_eq!(column_name(column), name);
            assert_eq!(parse_cell_ref(&format!("{name}7")), Some((7, column)));
        }
    }

    #[test]
    fn a_ref_that_is_not_canonical_is_refused() {
        for text in ["", "A", "7", "a1", "A0", "1A", "A1B", "A 1", "Ä1"] {
            assert_eq!(parse_cell_ref(text), None, "{text} must be refused");
        }
        assert_eq!(parse_cell_ref("A1"), Some((1, 1)));
        assert_eq!(parse_cell_ref("XFD1048576"), Some((1_048_576, 16_384)));
    }

    #[test]
    fn merged_ranges_round_trip() {
        let range = merge_ref((1, 1), (3, 4));
        assert_eq!(range, "A1:D3");
        assert_eq!(parse_merge_ref(&range), Some(((1, 1), (3, 4))));
        assert_eq!(parse_merge_ref("A1"), None);
        assert_eq!(parse_merge_ref("A1:"), None);
    }
}
