//! Best in slot by level for an agility melee (rogue), from the WoWSims Forever item database
//! (`fbl-rs bis`), against what the character wears now: per slot the best item at each level, where
//! it comes from, and how much it beats yours.
//!
//! Scoring: agility weights (agi 1, sta 0.6, str 0.5, AP 0.5, 1% crit or hit 12, weapon dps 3; ranged
//! dps 1, since a rogue's ranged slot is mostly stats). On-use and proc effects aren't scored, random
//! suffix ("of the Monkey") items are left out, and so are raids and PvP rank rewards unless `--all`.

use crate::sources::addon::Character;
use crate::sources::links::parse_link;
use crate::tools::upgrades::parse_stats;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Where the database is: WOWSIMS_DB, else wowsims-forever cloned next to this repository.
pub fn default_db_path() -> PathBuf {
    std::env::var_os("WOWSIMS_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../wowsims-forever/assets/database/db.json"))
}

/// Stat weights by WoWSims stat index (proto/common.proto): 0 Str, 1 Agi, 2 Sta, 14 AP, 17 hit rating,
/// 18 crit rating. About 10 hit rating and 14 crit rating are 1% in this data.
fn weight(stat: &str) -> f64 {
    match stat {
        "0" => 0.5,
        "1" => 1.0,
        "2" => 0.6,
        "14" => 0.5,
        "17" => 12.0 / 10.0,
        "18" => 12.0 / 14.0,
        _ => 0.0,
    }
}

const WEAPON: i64 = 13;
const RANGED: i64 = 14;
/// Dagger, fist, mace, sword.
const ROGUE_WEAPONS: [i64; 4] = [2, 3, 4, 9];
/// Bow, crossbow, gun, thrown.
const ROGUE_RANGED: [i64; 4] = [1, 2, 3, 4];
/// MC, BWL, AQ40, AQ20, ZG, Onyxia, Naxx.
const RAIDS: [i64; 7] = [2717, 2677, 3428, 3429, 1977, 2159, 3456];
/// Sources carry a faction (1 Alliance, 2 Horde) on vendors, quests and reputations.
const ALLIANCE: i64 = 1;

fn int(v: Option<&Value>) -> Option<i64> {
    v.and_then(Value::as_f64).map(|n| n as i64)
}

fn entries(v: Option<&Value>) -> Vec<(String, &Value)> {
    crate::json::entries(v)
        .into_iter()
        .map(|(k, x)| (if matches!(v, Some(Value::Array(_))) { (k.parse::<usize>().unwrap() - 1).to_string() } else { k }, x))
        .collect()
}

/// The item's first scaling option (stats, dps, item level).
fn option(it: &Value) -> Option<&Value> {
    it.get("scalingOptions").and_then(|o| crate::json::list(Some(o)).into_iter().next())
}

pub fn score(it: &Value) -> f64 {
    let o = option(it);
    let mut s: f64 = entries(o.and_then(|o| o.get("stats"))).iter().map(|(k, v)| weight(k) * v.as_f64().unwrap_or(0.0)).sum();
    let dps = o.and_then(|o| o.get("unroundedDps")).and_then(Value::as_f64).unwrap_or(0.0);
    match int(it.get("type")) {
        Some(WEAPON) => s += dps * 3.0,
        Some(RANGED) => s += dps,
        _ => {}
    }
    s
}

fn faction(src: &Value) -> Option<i64> {
    int(src.get("soldBy").and_then(|s| s.get("faction")))
        .or_else(|| int(src.get("quest").and_then(|q| q.get("faction"))))
        .or_else(|| int(src.get("rep").and_then(|r| r.get("factionId"))))
}

fn sources(it: &Value) -> Vec<&Value> {
    crate::json::list(it.get("sources"))
}

fn usable(it: &Value) -> bool {
    let truthy = |k: &str| it.get(k).is_some_and(crate::json::truthy);
    if truthy("randomSuffixOptions") || truthy("pvp") {
        return false;
    }
    let factions: Vec<i64> = sources(it).into_iter().filter_map(faction).collect();
    if !factions.is_empty() && !factions.contains(&ALLIANCE) {
        return false; // only Horde sources name a faction
    }
    if let Some(Value::Array(classes)) = it.get("classAllowlist") {
        if !classes.iter().any(|c| c.as_i64() == Some(4)) {
            return false; // 4 = rogue
        }
    }
    let kind = int(it.get("type")).unwrap_or(0);
    let armor = int(it.get("armorType"));
    if [1, 3, 5, 6, 7, 8, 9, 10].contains(&kind) && !matches!(armor, Some(1) | Some(2)) {
        return false; // cloth or leather
    }
    if kind == WEAPON && (!ROGUE_WEAPONS.contains(&int(it.get("weaponType")).unwrap_or(-1)) || int(it.get("handType")) == Some(4)) {
        return false;
    }
    if kind == RANGED && !ROGUE_RANGED.contains(&int(it.get("rangedWeaponType")).unwrap_or(-1)) {
        return false;
    }
    (1..=14).contains(&kind)
}

