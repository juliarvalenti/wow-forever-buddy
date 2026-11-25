//! Random-suffix stats ("Dreadblade of the Bandit": +agi, +sta, +attack power) for auction items.
//!
//! On Forever the suffix's stats come from bonus ids, and the tooltip the addon reads for an
//! auction result leaves them out: a suffix item is saved with its damage and level only, and
//! would score as if it had no stats. The WoWSims Forever database has every suffix (by name, with
//! a coefficient per stat) and every item's points; the game's formula is
//! `coefficient * points / 10000`, rounded. Suffix ids differ between the two (the auction house's
//! 14330 is the sim's 12751), so suffixes are matched by name.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// The sim's stat index -> our stat key, and a divisor that turns ratings into the percent the
/// tooltip parser uses (10 hit rating = 1%, 14 crit rating = 1%).
fn stat_key(index: usize) -> Option<(&'static str, f64)> {
    Some(match index {
        0 => ("str", 1.0),
        1 => ("agi", 1.0),
        2 => ("sta", 1.0),
        3 => ("int", 1.0),
        13 => ("spi", 1.0),
        14 => ("ap", 1.0),
        5 => ("sp", 1.0),
        17 => ("hit", 10.0),
        18 => ("crit", 14.0),
        _ => return None,
    })
}

#[derive(Debug, Default)]
pub struct Suffixes {
    /// " of the Bandit" -> (stat index, coefficient).
    by_name: Vec<(String, Vec<(usize, f64)>)>,
    /// Item id -> random property points.
    points: HashMap<u64, f64>,
}

impl Suffixes {
    pub fn from_db(db: &Value) -> Suffixes {
        let mut by_name: Vec<(String, Vec<(usize, f64)>)> = Vec::new();
        for s in db["randomSuffixes"].as_array().into_iter().flatten() {
            let (Some(name), Some(stats)) = (s["name"].as_str(), s["stats"].as_array()) else { continue };
            let coefs: Vec<(usize, f64)> =
                stats.iter().enumerate().filter_map(|(i, v)| v.as_f64().filter(|c| *c != 0.0).map(|c| (i, c))).collect();
            if coefs.is_empty() || by_name.iter().any(|(n, _)| n.trim() == name) {
                continue;
            }
            by_name.push((format!(" {name}"), coefs));
        }
        // Longest first, so "of the Bandit" can't be taken for a shorter suffix that ends it.
        by_name.sort_by_key(|(n, _)| std::cmp::Reverse(n.len()));
        let mut points = HashMap::new();
        for it in db["items"].as_array().into_iter().flatten() {
            let rpp = it["scalingOptions"]["0"]["randPropPoints"].as_f64();
            if let (Some(id), Some(p)) = (it["id"].as_u64(), rpp.filter(|p| *p > 0.0)) {
                points.insert(id, p);
            }
        }
        Suffixes { by_name, points }
    }

    /// The suffix's stats for item `id` named `name`, as (key, amount); empty when the name has no
    /// known suffix or the item's points aren't known.
    pub fn stats(&self, id: u64, name: &str) -> Vec<(&'static str, f64)> {
        let Some(points) = self.points.get(&id) else { return Vec::new() };
        let Some((_, coefs)) = self.by_name.iter().find(|(n, _)| name.ends_with(n.as_str())) else { return Vec::new() };
        coefs
            .iter()
            .filter_map(|(i, c)| {
                let (key, per_percent) = stat_key(*i)?;
                let amount = (c * points / 10000.0).round();
                (amount > 0.0).then_some((key, amount / per_percent))
            })
            .collect()
    }
}

/// The suffix tables from the WoWSims database at `path`, loaded once; empty when it's missing.
pub fn load(path: &Path) -> &'static Suffixes {
    static CELL: OnceLock<Suffixes> = OnceLock::new();
    CELL.get_or_init(|| {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .map(|db| Suffixes::from_db(&db))
            .unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db() -> Value {
        let mut bandit = vec![0.0; 39];
        bandit[1] = 3193.0;
        bandit[2] = 7266.0;
        bandit[14] = 8212.0;
        bandit[15] = 8212.0;
        let mut monkey = vec![0.0; 39];
        monkey[1] = 6666.0;
        monkey[2] = 6666.0;
        json!({
            "randomSuffixes": [
                { "id": 12751, "name": "of the Bandit", "stats": bandit },
                { "id": 12752, "name": "of the Monkey", "stats": monkey },
            ],
            "items": [{ "id": 4088, "name": "Dreadblade", "scalingOptions": { "0": { "randPropPoints": 8 } } }],
        })
    }

    #[test]
    fn computes_suffix_stats_with_the_game_formula() {
        let s = Suffixes::from_db(&db());
        // 3193*8/10000 = 2.55 -> 3; 7266*8/10000 = 5.8 -> 6; 8212*8/10000 = 6.57 -> 7 (ranged AP ignored).
        assert_eq!(s.stats(4088, "Dreadblade of the Bandit"), vec![("agi", 3.0), ("sta", 6.0), ("ap", 7.0)]);
        assert_eq!(s.stats(4088, "Dreadblade of the Monkey"), vec![("agi", 5.0), ("sta", 5.0)]);
    }

    #[test]
    fn nothing_without_a_suffix_or_points() {
        let s = Suffixes::from_db(&db());
        assert!(s.stats(4088, "Dreadblade").is_empty());
        assert!(s.stats(1, "Sword of the Bandit").is_empty());
    }
}
