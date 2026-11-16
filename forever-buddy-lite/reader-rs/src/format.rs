//! Formatting shared by the commands and the report: money, dates and ages.

use crate::json::{js_round, number_string};
use serde_json::{json, Value};

/// Copper as "1g 2s 3c", always all three parts (the character commands' style).
pub fn money_long(copper: f64) -> String {
    let gold = (copper / 10000.0).floor();
    let silver = ((copper % 10000.0) / 100.0).floor();
    let rest = copper % 100.0;
    format!("{}g {}s {}c", number_string(gold), number_string(silver), number_string(rest))
}

/// `money_long` for an optional amount: none stays none (absent in JSON).
pub fn money_long_opt(copper: Option<f64>) -> Option<String> {
    copper.map(money_long)
}

/// Copper as briefly as it reads: "2g 5s", "5s 07c", "42c" (the market report's style; copper is
/// left off gold amounts, it doesn't matter at that size).
pub fn money_short(copper: f64) -> String {
    let c = js_round(copper);
    let gold = (c / 1e4).floor();
    let silver = ((c % 1e4) / 100.0).floor();
    let rest = c % 100.0;
    if gold != 0.0 {
        format!("{}g {}s", number_string(gold), number_string(silver))
    } else if silver != 0.0 {
        format!("{}s {:0>2}c", number_string(silver), number_string(rest))
    } else {
        format!("{}c", number_string(rest))
    }
}

/// A change as a signed whole percentage: "+12%", "-30%", "0%".
pub fn percent_change(x: f64) -> String {
    let sign = if x > 0.0 { "+" } else { "" };
    format!("{sign}{}%", number_string(js_round(x * 100.0) + 0.0))
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

/// The first day Auctionator counts from: its price history is keyed by days since 2020-01-01.
const AUCTIONATOR_DAY0: i64 = 18262; // days from 1970-01-01 to 2020-01-01

/// An Auctionator day number as a date: "2026-11-14".
pub fn auctionator_date(day: i64) -> String {
    let (y, m, d) = civil_from_days(AUCTIONATOR_DAY0 + day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// How long ago, roughly: "5 min ago", "3 h ago", "4 days ago".
pub fn age(sec: f64, now: f64) -> String {
    let minutes = js_round((now - sec) / 60.0);
    if minutes < 60.0 {
        format!("{} min ago", number_string(minutes))
    } else if minutes < 2880.0 {
        format!("{} h ago", number_string(js_round(minutes / 60.0)))
    } else {
        format!("{} days ago", number_string(js_round(minutes / 1440.0)))
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
