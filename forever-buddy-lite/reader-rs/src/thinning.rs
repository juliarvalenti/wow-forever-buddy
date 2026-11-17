//! Thinning: supply drying up before the price catches up.
//!
//! An item is **thinning** at a scan when far fewer are listed than at the same hour yesterday
//! (`max_supply`, 0.5 = half or less) while its cheapest price is still where it has been over the
//! last few scans (within `price_band`). The bet: buy the cheap stock that's left, before sellers
//! notice and post higher, and relist at the price the market moves to.
//!
//! Comparing with the same hour yesterday, not the last few scans, is the point: supply drops every
//! night when listings expire, and prices don't follow that (tested on the first 77 scans: an
//! overnight supply halving was profitable 13% of the time, against 11% for any item at any scan).
//! A drop beyond the usual daily rhythm is what might be news.
//!
//! The same rule is run over every past scan to keep score (`track_record`): how often a thinning
//! signal was followed, 2 to 12 hours later, by a floor high enough to profit after the 5% cut, and
//! the same for every item-scan as a baseline. Until the signal clearly beats the baseline, it's a
//! hypothesis, not advice: the dashboard shows the score next to the list.

use crate::history::{Point, ScanHistory};
use serde_json::{json, Value};
use std::collections::HashMap;

const HOUR: f64 = 3600.0;
const DAY: f64 = 86400.0;

#[derive(Debug, Clone, PartialEq)]
pub struct ThinOptions {
    /// "The same hour yesterday": the scan nearest 24 hours earlier, within this many seconds.
    pub same_hour: f64,
    /// Thinning: listed now at most this share of yesterday's (0.5 = half or less).
    pub max_supply: f64,
    /// The price hasn't moved: cheapest now within these shares of the recent median.
    pub price_band: (f64, f64),
    /// The recent median is of up to this many earlier scans in the last day (at least 3).
    pub recent: usize,
    /// Leave out thin markets and cheap items: yesterday at least this many listed, price at least this.
    pub min_listed: u32,
    pub min_price: u32,
    /// Keeping score: the outcome is the median floor between these many seconds after the signal.
    pub outcome: (f64, f64),
    /// A win: that floor, after the AH cut, at least this much over the buy price (0.05 = 5%).
    pub win: f64,
    pub cut: f64,
}

impl Default for ThinOptions {
    fn default() -> Self {
        ThinOptions {
            same_hour: 1.5 * HOUR,
            max_supply: 0.5,
            price_band: (0.9, 1.05),
            recent: 6,
            min_listed: 50,
            min_price: 100,
            outcome: (2.0 * HOUR, 12.0 * HOUR),
            win: 0.05,
            cut: 0.05,
        }
    }
}

/// An item thinning at its latest scan.
#[derive(Debug, Clone, PartialEq)]
pub struct Thinning {
    pub id: u32,
    pub name: String,
    pub at: u32,
    pub price: u32,
    /// The median cheapest over the scans before.
    pub usual: u32,
    pub listed: u32,
    /// Listed at the same hour yesterday.
    pub yesterday: u32,
}

/// How the rule has done on your past scans.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Record {
    pub signals: usize,
    pub items: usize,
    pub wins: usize,
    /// Median gain after the cut, as a share (−0.05: the price didn't move, you paid the cut).
    pub median_gain: f64,
    /// Every item-scan that could be judged the same way (had a yesterday and an outcome).
    pub baseline: usize,
    pub baseline_wins: usize,
    pub baseline_median_gain: f64,
}

/// (recent median price, listed yesterday) when there's enough history to judge `points[k]`.
fn context(points: &[Point], k: usize, o: &ThinOptions) -> Option<(u32, u32)> {
    let cur = points[k];
    let target = cur.at as f64 - DAY;
    let yesterday = points[..k]
        .iter()
        .filter(|p| (p.at as f64 - target).abs() <= o.same_hour)
        .min_by(|a, b| (a.at as f64 - target).abs().total_cmp(&(b.at as f64 - target).abs()))?;
    let mut recent: Vec<u32> =
        points[..k].iter().rev().take_while(|p| cur.at as f64 - (p.at as f64) <= DAY).take(o.recent).map(|p| p.price).collect();
    if recent.len() < 3 {
        return None;
    }
    recent.sort_unstable();
    Some((recent[(recent.len() - 1) / 2], yesterday.listed))
}

fn is_signal(cur: Point, usual: u32, yesterday: u32, o: &ThinOptions) -> bool {
    let (lo, hi) = o.price_band;
    yesterday >= o.min_listed
        && usual >= o.min_price
        && (cur.listed as f64) <= yesterday as f64 * o.max_supply
        && (cur.price as f64) >= usual as f64 * lo
        && (cur.price as f64) <= usual as f64 * hi
}

