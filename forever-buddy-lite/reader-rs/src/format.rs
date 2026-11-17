//! Formatting shared by the commands and the report: money, dates and ages.

use serde_json::{json, Value};

/// Whole copper, split: (sign, gold, silver, copper).
fn coins(copper: f64) -> (&'static str, i64, i64, i64) {
    let c = copper.round() as i64;
    let sign = if c < 0 { "-" } else { "" };
    let c = c.abs();
    (sign, c / 10_000, c % 10_000 / 100, c % 100)
}

/// Copper as "1g 2s 3c", always all three parts (the character commands' style).
pub fn money_long(copper: f64) -> String {
    let (sign, g, s, c) = coins(copper);
    format!("{sign}{g}g {s}s {c}c")
}

/// Copper as briefly as it reads: "2g 5s", "5s 07c", "42c" (the market report's style; copper is
/// left off gold amounts, it doesn't matter at that size).
pub fn money_short(copper: f64) -> String {
    let (sign, g, s, c) = coins(copper);
    if g != 0 {
        format!("{sign}{g}g {s}s")
    } else if s != 0 {
        format!("{sign}{s}s {c:02}c")
    } else {
        format!("{sign}{c}c")
    }
}

/// A change as a signed whole percentage: "+12%", "-30%", "0%".
pub fn percent_change(x: f64) -> String {
    let pct = (x * 100.0).round() as i64;
    format!("{}{pct}%", if pct > 0 { "+" } else { "" })
}

/// The current time in seconds since 1970.
pub fn now_seconds() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Days since 1970-01-01 to a (year, month, day) date (proleptic Gregorian).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

/// Milliseconds since 1970 as ISO 8601 UTC: "2026-11-16T13:35:00.000Z".
pub fn iso_from_millis(ms: f64) -> String {
    let ms = ms.trunc() as i64;
    let days = ms.div_euclid(86_400_000);
    let in_day = ms.rem_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    let (h, min, s, milli) = (in_day / 3_600_000, in_day / 60_000 % 60, in_day / 1000 % 60, in_day % 1000);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{milli:03}Z")
}

/// Seconds since 1970 as ISO 8601.
pub fn iso_from_seconds(sec: f64) -> String {
    iso_from_millis(sec * 1000.0)
}

/// A (year, month, day) date to days since 1970-01-01: the inverse of `civil_from_days`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if month > 2 { month - 3 } else { month + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// ISO 8601 UTC ("2026-11-16T20:34:03.000Z", fraction and Z optional) to seconds since 1970.
pub fn seconds_from_iso(s: &str) -> Option<f64> {
    let num = |range: std::ops::Range<usize>| s.get(range)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)? as u32, num(8..10)? as u32);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let fraction = s.get(19..).and_then(|f| f.strip_prefix('.')).map_or(0.0, |f| {
        let digits: String = f.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            0.0
        } else {
            digits.parse::<f64>().unwrap_or(0.0) / 10f64.powi(digits.len() as i32)
        }
    });
    Some((days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec) as f64 + fraction)
}

/// The first day Auctionator counts from: its price history is keyed by days since 2020-01-01.
const AUCTIONATOR_DAY0: i64 = 18262; // days from 1970-01-01 to 2020-01-01

/// An Auctionator day number as a date: "2026-11-14".
pub fn auctionator_date(day: i64) -> String {
    let (y, m, d) = civil_from_days(AUCTIONATOR_DAY0 + day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// How long ago, roughly: "5 min ago", "3 h ago", "4 days ago".
pub fn age(sec: f64, now: f64) -> String {
    let minutes = ((now - sec) / 60.0).round();
    if minutes < 60.0 {
        format!("{minutes} min ago")
    } else if minutes < 2880.0 {
        format!("{} h ago", (minutes / 60.0).round())
    } else {
        format!("{} days ago", (minutes / 1440.0).round())
    }
}

/// A timestamp as `{ at, age }`, or nothing when there is none (0 counts as none).
pub fn when(sec: Option<f64>, now: f64) -> Option<Value> {
    let sec = sec.filter(|&s| s != 0.0 && !s.is_nan())?;
    Some(json!({ "at": iso_from_seconds(sec), "age": age(sec, now) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_styles() {
        assert_eq!(money_long(125.0), "0g 1s 25c");
        assert_eq!(money_long(123456.0), "12g 34s 56c");
        assert_eq!(money_short(2100.0), "21s 00c");
        assert_eq!(money_short(3799.0), "37s 99c");
        assert_eq!(money_short(7445.0), "74s 45c");
        assert_eq!(money_short(50000.0), "5g 0s");
        assert_eq!(money_short(42.0), "42c");
        assert_eq!(money_short(105.4), "1s 05c");
    }

    #[test]
    fn percentages() {
        assert_eq!(percent_change(0.123), "+12%");
        assert_eq!(percent_change(-0.3), "-30%");
        assert_eq!(percent_change(-0.004), "0%");
    }

    #[test]
    fn dates() {
        assert_eq!(iso_from_seconds(0.0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_seconds(1794600000.0), "2026-11-13T20:00:00.000Z");
        assert_eq!(auctionator_date(0), "2020-01-01");
        assert_eq!(auctionator_date(2509), "2026-11-14");
        assert_eq!(iso_from_millis(951_782_400_000.0), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn ages() {
        assert_eq!(age(1000.0, 1000.0 + 5.0 * 60.0), "5 min ago");
        assert_eq!(age(0.0, 3.0 * 3600.0), "3 h ago");
        assert_eq!(age(0.0, 4.0 * 86400.0), "4 days ago");
        assert!(when(Some(0.0), 1.0).is_none());
    }
}
