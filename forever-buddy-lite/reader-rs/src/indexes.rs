//! Market indexes: how whole markets move, from your saved scans, like a stock index.
//!
//! Each index is a basket of busy trade goods of some kinds ("Herbs", "Metal & stone"...). An item
//! is in it when it shows up in most scans of the window with real volume. Its weight is its size,
//! the base price times how many are usually listed (like the S&P 500's market cap), capped at
//! `MAX_WEIGHT` of the basket so one item can't be the whole index. The index is 100 at the start of
//! the window, and at each scan 100 times the weighted average of each item's price over its own
//! base price (its median over the first scans, so one stray listing doesn't set it). An item's
//! price between its scans is its last seen one. Ratios are clamped to 0.2..5, so a single absurd
//! listing can't swing a sector.

use crate::history::{Point, ScanHistory};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The baskets: a name and the item kinds in it (catalog kinds).
pub const BASKETS: &[(&str, &[&str])] = &[
    (
        "WoW 50",
        &[
            "Herbs",
            "Metal & stone",
            "Cloth",
            "Leather",
            "Enchanting",
            "Elemental",
            "Potions & elixirs",
            "Meat & fish",
            "Reagents",
            "Parts",
            "Other trade goods",
        ],
    ),
    ("Herbs", &["Herbs"]),
    ("Ore & bars", &["Metal & stone"]),
    ("Cloth", &["Cloth"]),
    ("Leather", &["Leather"]),
    ("Enchanting", &["Enchanting"]),
    ("Elemental", &["Elemental"]),
    ("Potions & elixirs", &["Potions & elixirs"]),
    ("Cooking", &["Meat & fish"]),
];

/// The composite takes this many of its biggest markets; sectors take up to `SECTOR_SIZE`.
const COMPOSITE_SIZE: usize = 50;
const SECTOR_SIZE: usize = 30;
const MAX_WEIGHT: f64 = 0.1;
/// A member is in at least this share of the window's scans, usually has this many listed, and
/// costs at least this much (copper).
const MIN_COVERAGE: f64 = 0.6;
const MIN_LISTED: f64 = 20.0;
const MIN_PRICE: f64 = 10.0;
const BASE_SCANS: usize = 3;

#[derive(Debug, Clone)]
struct Member {
    id: u32,
    base: f64,
    weight: f64,
    /// Price at each scan of the window (carried forward), as a ratio to `base`.
    ratios: Vec<f64>,
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    v[(v.len() - 1) / 2]
}

/// The scans in the window with new data, oldest first. The archive also keeps a copy when the
/// prices didn't change (a save with no new AH scan): a scan where no item's price or count moved
/// from its previous point is a repeat and is left out, so "24 hours ago" means real data.
fn scan_times(history: &ScanHistory, since: u32) -> Vec<u32> {
    let mut changed: HashMap<u32, bool> = HashMap::new();
    for (_, pts) in history.items() {
        let mut prev: Option<&Point> = None;
        for p in pts {
            if p.at >= since {
                let moved = prev.is_none_or(|q| q.price != p.price || q.listed != p.listed);
                let e = changed.entry(p.at).or_insert(false);
                *e = *e || moved;
            }
            prev = Some(p);
        }
    }
    let mut times: Vec<u32> = changed.into_iter().filter(|(_, moved)| *moved).map(|(t, _)| t).collect();
    times.sort_unstable();
    times
}

fn member(id: u32, points: &[Point], times: &[u32]) -> Option<Member> {
    let window: Vec<&Point> = points.iter().filter(|p| p.at >= times[0] && p.price > 0).collect();
    if (window.len() as f64) < MIN_COVERAGE * times.len() as f64 {
        return None;
    }
    let base = median(window.iter().take(BASE_SCANS).map(|p| p.price as f64).collect());
    let listed = median(window.iter().map(|p| p.listed as f64).collect());
    if base < MIN_PRICE || listed < MIN_LISTED {
        return None;
    }
    let mut ratios = Vec::with_capacity(times.len());
    let mut k = 0;
    let mut last = base;
    for &t in times {
        while k < window.len() && window[k].at <= t {
            last = window[k].price as f64;
            k += 1;
        }
        ratios.push((last / base).clamp(0.2, 5.0));
    }
    Some(Member { id, base, weight: base * listed, ratios })
}

