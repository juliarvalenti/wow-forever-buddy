//! The item catalog for the dashboard: every item Auctionator has prices for that we can name, with
//! its kind (Cloth, Herbs, ...) and its daily history.
//!
//! Auctionator stores item ids only. Names come from QuestieDB (if installed), then the gear AH scan
//! and your characters' items.

use crate::format::auctionator_date;
use crate::sources::addon::{AddonData, Places};
use crate::sources::auctionator::PriceDb;
use crate::sources::questie::{QuestieItem, QuestieItems};
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// One item as the dashboard reads it (short field names: there are thousands).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogItem {
    pub id: u32,
    /// Name.
    pub n: String,
    /// Kind ("Cloth", "Herbs", "Armor"...).
    pub k: String,
    /// The cheapest listing in the latest scan, copper.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now: Option<i64>,
    /// Daily history, oldest first: ("MM-DD", low, high, listed).
    pub h: Vec<(String, i64, i64, i64)>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Catalog {
    /// Sorted by name.
    pub items: Vec<CatalogItem>,
    /// The last day the scans cover ("2026-11-16").
    pub stamp: String,
}

/// The game's item classes we group by.
fn class_kind(class: i64) -> Option<&'static str> {
    Some(match class {
        0 => "Consumables",
        1 => "Bags",
        2 => "Weapons",
        4 => "Armor",
        5 => "Reagents",
        6 => "Ammo",
        9 => "Recipes",
        11 => "Quivers",
        12 => "Quest items",
        15 => "Miscellaneous",
        _ => return None,
    })
}

const TRADE_GOODS: i64 = 7;

/// Trade goods by subclass.
fn trade_kind(subclass: i64) -> Option<&'static str> {
    Some(match subclass {
        1 => "Parts",
        2 => "Explosives",
        3 => "Devices",
        5 => "Cloth",
        6 => "Leather",
        7 => "Metal & stone",
        8 => "Meat & fish",
        9 => "Herbs",
        10 => "Elemental",
        11 => "Other trade goods",
        12 => "Enchanting",
        _ => return None,
    })
}

/// Classic-era data files most trade goods under one subclass, so the name decides the finer kind.
/// First match wins.
fn kinds_by_name() -> &'static [(Regex, &'static str)] {
    static K: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    K.get_or_init(|| {
        let table: [(&str, &str); 9] = [
            (r"^(Linen|Wool|Silk|Mageweave|Runecloth|Felcloth|Mooncloth) Cloth$|^Bolt of |^(Runecloth|Felcloth|Mooncloth|Shadow Silk|Spider's Silk|Thick Spider's Silk|Ironweb Spider Silk)$", "Cloth"),
            (r"Leather$|Hide$|Scale$|Scales$", "Leather"),
            (r"Ore$|Bar$|Stone$|^Coal$|^Elementium", "Metal & stone"),
            (r"^Elemental |^Essence of (Air|Earth|Fire|Water|Undeath)$|^(Heart|Breath|Core|Globe) of ", "Elemental"),
            (r"Dust$|Essence$|Shard$|^Nexus Crystal$", "Enchanting"),
            (r"Meat$|Flesh$|Wing$|Egg$|Leg$|Tail$|Clam Meat$|Haunch|Chunk of|Fillet|Snapper|Mackerel|Squid$|Eel$|Sagefish", "Meat & fish"),
            (r"Lotus$|bloom$|thistle$|^(Sungrass|Dreamfoil|Gromsblood|Icecap|Kingsblood|Liferoot|Fadeleaf|Goldthorn|Wintersbite|Firebloom|Sorrowmoss|Blindweed|Stranglekelp|Bruiseweed|Silverleaf|Briarthorn|Mageroyal|Earthroot|Wild Steelbloom|Grave Moss|Khadgar's Whisker|Arthas' Tears|Ghost Mushroom|Mountain Silversage|Plaguebloom|Golden Sansam|Black Lotus|Swiftthistle)$", "Herbs"),
            (r"Potion$|Elixir|Flask of|Draught", "Potions & elixirs"),
            (r"^(Pattern|Recipe|Schematic|Formula|Plans|Manual|Design): ", "Recipes"),
        ];
        table.iter().map(|(re, kind)| (Regex::new(re).unwrap(), *kind)).collect()
    })
}

/// An item's kind: by name first, then by its class (and trade-goods subclass).
pub fn kind_of(info: &QuestieItem) -> &'static str {
    let name = info.name.as_deref().unwrap_or("");
    if let Some((_, kind)) = kinds_by_name().iter().find(|(re, _)| re.is_match(name)) {
        return kind;
    }
    match info.class {
        Some(TRADE_GOODS) => info.subclass.and_then(trade_kind).unwrap_or("Other trade goods"),
        Some(c) => class_kind(c).unwrap_or("Other"),
        None => "Other",
    }
}