/// Forever's new items often say level 1 whatever their item level: then about item level - 5.
pub fn required_level(it: &Value) -> i64 {
    match int(it.get("reqLvl")).filter(|&l| l > 1) {
        Some(l) => l,
        None => (int(option(it).and_then(|o| o.get("ilvl"))).unwrap_or(1) - 5).max(1),
    }
}

fn raid(it: &Value) -> bool {
    sources(it).iter().any(|s| s.get("drop").is_some_and(|d| RAIDS.contains(&int(d.get("zoneId")).unwrap_or(-1))))
}

fn text<'a>(v: Option<&'a Value>, k: &str) -> Option<&'a str> {
    v.and_then(|v| v.get(k)).and_then(Value::as_str)
}

/// Where an item comes from, briefly.
fn source(it: &Value, zones: &HashMap<i64, String>) -> String {
    let list: Vec<&Value> = sources(it).into_iter().filter(|s| faction(s).is_none_or(|f| f == ALLIANCE)).collect();
    let pvp = |s: &Value| s.get("pvp").is_some_and(crate::json::truthy);
    let s = list.iter().find(|s| !pvp(s)).or(list.first()).copied();
    if list.iter().any(|s| pvp(s)) {
        return format!("PvP rep: {}", text(s.and_then(|s| s.get("soldBy")), "npcName").unwrap_or("battleground vendor"));
    }
    let Some(s) = s else { return "?".into() };
    if let Some(q) = s.get("quest") {
        return format!("quest: {}", text(Some(q), "name").unwrap_or("?"));
    }
    if let Some(c) = s.get("crafted") {
        let profession = match int(c.get("profession")) {
            Some(1) => "Alchemy",
            Some(2) => "Blacksmithing",
            Some(4) => "Engineering",
            Some(8) => "Leatherworking",
            Some(10) => "Tailoring",
            _ => "profession",
        };
        return format!("crafted ({profession})");
    }
    let drop = s.get("drop");
    if let Some(other) = text(drop, "otherName") {
        return other.to_lowercase();
    }
    if s.get("rep").is_some() {
        return match text(s.get("soldBy"), "npcName") {
            Some(npc) => format!("reputation: {npc}"),
            None => "reputation".into(),
        };
    }
    if let Some(d) = drop {
        let npc = text(Some(d), "npcName").map_or(String::new(), |n| format!(": {n}"));
        let zone = int(d.get("zoneId"))
            .filter(|&z| z != 0)
            .map_or(String::new(), |z| format!(", {}", zones.get(&z).cloned().unwrap_or(format!("zone {z}"))));
        return format!("drop{npc}{zone}");
    }
    if let Some(npc) = text(s.get("soldBy"), "npcName") {
        return format!("vendor: {npc}");
    }
    s.as_object().and_then(|o| o.keys().next().cloned()).unwrap_or_else(|| "?".into())
}

/// Slot name, WoWSims item type, inventory slot numbers.
const SLOTS: [(&str, i64, &[usize]); 15] = [
    ("Head", 1, &[1]),
    ("Neck", 2, &[2]),
    ("Shoulder", 3, &[3]),
    ("Back", 4, &[15]),
    ("Chest", 5, &[5]),
    ("Wrist", 6, &[9]),
    ("Hands", 7, &[10]),
    ("Waist", 8, &[6]),
    ("Legs", 9, &[7]),
    ("Feet", 10, &[8]),
    ("Ring", 11, &[11, 12]),
    ("Trinket", 12, &[13, 14]),
    ("Main hand", 13, &[16]),
    ("Off hand", 13, &[17]),
    ("Ranged", 14, &[18]),
];

/// What's worn in a slot, scored from its own tooltip lines (random suffixes included) with the same weights.
fn worn(c: &Character, slot: usize, by_id: &HashMap<i64, &Value>) -> (f64, String) {
    let Some((_, g)) = c.gear.iter().find(|(k, _)| k.parse::<usize>().ok() == Some(slot)) else { return (0.0, "(empty)".into()) };
    let s = parse_stats(g.tip.as_deref().unwrap_or(&[]));
    let stat = |k: &str| s.values.iter().find(|(name, _)| name == k).map_or(0.0, |(_, v)| *v);
    let dps_weight = if slot == 18 { 1.0 } else { 3.0 };
    let score = stat("agi")
        + 0.6 * stat("sta")
        + 0.5 * stat("str")
        + 0.5 * stat("ap")
        + 12.0 * stat("crit")
        + 12.0 * stat("hit")
        + stat("dps") * dps_weight;
    let id = g.id.map(|i| i as i64);
    let name = parse_link(g.link.as_deref())
        .name
        .or_else(|| id.and_then(|i| by_id.get(&i)).and_then(|it| text(Some(it), "name")).map(String::from))
        .unwrap_or_else(|| format!("item {}", id.map_or("?".into(), |i| i.to_string())));
    (score, name)
}

