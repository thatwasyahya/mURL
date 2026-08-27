//! Time abstraction and a strict RFC 3339 subset parser.
//!
//! The core crate takes wall-clock time through the [`Clock`] trait so that
//! expiry and cache-freshness logic is deterministic under test. The only
//! timestamp format accepted anywhere in a manifest is the strict UTC form
//! `YYYY-MM-DDTHH:MM:SSZ` — no offsets, no fractional seconds. One format
//! means one parser, and one parser means one set of bugs to fuzz.

use std::time::{SystemTime, UNIX_EPOCH};

/// Source of "now" as seconds since the Unix epoch.
pub trait Clock: std::fmt::Debug {
    fn now_epoch(&self) -> u64;
}

/// The real system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_epoch(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// A fixed clock for tests.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now_epoch(&self) -> u64 {
        self.0
    }
}

/// Parse `YYYY-MM-DDTHH:MM:SSZ` (strict) into epoch seconds.
pub fn parse_rfc3339_utc(s: &str) -> Result<u64, String> {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return Err(format!(
            "`{s}` is not a strict UTC timestamp (expected YYYY-MM-DDTHH:MM:SSZ)"
        ));
    }
    let num = |range: std::ops::Range<usize>| -> Result<i64, String> {
        let part = &s[range];
        if !part.bytes().all(|c| c.is_ascii_digit()) {
            return Err(format!("`{s}` contains a non-digit in a numeric field"));
        }
        part.parse::<i64>().map_err(|e| e.to_string())
    };
    let year = num(0..4)?;
    let month = num(5..7)?;
    let day = num(8..10)?;
    let hour = num(11..13)?;
    let minute = num(14..16)?;
    let second = num(17..19)?;

    if !(1970..=9999).contains(&year) {
        return Err(format!("year {year} out of range 1970..=9999"));
    }
    if !(1..=12).contains(&month) {
        return Err(format!("month {month} out of range"));
    }
    if day < 1 || day > days_in_month(year, month) {
        return Err(format!("day {day} out of range for {year}-{month:02}"));
    }
    if hour > 23 || minute > 59 || second > 59 {
        return Err(format!(
            "time {hour:02}:{minute:02}:{second:02} out of range"
        ));
    }

    let days = days_from_civil(year, month, day);
    Ok((days * 86_400 + hour * 3_600 + minute * 60 + second) as u64)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_zero() {
        assert_eq!(parse_rfc3339_utc("1970-01-01T00:00:00Z").unwrap(), 0);
    }

    #[test]
    fn known_timestamps() {
        assert_eq!(
            parse_rfc3339_utc("2000-01-01T00:00:00Z").unwrap(),
            946_684_800
        );
        assert_eq!(
            parse_rfc3339_utc("2026-08-27T12:00:00Z").unwrap(),
            1_787_832_000
        );
        // Leap day.
        assert_eq!(
            parse_rfc3339_utc("2024-02-29T00:00:00Z").unwrap(),
            1_709_164_800
        );
    }

    #[test]
    fn rejects_lenient_forms() {
        for bad in [
            "2026-08-27T12:00:00+00:00",
            "2026-08-27T12:00:00.000Z",
            "2026-08-27 12:00:00Z",
            "2026-8-27T12:00:00Z",
            "2026-02-30T00:00:00Z",
            "2023-02-29T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-00-01T00:00:00Z",
            "2026-08-27T24:00:00Z",
            "1969-12-31T23:59:59Z",
            "garbage",
            "",
        ] {
            assert!(parse_rfc3339_utc(bad).is_err(), "should reject {bad}");
        }
    }
}
