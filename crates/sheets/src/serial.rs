//! Excel serial dates: the conversion the writer needs.
//!
//! A workbook stores a date as a number of days from a system epoch, with a time as the
//! fractional part. The 1900 system carries Lotus 1-2-3's leap-year bug: serial 60 is
//! `1900-02-29`, a date that never existed. So the epoch is not a single subtraction — serials
//! below 60 are one day off from serials above it. That rule is here, in one place, because
//! getting it wrong shifts every date before March 1900 by a day.
//!
//! Only the forward direction lives here: reading a serial back is `calamine`'s job, and it
//! implements the same rule in its own converter.

use crate::model::{Date, DateTime};

/// Days from 1899-12-30 to 1970-01-01 in the 1900 system.
const DAYS_1899_12_30_TO_EPOCH: i64 = 25_569;

/// Days since 1970-01-01 for a proleptic-Gregorian date (Howard Hinnant's `days_from_civil`).
pub(crate) fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let mut year = i64::from(year);
    let month = i64::from(month);
    let day = i64::from(day);
    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The 1900-system serial for a date, or `None` before 1900-01-01 (which Excel cannot carry).
pub(crate) fn date_serial(date: Date) -> Option<i64> {
    let days = days_from_civil(date.year, date.month, date.day);
    let mut serial = days + DAYS_1899_12_30_TO_EPOCH;
    if (date.year, date.month, date.day) < (1900, 3, 1) {
        // Before the phantom day the serial counts from 1899-12-31, not 1899-12-30.
        serial -= 1;
    }
    (serial >= 1).then_some(serial)
}

/// The 1900-system serial for a date and time, with the time as the fraction of a day.
pub(crate) fn date_time_serial(value: DateTime) -> Option<f64> {
    let date = Date {
        year: value.year,
        month: value.month,
        day: value.day,
    };
    let serial = date_serial(date)?;
    let seconds =
        u32::from(value.hour) * 3_600 + u32::from(value.minute) * 60 + u32::from(value.second);
    Some(serial as f64 + f64::from(seconds) / 86_400.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_and_the_1900_bug_are_both_right() {
        let date = |year, month, day| Date { year, month, day };
        assert_eq!(date_serial(date(1970, 1, 1)), Some(25_569));
        assert_eq!(date_serial(date(1900, 1, 1)), Some(1));
        assert_eq!(date_serial(date(1900, 2, 28)), Some(59));
        // 1900-03-01 is 61 because serial 60 is the phantom day.
        assert_eq!(date_serial(date(1900, 3, 1)), Some(61));
        assert_eq!(date_serial(date(1899, 12, 31)), None);
        assert_eq!(date_serial(date(2000, 2, 29)), Some(36_585));
    }

    #[test]
    fn a_time_is_the_fraction_of_its_day() {
        let midnight = DateTime {
            year: 2026,
            month: 11,
            day: 18,
            hour: 0,
            minute: 0,
            second: 0,
        };
        let serial = date_serial(Date {
            year: 2026,
            month: 11,
            day: 18,
        })
        .expect("a writable date");
        assert_eq!(date_time_serial(midnight), Some(serial as f64));
        let noon = DateTime {
            hour: 12,
            ..midnight
        };
        assert_eq!(date_time_serial(noon), Some(serial as f64 + 0.5));
        let six = DateTime {
            hour: 6,
            minute: 0,
            second: 0,
            ..midnight
        };
        assert_eq!(date_time_serial(six), Some(serial as f64 + 0.25));
    }

    #[test]
    fn civil_days_have_their_known_anchors() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 2), 1);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2000, 1, 1), 10_957);
    }
}