/// Items thinning at their latest scan, if that scan is the newest in the archive (within an hour).
/// The most value gone first (units gone since yesterday times the price), so busy markets lead
/// rather than one seller's 2-of-50.
pub fn current(history: &ScanHistory, names: &HashMap<u32, String>, o: &ThinOptions) -> Vec<Thinning> {
    let newest = history.items().filter_map(|(_, p)| p.last().map(|p| p.at)).max().unwrap_or(0);
    let mut out: Vec<Thinning> = history
        .items()
        .filter_map(|(id, points)| {
            let k = points.len().checked_sub(1)?;
            let cur = points[k];
            if (newest - cur.at) as f64 > HOUR {
                return None;
            }
            let (usual, yesterday) = context(points, k, o)?;
            is_signal(cur, usual, yesterday, o).then(|| Thinning {
                id,
                name: names.get(&id).cloned().unwrap_or_else(|| format!("item {id}")),
                at: cur.at,
                price: cur.price,
                usual,
                listed: cur.listed,
                yesterday,
            })
        })
        .collect();
    let gone = |t: &Thinning| (t.yesterday - t.listed) as f64 * t.price as f64;
    out.sort_by(|a, b| gone(b).total_cmp(&gone(a)));
    out
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    v[(v.len() - 1) / 2]
}

/// The rule run over every past scan, with the outcome 2–12 hours later.
pub fn track_record(history: &ScanHistory, o: &ThinOptions) -> Record {
    let (mut gains, mut base) = (Vec::new(), Vec::new());
    let mut items = 0;
    for (_, points) in history.items() {
        let mut fired = false;
        for k in 0..points.len() {
            let cur = points[k];
            let Some((usual, yesterday)) = context(points, k, o) else { continue };
            if yesterday < o.min_listed || usual < o.min_price || cur.price == 0 {
                continue;
            }
            let (from, to) = (cur.at as f64 + o.outcome.0, cur.at as f64 + o.outcome.1);
            let later: Vec<f64> =
                points[k + 1..].iter().filter(|p| p.at as f64 > from && p.at as f64 <= to).map(|p| p.price as f64).collect();
            if later.is_empty() {
                continue;
            }
            let gain = median(later) * (1.0 - o.cut) / cur.price as f64 - 1.0;
            base.push(gain);
            if is_signal(cur, usual, yesterday, o) {
                gains.push(gain);
                fired = true;
            }
        }
        items += fired as usize;
    }
    let wins = |g: &[f64]| g.iter().filter(|&&x| x >= o.win).count();
    Record {
        signals: gains.len(),
        items,
        wins: wins(&gains),
        baseline: base.len(),
        baseline_wins: wins(&base),
        median_gain: median(gains),
        baseline_median_gain: median(base),
    }
}

pub fn thinning_json(list: &[Thinning], record: &Record, scans: usize) -> Value {
    let items: Vec<Value> = list
        .iter()
        .map(|t| json!({ "id": t.id, "name": t.name, "at": t.at, "now": t.price, "usual": t.usual, "listed": t.listed, "yesterday": t.yesterday }))
        .collect();
    json!({
        "items": items,
        "scans": scans,
        "record": {
            "signals": record.signals, "items": record.items, "wins": record.wins, "medianGain": record.median_gain,
            "baseline": record.baseline, "baselineWins": record.baseline_wins, "baselineMedianGain": record.baseline_median_gain,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One item: (seconds, price, listed) at each scan.
    fn history(points: &[(f64, i64, i64)]) -> ScanHistory {
        let mut h = ScanHistory::default();
        for &(at, price, listed) in points {
            h.add(at, [(7, price, listed)]);
        }
        h
    }

    #[test]
    fn fewer_than_yesterday_at_a_steady_price_is_thinning() {
        let o = ThinOptions::default();
        // Yesterday noon 400 listed; this morning a few scans at 10s; now noon, 150 listed, still 10s.
        let h = history(&[(0.0, 1000, 400), (20.0 * HOUR, 1000, 300), (21.0 * HOUR, 990, 250), (22.0 * HOUR, 1010, 200), (DAY, 1000, 150)]);
        let list = current(&h, &HashMap::new(), &o);
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].listed, list[0].yesterday, list[0].usual), (150, 400, 1000));
    }

    #[test]
    fn a_price_that_already_rose_or_no_yesterday_is_not() {
        let o = ThinOptions::default();
        let risen =
            history(&[(0.0, 1000, 400), (20.0 * HOUR, 1000, 300), (21.0 * HOUR, 1000, 250), (22.0 * HOUR, 1000, 200), (DAY, 1300, 150)]);
        assert!(current(&risen, &HashMap::new(), &o).is_empty());
        // Nothing near the same hour yesterday (the first scan is 5 hours off).
        let no_yesterday = history(&[
            (5.0 * HOUR, 1000, 400),
            (20.0 * HOUR, 1000, 300),
            (21.0 * HOUR, 1000, 250),
            (22.0 * HOUR, 1000, 200),
            (DAY, 1000, 150),
        ]);
        assert!(current(&no_yesterday, &HashMap::new(), &o).is_empty());
    }

    #[test]
    fn the_record_scores_what_happened_next() {
        let o = ThinOptions::default();
        // The signal at day 1 noon, then the floor goes to 12s from 3h later: a win.
        let h = history(&[
            (0.0, 1000, 400),
            (20.0 * HOUR, 1000, 300),
            (21.0 * HOUR, 1000, 250),
            (22.0 * HOUR, 1000, 200),
            (DAY, 1000, 150),
            (DAY + 3.0 * HOUR, 1200, 100),
            (DAY + 4.0 * HOUR, 1200, 100),
        ]);
        let r = track_record(&h, &o);
        assert_eq!((r.signals, r.wins, r.items), (1, 1, 1));
        assert!((r.median_gain - (1200.0 * 0.95 / 1000.0 - 1.0)).abs() < 1e-9);
        assert!(r.baseline >= 1);
    }
}
