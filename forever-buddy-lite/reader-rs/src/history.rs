//! Every archived scan, by item: what the cheapest listing was and how many were up, at each scan.
//!
//! Auctionator folds a day into one low and one high, so a day of scans shows as one point. `fbl-rs
//! live` archives each scan that reaches disk (`<archive>/<time>.json.gz`, `{ at, items: { id:
//! [cheapest, listed] } }`); this reads them back so the dashboard can chart an item scan by scan.
//! Kept compact (12 bytes a point): a few hundred scans of ~7000 items is a few tens of MB at most.

use crate::format::seconds_from_iso;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// One item at one scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    /// Unix seconds.
    pub at: u32,
    /// The cheapest listing, copper.
    pub price: u32,
    /// How many were up.
    pub listed: u32,
}

#[derive(Debug, Default)]
pub struct ScanHistory {
    by_item: HashMap<u32, Vec<Point>>,
    scans: usize,
}

impl ScanHistory {
    /// Reads every `*.json.gz` scan in `dir`, oldest first (the file names sort by time). A missing
    /// folder is an empty history; an unreadable file is skipped.
    pub fn load(dir: &Path) -> ScanHistory {
        let mut history = ScanHistory::default();
        let Ok(entries) = std::fs::read_dir(dir) else { return history };
        let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(".json.gz")).collect();
        files.sort();
        for file in files {
            if let Some((at, items)) = read_scan(&file) {
                history.add(at, items);
            }
        }
        history
    }

    /// Adds one scan: `(item id, cheapest, listed)` for each item in it.
    pub fn add(&mut self, at: f64, items: impl IntoIterator<Item = ScanRow>) {
        let at = at.max(0.0) as u32;
        for (id, price, listed) in items {
            let point = Point { at, price: price.clamp(0, u32::MAX as i64) as u32, listed: listed.clamp(0, u32::MAX as i64) as u32 };
            self.by_item.entry(id).or_default().push(point);
        }
        self.scans += 1;
    }

    pub fn scans(&self) -> usize {
        self.scans
    }

    pub fn points(&self, id: u32) -> &[Point] {
        self.by_item.get(&id).map_or(&[], Vec::as_slice)
    }

    /// `/history.json?id=..`: `{ id, scans, points: [[at, cheapest, listed], ...] }`, oldest first.
    pub fn item_json(&self, id: u32) -> Value {
        let points: Vec<Value> = self.points(id).iter().map(|p| json!([p.at, p.price, p.listed])).collect();
        json!({ "id": id, "scans": self.scans, "points": points })
    }
}

/// One item in a scan: (id, cheapest, listed).
type ScanRow = (u32, i64, i64);

/// One archived scan: when (unix seconds), and its rows.
fn read_scan(file: &Path) -> Option<(f64, Vec<ScanRow>)> {
    let bytes = std::fs::read(file).ok()?;
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes.as_slice()).read_to_string(&mut text).ok()?;
    let scan: Value = serde_json::from_str(&text).ok()?;
    let at = seconds_from_iso(scan.get("at")?.as_str()?)?;
    let items = scan
        .get("items")?
        .as_object()?
        .iter()
        .filter_map(|(id, v)| Some((id.parse().ok()?, v.get(0)?.as_i64()?, v.get(1).and_then(Value::as_i64).unwrap_or(0))))
        .collect();
    Some((at, items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_scan(dir: &Path, name: &str, body: &str) {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(body.as_bytes()).unwrap();
        std::fs::write(dir.join(name), gz.finish().unwrap()).unwrap();
    }

    #[test]
    fn loads_scans_in_time_order_and_adds_new_ones() {
        let dir = std::env::temp_dir().join(format!("fbl-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Written out of order; read by name, which sorts by time.
        write_scan(&dir, "2026-11-16-21-00-00.json.gz", r#"{"at":"2026-11-16T21:00:00.000Z","items":{"7":[150,40]}}"#);
        write_scan(&dir, "2026-11-16-20-00-00.json.gz", r#"{"at":"2026-11-16T20:00:00.000Z","items":{"7":[100,50],"8":[9,1]}}"#);
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let mut h = ScanHistory::load(&dir);
        assert_eq!(h.scans(), 2);
        let t = seconds_from_iso("2026-11-16T20:00:00Z").unwrap() as u32;
        assert_eq!(h.points(7), [Point { at: t, price: 100, listed: 50 }, Point { at: t + 3600, price: 150, listed: 40 }]);
        h.add((t + 7200) as f64, [(7, 120, 45)]);
        assert_eq!(h.item_json(7)["points"][2], json!([t + 7200, 120, 45]));
        assert!(h.points(99).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn iso_round_trip() {
        let s = seconds_from_iso("2026-11-16T20:34:03.250Z").unwrap();
        assert_eq!(crate::format::iso_from_seconds(s), "2026-11-16T20:34:03.250Z");
        assert_eq!(seconds_from_iso("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(seconds_from_iso("nope"), None);
    }
}