/// Names and classes by id: QuestieDB's, then the gear scan's for items it lacks, then names from your
/// characters' items for any still unnamed.
fn item_info(addon: &AddonData, questie: &QuestieItems) -> HashMap<u32, QuestieItem> {
    let mut info = questie.items.clone();
    for (_, item) in addon.ah.iter().flat_map(|ah| ah.items.iter().flatten()) {
        let Some(id) = item.id.filter(|&i| i > 0.0).map(|i| i as u32) else { continue };
        info.entry(id).or_insert_with(|| QuestieItem {
            name: item.name.clone(),
            class: item.class.map(|c| c as i64),
            subclass: item.subclass.map(|s| s as i64),
        });
    }
    for c in addon.characters.values() {
        for row in c.item_rows(Places::ALL) {
            let Some(id) = row.id.filter(|&i| i > 0.0).map(|i| i as u32) else { continue };
            let entry = info.entry(id).or_default();
            if entry.name.is_none() {
                entry.name = row.name.clone();
            }
        }
    }
    info
}

/// Sort key for names: case-insensitive, then lowercase first (close to a locale compare).
fn name_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| b.cmp(a))
}

pub fn build(addon: &AddonData, prices: &PriceDb, questie: &QuestieItems) -> Catalog {
    let info = item_info(addon, questie);
    let mut items = Vec::new();
    let mut last_day = 0;
    for entry in prices.realms.iter().flat_map(|r| r.items.iter()).filter(|e| e.has_history) {
        let Some(id) = entry.id() else { continue };
        let Some(item) = info.get(&id) else { continue };
        let Some(name) = item.name.clone().filter(|n| !n.is_empty()) else { continue };
        last_day = last_day.max(entry.days.last().map_or(0, |d| d.day));
        let h = entry
            .days
            .iter()
            .map(|d| (auctionator_date(d.day)[5..].to_string(), d.cheapest() as i64, d.high as i64, d.listed.unwrap_or(0.0) as i64))
            .collect();
        items.push(CatalogItem { id, n: name, k: kind_of(item).to_string(), now: entry.now.map(|n| n as i64), h });
    }
    items.sort_by(|a, b| name_order(&a.n, &b.n));
    Catalog { items, stamp: auctionator_date(last_day) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, class: Option<i64>, subclass: Option<i64>) -> QuestieItem {
        QuestieItem { name: Some(name.into()), class, subclass }
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(&info("Linen Cloth", Some(7), Some(5))), "Cloth");
        assert_eq!(kind_of(&info("Heavy Leather", Some(7), None)), "Leather");
        assert_eq!(kind_of(&info("Peacebloom", Some(7), None)), "Herbs");
        assert_eq!(kind_of(&info("Recipe: Elixir of Fortitude", Some(9), None)), "Potions & elixirs"); // first match wins
        assert_eq!(kind_of(&info("Copper Tube", Some(7), Some(1))), "Parts");
        assert_eq!(kind_of(&info("Mystery Goods", Some(7), Some(99))), "Other trade goods");
        assert_eq!(kind_of(&info("Worn Shortsword", Some(2), None)), "Weapons");
        assert_eq!(kind_of(&info("Something", None, None)), "Other");
    }

    #[test]
    fn named_items_with_their_days() {
        use crate::sources::auctionator::realm_from_json;
        use serde_json::json;
        let mut prices = PriceDb::default();
        prices.realms.push(realm_from_json(
            "R",
            &json!({"version": 2,
            "2589": {"m": 120, "h": {"2509": 130, "2510": 125}, "l": {"2509": 100}, "a": {"2509": 400}},
            "9999": {"m": 1, "h": {"2509": 1}}}),
        ));
        let mut questie = QuestieItems::default();
        questie.items.insert(2589, info("Linen Cloth", Some(7), Some(5)));
        let c = build(&AddonData::default(), &prices, &questie);
        assert_eq!(c.items.len(), 1); // 9999 has no name
        let expected = CatalogItem {
            id: 2589,
            n: "Linen Cloth".into(),
            k: "Cloth".into(),
            now: Some(120),
            h: vec![("11-14".into(), 100, 130, 400), ("11-15".into(), 125, 125, 0)],
        };
        assert_eq!(c.items[0], expected);
        assert_eq!(c.stamp, "2026-11-15");
    }

    #[test]
    fn names_sort_case_insensitively() {
        let mut names = vec!["linen", "Bolt", "apple", "Apple"];
        names.sort_by(|a, b| name_order(a, b));
        assert_eq!(names, ["apple", "Apple", "Bolt", "linen"]);
    }
}
