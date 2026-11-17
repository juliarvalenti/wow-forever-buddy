//! An Auctionator shopping list for Point Blank Sniper (`fbl-rs snipes`): busy items, each capped
//! under its usual price.
//!
//! Point Blank Sniper buys whatever listing is under the cap, often a single item, and reposting it
//! costs a deposit. So the cap also leaves `min_profit` on ONE item after the 5% cut and the deposit;
//! items too cheap for that drop out. The cap is `ratio` x the lower of usual and the cheapest listing
//! now: a market that has fallen isn't a snipe, only a listing under today's floor is.

use crate::sources::addon::{AddonData, Places};
use crate::sources::auctionator::PriceDb;
use crate::sources::questie::QuestieItems;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct SnipeOptions {
    /// The most to pay per item (copper).
    pub cap: f64,
    /// The cap's share of the floor.
    pub ratio: f64,
    /// The deposit to repost one item, and the least profit on one item (copper).
    pub deposit: f64,
    pub min_profit: f64,
    /// How many items on the list.
    pub count: usize,
    /// Only items with at least this many up on an average day.
    pub min_listed: f64,
    /// Items never to list: ones you've hidden, and goods vendors sell without limit.
    pub exclude: HashSet<u32>,
}

impl Default for SnipeOptions {
    fn default() -> Self {
        SnipeOptions { cap: 2500.0, ratio: 0.8, deposit: 100.0, min_profit: 200.0, count: 50, min_listed: 300.0, exclude: HashSet::new() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snipe {
    pub id: u32,
    pub name: String,
    pub usual: f64,
    pub now: Option<f64>,
    /// Average listed per day.
    pub listed: f64,
    /// The cap: buy anything at or under this.
    pub max: f64,
    pub days: usize,
    /// Profit on one item bought at the cap.
    pub single: f64,
}

impl Snipe {
    /// The lower of usual and the cheapest now.
    pub fn floor(&self) -> f64 {
        self.usual.min(self.now.filter(|&n| n != 0.0).unwrap_or(self.usual))
    }
}

/// Item names: from the gear scan, the recipe data, your characters' items, then QuestieDB.
pub fn names(addon: &AddonData, recipe_items: &HashMap<String, u32>, questie: &QuestieItems) -> HashMap<u32, String> {
    let mut names: HashMap<u32, String> = HashMap::new();
    for (_, it) in addon.ah.iter().flat_map(|ah| ah.items.iter().flatten()) {
        if let (Some(id), Some(name)) = (it.id.filter(|&i| i > 0.0), &it.name) {
            names.insert(id as u32, name.clone());
        }
    }
    for (name, &id) in recipe_items {
        names.insert(id, name.clone());
    }
    for c in addon.characters.values() {
        for r in c.item_rows(Places::ALL) {
            if let (Some(id), Some(name)) = (r.id.filter(|&i| i > 0.0), &r.name) {
                names.insert(id as u32, name.clone());
            }
        }
    }
    for (&id, item) in &questie.items {
        if let Some(name) = &item.name {
            names.entry(id).or_insert_with(|| name.clone());
        }
    }
    names
}

/// The snipes, best first: worth most where the price and the trade are both big.
pub fn snipes(prices: &PriceDb, names: &HashMap<u32, String>, o: &SnipeOptions) -> Vec<Snipe> {
    let mut rows = Vec::new();
    for e in prices.realms.iter().flat_map(|r| r.items.iter()).filter(|e| e.has_history && !e.days.is_empty()) {
        let Some(usual) = crate::model::usual_price(&e.days).map(|u| u as f64) else { continue };
        let listed = e.days.iter().map(|d| d.listed.unwrap_or(0.0)).sum::<f64>() / e.days.len() as f64;
        let floor = usual.min(e.now.filter(|&n| n != 0.0).unwrap_or(usual));
        let max = (floor * o.ratio).min(floor * 0.95 - o.deposit - o.min_profit).floor();
        if listed < o.min_listed || max <= 0.0 || max > o.cap {
            continue;
        }
        let Some(id) = e.id().filter(|id| !o.exclude.contains(id)) else { continue };
        // Names with a quote can't go in Auctionator's list format.
        let Some(name) = names.get(&id).filter(|n| !n.contains('"')) else { continue };
        let single = (floor * 0.95 - o.deposit - max).floor();
        rows.push(Snipe { id, name: name.clone(), usual, now: e.now, listed: f64::round(listed), max, days: e.days.len(), single });
    }
    let worth = |s: &Snipe| s.usual * s.listed.sqrt();
    rows.sort_by(|a, b| worth(b).total_cmp(&worth(a)));
    rows.truncate(o.count);
    rows
}

/// Gold with up to four decimals, trailing zeros dropped: 2500 -> "0.25".
pub fn gold(copper: f64) -> String {
    let s = format!("{:.4}", copper / 10000.0);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Auctionator's shopping-list import line: `List name^"exact name";;;;;;;;;<max gold>;;#;;^...`.
pub fn shopping_list(list_name: &str, snipes: &[Snipe]) -> String {
    let terms: Vec<String> = snipes.iter().map(|s| format!("\"{}\";;;;;;;;;{};;#;;", s.name, gold(s.max))).collect();
    format!("{list_name}^{}", terms.join("^"))
}

/// The list for the dashboard: its name, Auctionator's import line, and each item (prices in copper).
pub fn snipes_json(list_name: &str, snipes: &[Snipe]) -> Value {
    let items: Vec<Value> = snipes
        .iter()
        .map(|s| {
            json!({ "id": s.id, "name": s.name, "max": s.max, "floor": s.floor(), "usual": s.usual, "now": s.now,
                    "listed": s.listed, "single": s.single, "days": s.days })
        })
        .collect();
    json!({ "name": list_name, "list": shopping_list(list_name, snipes), "items": items })
}

/// One line per snipe, for the console.
pub fn describe(s: &Snipe) -> String {
    let silver = |c: f64| format!("{:.2}s", c / 100.0);
    format!(
        "{:<30} max {:>8}  floor {:>8}  one-item profit {:>7}  listed ~{}",
        s.name,
        silver(s.max),
        silver(s.floor()),
        silver(s.single),
        s.listed
    )
}

/// A short random tag, so each import is a new list and never merges into an old one.
pub fn random_tag() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    format!("{:06x}", h.finish() & 0xff_ffff)
}

/// A list name as a file name: anything but letters, digits, `_` and `-` becomes `-`.
pub fn file_name(list_name: &str) -> String {
    let mut out = String::new();
    let mut in_run = false;
    for ch in list_name.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        if keep {
            out.push(ch);
        } else if !in_run {
            out.push('-');
        }
        in_run = !keep;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::auctionator::realm_from_json;
    use serde_json::json;

    #[test]
    fn caps_leave_a_profit_on_one_item() {
        let mut db = PriceDb::default();
        // Usual 1000 (today left out), now 900: floor 900. Cap: min(720, 855 - 100 - 200 = 555) = 555.
        db.realms.push(realm_from_json(
            "R",
            &json!({"version": 2,
            "5": {"m": 900, "h": {"1": 1000, "2": 1000, "3": 900}, "a": {"1": 400, "2": 400, "3": 400}},
            "6": {"m": 300, "h": {"1": 300, "2": 300}, "a": {"1": 400, "2": 400}}}),
        ));
        let names = HashMap::from([(5, "Wool Cloth".to_string()), (6, "Cheap \"Thing\"".to_string())]);
        let s = snipes(&db, &names, &SnipeOptions::default());
        assert_eq!(s.len(), 1); // 6 is too cheap to leave 2s (and has a quote)
        assert_eq!((s[0].max, s[0].single), (555.0, 200.0));
        assert_eq!(shopping_list("FBL Snipes abc123", &s), "FBL Snipes abc123^\"Wool Cloth\";;;;;;;;;0.0555;;#;;");
        assert_eq!(describe(&s[0]), "Wool Cloth                     max    5.55s  floor    9.00s  one-item profit   2.00s  listed ~400");
    }

    #[test]
    fn names_for_files_and_tags() {
        assert_eq!(file_name("FBL Snipes 1a2b3c"), "FBL-Snipes-1a2b3c");
        assert_eq!(random_tag().len(), 6);
    }
}
