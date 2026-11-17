//! Snipe targets from your own scans: items whose cheapest listing keeps dipping well under its
//! recent level and then gets bought back.
//!
//! Every archived scan (see `history`) records each item's cheapest listing. A **dip** is a scan
//! whose cheapest is at least `depth` under the median of the scans before it. Two things can follow:
//! - the next change brings the price back near that median: someone bought the cheap listing.
//!   That's a snipe market: cheap listings turn up and they sell. Counted as **bought back**.
//! - the price stays down: the market moved to a new level. Not a snipe; not counted.
//!
//! Items are ranked by how many dips were bought back, and each gets a cap under its recent median
//! with the same per-item profit rule as the daily list (`snipes`).
//!
//! What the archive can't see: a listing that appears and is bought between two of your scans.
//! The more often you scan, the more of these it catches.

use crate::history::{Point, ScanHistory};
use crate::snipes::SnipeOptions;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct DipOptions {
    /// Look back this many seconds.
    pub window: f64,
    /// A dip is at least this far under the recent median (0.25 = 25%).
    pub depth: f64,
    /// Keep items whose dips were bought back at least this many times.
    pub min_bought: usize,
    /// The recent median is of up to this many earlier price changes.
    pub median_of: usize,
}

impl Default for DipOptions {
    fn default() -> Self {
        DipOptions { window: 3.0 * 86400.0, depth: 0.25, min_bought: 2, median_of: 12 }
    }
}

/// An item that keeps dipping and getting bought back.
#[derive(Debug, Clone, PartialEq)]
pub struct DipTarget {
    pub id: u32,
    pub name: String,
    /// The median cheapest over the window: what it normally sells at.
    pub median: f64,
    /// The cap: buy anything at or under this.
    pub max: f64,
    /// Profit on one item bought at the cap and resold at the median, after the cut and deposit.
    pub single: f64,
    pub dips: usize,
    pub bought: usize,
    /// The deepest dip, as a share under the median then (0.6 = 60% under).
    pub deepest: f64,
    /// When the last dip was seen (unix seconds).
    pub last_dip: f64,
    /// How many were up at the latest scan.
    pub listed: f64,
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[(values.len() - 1) / 2])
}

/// One item's dips over its points (oldest first). Only changes count: an archive line repeats an
/// item's price while nothing about it moved.
fn read_item(points: &[Point], since: f64, o: &DipOptions) -> Option<(usize, usize, f64, f64, f64)> {
    let mut changes: Vec<&Point> = Vec::new();
    for p in points.iter().filter(|p| p.at as f64 >= since) {
        if changes.last().is_none_or(|last| last.price != p.price) {
            changes.push(p);
        }
    }
    if changes.len() < 4 {
        return None;
    }
    let (mut dips, mut bought, mut deepest, mut last_dip) = (0, 0, 0.0f64, 0.0);
    for i in 3..changes.len() {
        let mut before: Vec<f64> = changes[i.saturating_sub(o.median_of)..i].iter().map(|p| p.price as f64).collect();
        let Some(level) = median(&mut before) else { continue };
        let price = changes[i].price as f64;
        if level <= 0.0 || price > level * (1.0 - o.depth) {
            continue;
        }
        dips += 1;
        deepest = deepest.max(1.0 - price / level);
        last_dip = changes[i].at as f64;
        if changes.get(i + 1).is_some_and(|next| next.price as f64 >= level * 0.9) {
            bought += 1;
        }
    }
    let mut all: Vec<f64> = changes.iter().map(|p| p.price as f64).collect();
    Some((dips, bought, deepest, last_dip, median(&mut all)?))
}