/// Caps each weight at `MAX_WEIGHT` of the total (a few rounds: capping one raises the others' share).
fn cap_weights(members: &mut [Member]) {
    if members.len() < (1.0 / MAX_WEIGHT) as usize {
        return; // too few to cap at 10% each
    }
    for _ in 0..5 {
        let total: f64 = members.iter().map(|m| m.weight).sum();
        let cap = MAX_WEIGHT * total;
        let mut changed = false;
        for m in members.iter_mut() {
            if m.weight > cap * 1.0001 {
                m.weight = cap;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

fn value_at(members: &[Member], i: usize) -> f64 {
    let total: f64 = members.iter().map(|m| m.weight).sum();
    if total <= 0.0 {
        return 100.0;
    }
    100.0 * members.iter().map(|m| m.weight * m.ratios[i]).sum::<f64>() / total
}

/// The scan nearest `secs` before the last one.
fn index_ago(times: &[u32], secs: u32) -> usize {
    let last = *times.last().unwrap();
    let target = last.saturating_sub(secs);
    times.iter().enumerate().min_by_key(|(_, &t)| t.abs_diff(target)).map_or(0, |(i, _)| i)
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Every index over the last `days` of scans: `{ indexes: [{ name, members, value, change24h,
/// changeAll, series: [[t, value]], movers: [{ id, name, change24h, impact }] }], scans, since }`.
pub fn indexes_json(history: &ScanHistory, kinds: &HashMap<u32, String>, names: &HashMap<u32, String>, now: f64, days: f64) -> Value {
    let since = (now - days * 86400.0).max(0.0) as u32;
    let times = scan_times(history, since);
    if times.len() < 2 {
        return json!({ "indexes": [], "scans": times.len(), "since": since });
    }
    let all: HashMap<u32, Member> =
        history.items().filter(|(id, _)| kinds.contains_key(id)).filter_map(|(id, pts)| member(id, pts, &times).map(|m| (id, m))).collect();
    let day = index_ago(&times, 86400);
    let last = times.len() - 1;
    let mut out = Vec::new();
    for (name, wanted) in BASKETS {
        let mut members: Vec<Member> =
            all.values().filter(|m| kinds.get(&m.id).is_some_and(|k| wanted.contains(&k.as_str()))).cloned().collect();
        members.sort_by(|a, b| b.weight.total_cmp(&a.weight));
        members.truncate(if wanted.len() > 1 { COMPOSITE_SIZE } else { SECTOR_SIZE });
        if members.len() < 3 {
            continue;
        }
        cap_weights(&mut members);
        let series: Vec<Value> = (0..times.len()).map(|i| json!([times[i], round1(value_at(&members, i))])).collect();
        let (now_v, day_v) = (value_at(&members, last), value_at(&members, day));
        let total: f64 = members.iter().map(|m| m.weight).sum();
        let mut movers: Vec<(f64, &Member)> =
            members.iter().map(|m| (100.0 * m.weight * (m.ratios[last] - m.ratios[day]) / total, m)).collect();
        movers.sort_by(|a, b| b.0.abs().total_cmp(&a.0.abs()));
        let movers: Vec<Value> = movers
            .iter()
            .take(6)
            .map(|(impact, m)| {
                json!({
                    "id": m.id,
                    "name": names.get(&m.id).cloned().unwrap_or_else(|| format!("item {}", m.id)),
                    "change24h": (m.ratios[last] / m.ratios[day] - 1.0),
                    "impact": round1(*impact * 10.0) / 10.0,
                    "base": m.base,
                })
            })
            .collect();
        out.push(json!({
            "name": name,
            "members": members.len(),
            "value": round1(now_v),
            "change24h": if day_v > 0.0 { now_v / day_v - 1.0 } else { 0.0 },
            "changeAll": now_v / 100.0 - 1.0,
            "series": series,
            "movers": movers,
        }));
    }
    json!({ "indexes": out, "scans": times.len(), "since": times[0], "latest": times[last] })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Items 1..=12 are herbs; each scan at hour h has price base*(1 + rise*h) and 100 listed.
    fn history(rise: f64) -> (ScanHistory, HashMap<u32, String>, HashMap<u32, String>) {
        let mut h = ScanHistory::default();
        for hour in 0..30u32 {
            let mut rows: Vec<_> = (1..=12u32).map(|id| (id, (100.0 * id as f64 * (1.0 + rise * hour as f64)) as i64, 100i64)).collect();
            rows.push((99, 1000 + hour as i64, 50)); // something always moves between real scans (not in a basket)
            h.add((hour * 3600) as f64, rows);
        }
        let kinds = (1..=12u32).map(|id| (id, "Herbs".to_string())).collect();
        let names = (1..=12u32).map(|id| (id, format!("Herb {id}"))).collect();
        (h, kinds, names)
    }

    #[test]
    fn a_flat_market_stays_at_100_and_a_rising_one_rises() {
        let (h, kinds, names) = history(0.0);
        let v = indexes_json(&h, &kinds, &names, 29.0 * 3600.0, 7.0);
        let herbs = v["indexes"].as_array().unwrap().iter().find(|i| i["name"] == "Herbs").unwrap();
        assert_eq!(herbs["value"].as_f64(), Some(100.0));
        assert_eq!(herbs["members"].as_u64(), Some(12));

        let (h, kinds, names) = history(0.01); // +1% of base an hour
        let v = indexes_json(&h, &kinds, &names, 29.0 * 3600.0, 7.0);
        let herbs = v["indexes"].as_array().unwrap().iter().find(|i| i["name"] == "Herbs").unwrap();
        // Base is the median of the first 3 scans (+1%), so 29h in it's about 1.29/1.01 = 127.7.
        let value = herbs["value"].as_f64().unwrap();
        assert!((127.0..129.0).contains(&value), "{value}");
        assert!(herbs["change24h"].as_f64().unwrap() > 0.15);
        assert_eq!(herbs["series"].as_array().unwrap().len(), 30);
        // The composite holds the same herbs.
        assert!(v["indexes"].as_array().unwrap().iter().any(|i| i["name"] == "WoW 50"));
    }

    #[test]
    fn repeated_scans_are_not_new_data() {
        let (mut h, kinds, names) = history(0.01);
        // Three more saves of the last scan's prices, later.
        let mut last: Vec<_> = (1..=12u32).map(|id| (id, (100.0 * id as f64 * 1.29) as i64, 100i64)).collect();
        last.push((99, 1029, 50)); // the ticker as it was at the last real scan
        for k in 1..=3u32 {
            h.add(((29 + k * 5) * 3600) as f64, last.clone());
        }
        let v = indexes_json(&h, &kinds, &names, 44.0 * 3600.0, 7.0);
        assert_eq!(v["scans"].as_u64(), Some(30));
        assert_eq!(v["latest"].as_u64(), Some(29 * 3600));
    }

    #[test]
    fn caps_stop_one_item_from_being_the_index() {
        let mut members: Vec<Member> =
            (0..12).map(|i| Member { id: i, base: 1.0, weight: if i == 0 { 1000.0 } else { 1.0 }, ratios: vec![] }).collect();
        cap_weights(&mut members);
        let total: f64 = members.iter().map(|m| m.weight).sum();
        assert!(members[0].weight / total <= MAX_WEIGHT + 0.01);
    }

    #[test]
    fn thin_or_rare_items_stay_out() {
        let mut h = ScanHistory::default();
        for hour in 0..10u32 {
            // Item 1: 5 listed (too thin). Item 2: only every 3rd scan (too rare). Items 3-5: fine.
            let mut rows = vec![(1u32, 500i64, 5i64)];
            if hour % 3 == 0 {
                rows.push((2, 500, 100));
            }
            rows.extend((3..=5u32).map(|id| (id, 500i64, 100i64)));
            rows.push((99, 1000 + hour as i64, 50)); // the ticker: real scans always move something
            h.add((hour * 3600) as f64, rows);
        }
        let kinds = (1..=5u32).map(|id| (id, "Cloth".to_string())).collect();
        let v = indexes_json(&h, &kinds, &HashMap::new(), 9.0 * 3600.0, 7.0);
        let cloth = v["indexes"].as_array().unwrap().iter().find(|i| i["name"] == "Cloth").unwrap();
        assert_eq!(cloth["members"].as_u64(), Some(3));
    }
}