/// The report: per level, per slot, what you wear against the best you could.
pub fn report(c: &Character, db: &Value, levels: &[i64], all: bool) -> String {
    let items: Vec<&Value> = crate::json::list(db.get("items"));
    let zones: HashMap<i64, String> =
        crate::json::list(db.get("zones")).iter().filter_map(|z| Some((int(z.get("id"))?, z.get("name")?.as_str()?.to_string()))).collect();
    let by_id: HashMap<i64, &Value> = items.iter().filter_map(|it| Some((int(it.get("id"))?, *it))).collect();
    let pool: Vec<&Value> = items.iter().copied().filter(|it| usable(it) && (all || !raid(it))).collect();
    let mut out = vec![format!(
        "{}: best in slot by level (agility weights; raids{} {})\n",
        c.key,
        if all { "" } else { " and PvP" },
        if all { "included" } else { "left out" }
    )];
    for &level in levels {
        let (mut current, mut best_total) = (0.0, 0.0);
        let mut lines = Vec::new();
        for (name, kind, slots) in SLOTS {
            let hand = |it: &Value| int(it.get("handType"));
            let mut candidates: Vec<&Value> = pool
                .iter()
                .copied()
                .filter(|it| int(it.get("type")) == Some(kind) && required_level(it) <= level)
                .filter(|it| (name != "Off hand" || hand(it) != Some(1)) && (name != "Main hand" || hand(it) != Some(3)))
                .collect();
            candidates.sort_by(|a, b| score(b).total_cmp(&score(a)));
            for (k, &slot) in slots.iter().enumerate() {
                let (worn_score, worn_name) = worn(c, slot, &by_id);
                // Two ring and trinket slots: the next best that isn't unique-equipped twice.
                let top_id = candidates.first().and_then(|it| int(it.get("id")));
                let unique_top = |it: &&Value| it.get("unique").is_some_and(crate::json::truthy) && int(it.get("id")) == top_id;
                let filtered: Vec<&Value> = candidates.iter().copied().filter(|it| k == 0 || !unique_top(it)).collect();
                let Some(best) = filtered.get(k).or(candidates.get(k)).copied() else { continue };
                let b = score(best);
                current += worn_score;
                best_total += b.max(worn_score);
                let verdict = if b <= worn_score {
                    "   keep".to_string()
                } else if worn_score > 0.0 {
                    format!("{:>7}", format!("+{}%", f64::round((b - worn_score) / worn_score * 100.0)))
                } else {
                    "    new".to_string()
                };
                let slot_label = if slots.len() > 1 { format!("{name} {}", k + 1) } else { name.to_string() };
                lines.push(format!(
                    "  {slot_label:<11} {worn_name:<30} {:>4}  ->  {:<32} {:>4}  {verdict}  req {}  {}",
                    format!("{worn_score:.0}"),
                    text(Some(best), "name").unwrap_or("?"),
                    format!("{b:.0}"),
                    required_level(best),
                    source(best, &zones)
                ));
            }
        }
        let gain = f64::round((best_total - current) / current.max(1.0) * 100.0);
        out.push(format!("Level {level}: total {current:.0} now -> {best_total:.0} best (+{gain}%)"));
        out.push(lines.join("\n") + "\n");
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scores_and_levels() {
        let dagger = json!({"id": 776, "name": "Vendetta", "type": 13, "weaponType": 2, "handType": 2, "reqLvl": 26,
            "sources": [{"drop": {"zoneId": 491, "otherName": "Trash"}}],
            "scalingOptions": {"0": {"unroundedDps": 19.770626, "stats": {"1": 7}, "ilvl": 31}}});
        assert!((score(&dagger) - (7.0 + 19.770626 * 3.0)).abs() < 1e-9);
        assert!(usable(&dagger));
        assert_eq!(required_level(&dagger), 26);
        assert_eq!(source(&dagger, &HashMap::new()), "trash");
        let new_item = json!({"reqLvl": 1, "scalingOptions": {"0": {"ilvl": 30}}});
        assert_eq!(required_level(&new_item), 25);
        let plate = json!({"type": 1, "armorType": 4});
        assert!(!usable(&plate));
        let horde_only = json!({"type": 2, "sources": [{"soldBy": {"faction": 2, "npcName": "x"}}]});
        assert!(!usable(&horde_only));
    }
}