/// Items that keep dipping and getting bought back, best first, capped like the daily list:
/// `snipe.ratio` of the median, and room for `min_profit` on one item after the cut and deposit.
pub fn targets(history: &ScanHistory, names: &HashMap<u32, String>, snipe: &SnipeOptions, o: &DipOptions, now: f64) -> Vec<DipTarget> {
    let since = now - o.window;
    let mut out = Vec::new();
    for (id, points) in history.items() {
        if snipe.exclude.contains(&id) {
            continue;
        }
        let Some(latest) = points.last() else { continue };
        if (latest.listed as f64) < snipe.min_listed {
            continue;
        }
        let Some((dips, bought, deepest, last_dip, level)) = read_item(points, since, o) else { continue };
        if bought < o.min_bought {
            continue;
        }
        let max = (level * snipe.ratio).min(level * 0.95 - snipe.deposit - snipe.min_profit).floor();
        if max <= 0.0 || max > snipe.cap {
            continue;
        }
        let Some(name) = names.get(&id).filter(|n| !n.contains('"')) else { continue };
        let single = (level * 0.95 - snipe.deposit - max).floor();
        out.push(DipTarget {
            id,
            name: name.clone(),
            median: level,
            max,
            single,
            dips,
            bought,
            deepest,
            last_dip,
            listed: latest.listed as f64,
        });
    }
    out.sort_by(|a, b| b.bought.cmp(&a.bought).then(b.dips.cmp(&a.dips)).then(b.median.total_cmp(&a.median)));
    out.truncate(snipe.count);
    out
}

/// The list for the dashboard, shaped like `snipes::snipes_json` plus the dip evidence.
pub fn targets_json(list_name: &str, targets: &[DipTarget]) -> Value {
    let terms: Vec<String> = targets.iter().map(|t| format!("\"{}\";;;;;;;;;{};;#;;", t.name, crate::snipes::gold(t.max))).collect();
    let items: Vec<Value> = targets
        .iter()
        .map(|t| {
            json!({ "id": t.id, "name": t.name, "max": t.max, "floor": t.median, "usual": t.median, "listed": t.listed,
                    "single": t.single, "dips": t.dips, "bought": t.bought, "deepest": t.deepest, "lastDip": t.last_dip })
        })
        .collect();
    json!({ "name": list_name, "list": format!("{list_name}^{}", terms.join("^")), "items": items })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(prices: &[(u32, u32)], listed: u32) -> ScanHistory {
        let mut h = ScanHistory::default();
        for &(at, price) in prices {
            h.add(at as f64, [(1, price as i64, listed as i64)]);
        }
        h
    }

    fn snipe_opts() -> SnipeOptions {
        SnipeOptions { min_listed: 100.0, cap: 100_000.0, ..SnipeOptions::default() }
    }

    #[test]
    fn a_dip_that_bounces_counts_one_that_stays_does_not() {
        // 10s for a while; dips to 5s and comes back (bought); later falls to 6s and stays (moved).
        let prices = [(1, 1000), (2, 1000), (3, 1010), (4, 1000), (5, 500), (6, 1000), (7, 990), (8, 1000), (9, 600), (10, 600), (11, 610)];
        let names = HashMap::from([(1, "Thick Leather".to_string())]);
        let o = DipOptions { min_bought: 1, ..DipOptions::default() };
        let t = targets(&history(&prices, 300), &names, &snipe_opts(), &o, 100.0);
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].dips, t[0].bought), (3, 1)); // 5s bounced; 6s and 6s 10c stayed down
        assert!((t[0].deepest - 0.5).abs() < 1e-9);
    }

    #[test]
    fn needs_enough_bought_back_and_a_busy_market() {
        let prices = [(1, 1000), (2, 1000), (3, 1000), (4, 1000), (5, 500), (6, 1000)];
        let names = HashMap::from([(1, "X".to_string())]);
        assert!(targets(&history(&prices, 300), &names, &snipe_opts(), &DipOptions::default(), 100.0).is_empty()); // 1 < 2 bought
        let o = DipOptions { min_bought: 1, ..DipOptions::default() };
        assert!(targets(&history(&prices, 50), &names, &snipe_opts(), &o, 100.0).is_empty());
        // 50 listed < 100
    }
}
