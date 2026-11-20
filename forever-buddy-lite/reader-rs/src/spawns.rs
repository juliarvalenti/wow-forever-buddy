//! Where things are, from QuestieDB: every spawn point of an object ("Waterlogged Footlocker",
//! "Silverleaf", "Tin Vein") or an NPC, by zone, and an OpenMap pin code for them
//! (`fbl-rs objects <name> --pins`, `fbl-rs npc <name> --pins`).
//!
//! QuestieDB keeps objects like NPCs: a summary (`X-Object-<id>-S`, field 1 the name) and spawn
//! points (`X-Object-<id>-4`; for NPCs `X-Npc-<id>-7`), by zone (area) id, each `[x, y]` in map
//! percent. OpenMap wants the game's uiMap id instead of the area id; QuestieDB ships that table
//! (`support/Forever/Zones/areaIdToUiMapId.lua`).
//!
//! An OpenMap code is `OMPINS1:` then `map,x,y,colour,note` per pin (x and y 0..1), joined by `;`.
//! OpenMap reads the code up to the first space, so a note can't hold one; it writes `%`, `,` and
//! `;` in a note as `%XX`. It takes at most 500 pins from one code, so more are split over codes.

use crate::cbor::Value;
use crate::sources::questie::QuestieDb;
use crate::zones;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// OpenMap's pin colours, in its order: 1 gold, 2 red, 3 green, 4 blue, 5 purple.
pub const COLORS: [&str; 5] = ["gold", "red", "green", "blue", "purple"];
/// Pins OpenMap takes from one code.
pub const MAX_PER_CODE: usize = 500;

/// One zone's spawn points for a thing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Place {
    pub zone: Option<u32>,
    pub zone_name: String,
    /// Each `[x, y]`, map percent.
    pub points: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Found {
    /// "object" or "npc".
    pub kind: &'static str,
    pub id: u32,
    pub name: String,
    pub title: Option<String>,
    pub places: Vec<Place>,
}

/// A list's items, or a map's values in key order.
fn items(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(list) => list.iter().collect(),
        other => other.map_entries().into_iter().map(|(_, v)| v).collect(),
    }
}

/// Every spawn point, by zone. A table keyed 1..n arrives as a list, so its first entry is zone 1.
pub fn places(spawns: Option<Value>) -> Vec<Place> {
    let by_zone: Vec<(String, &Value)> = match &spawns {
        Some(Value::Array(list)) => list.iter().enumerate().map(|(i, pts)| ((i + 1).to_string(), pts)).collect(),
        Some(map) => map.map_entries(),
        None => return Vec::new(),
    };
    by_zone
        .into_iter()
        .map(|(zone, pts)| {
            let points = items(pts)
                .into_iter()
                .filter_map(|p| {
                    let xy = items(p);
                    Some((xy.first()?.as_f64()?, xy.get(1)?.as_f64()?))
                })
                .collect();
            let zone = zone.parse().ok();
            Place { zone, zone_name: zones::describe(zone), points }
        })
        .collect()
}

/// Objects (`kind` "Object", spawns in field 4) or NPCs ("Npc", field 7) whose name contains
/// `query` (any case), optionally only their places in a zone whose name contains `zone`. By name.
pub fn search(db: &QuestieDb, kind: &str, query: &str, zone: Option<&str>) -> Vec<Found> {
    let (label, spawn_field) = if kind == "Npc" { ("npc", "7") } else { ("object", "4") };
    let query = query.to_lowercase();
    let zone = zone.map(str::to_lowercase);
    let mut found = Vec::new();
    for id in db.ids_with(kind, "S") {
        let Some(summary) = db.field(kind, id, "S") else { continue };
        let name = summary.get("1").and_then(Value::as_text).unwrap_or_default();
        if !name.to_lowercase().contains(&query) {
            continue;
        }
        let title = if kind == "Npc" { summary.get("14").and_then(Value::as_text).filter(|t| !t.is_empty()) } else { None };
        let mut places = places(db.field(kind, id, spawn_field));
        if let Some(z) = &zone {
            places.retain(|p| p.zone_name.to_lowercase().contains(z.as_str()));
            if places.is_empty() {
                continue;
            }
        }
        found.push(Found { kind: label, id, name, title, places });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    found
}

/// QuestieDB's area id -> uiMap id table (`[area] = map` lines).
pub fn ui_maps(questiedb_dir: &Path) -> HashMap<u32, u32> {
    let text = std::fs::read_to_string(questiedb_dir.join("support/Forever/Zones/areaIdToUiMapId.lua")).unwrap_or_default();
    parse_ui_maps(&text)
}

pub fn parse_ui_maps(text: &str) -> HashMap<u32, u32> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix('[') else { continue };
        let Some((area, map)) = rest.split_once(']') else { continue };
        let map: String = map.trim_start().trim_start_matches('=').trim().chars().take_while(char::is_ascii_digit).collect();
        if let (Ok(area), Ok(map)) = (area.trim().parse(), map.parse()) {
            out.insert(area, map);
        }
    }
    out
}

/// A note OpenMap keeps whole: no spaces (its code ends at the first), `%` `,` `;` as `%XX`, no `|`.
fn note(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            ' ' | '\t' => out.push('-'),
            '%' | ',' | ';' => out.push_str(&format!("%{:02X}", c as u32)),
            '|' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// A colour name or number (1-5) to OpenMap's number.
pub fn color(name: &str) -> Option<usize> {
    let n = name.trim().to_lowercase();
    n.parse::<usize>().ok().filter(|i| (1..=5).contains(i)).or_else(|| COLORS.iter().position(|c| *c == n).map(|i| i + 1))
}

/// What pinning left out: points in zones without a uiMap (dungeons, unknown areas) or off the map.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Pins {
    /// One or more codes (OpenMap takes at most `MAX_PER_CODE` from each).
    pub codes: Vec<String>,
    pub pinned: usize,
    pub skipped: usize,
}

/// OpenMap codes for everything found. `fixed`: one colour for all; otherwise each distinct name
/// gets its own, in the order blue, green, red, purple, gold.
pub fn pins(found: &[Found], maps: &HashMap<u32, u32>, fixed: Option<usize>) -> Pins {
    const CYCLE: [usize; 5] = [4, 3, 2, 5, 1];
    let mut by_name: HashMap<&str, usize> = HashMap::new();
    let mut parts = Vec::new();
    let mut skipped = 0;
    for f in found {
        let next = by_name.len();
        let colour = fixed.unwrap_or_else(|| *by_name.entry(f.name.as_str()).or_insert(CYCLE[next % CYCLE.len()]));
        let label = note(&f.name);
        for place in &f.places {
            let map = place.zone.and_then(|z| maps.get(&z));
            for &(x, y) in &place.points {
                let (px, py) = (x / 100.0, y / 100.0);
                match map {
                    Some(m) if (0.0..=1.0).contains(&px) && (0.0..=1.0).contains(&py) => {
                        parts.push(format!("{m},{px:.4},{py:.4},{colour},{label}"));
                    }
                    _ => skipped += 1,
                }
            }
        }
    }
    let pinned = parts.len();
    let codes = parts.chunks(MAX_PER_CODE).map(|c| format!("OMPINS1:{}", c.join(";"))).collect();
    Pins { codes, pinned, skipped }
}

/// One block per thing: its zones, and every point (up to `per_zone` a zone).
pub fn report(found: &[Found], limit: usize, per_zone: usize) -> String {
    if found.is_empty() {
        return "Nothing by that name in QuestieDB.".into();
    }
    let mut out = Vec::new();
    for f in found.iter().take(limit) {
        let title = f.title.as_ref().map_or(String::new(), |t| format!(" <{t}>"));
        let total: usize = f.places.iter().map(|p| p.points.len()).sum();
        out.push(format!("{}{title} ({} {}, {total} spawn{})", f.name, f.kind, f.id, if total == 1 { "" } else { "s" }));
        if f.places.is_empty() {
            out.push("  no known spot".into());
        }
        for p in &f.places {
            let shown: Vec<String> = p.points.iter().take(per_zone).map(|(x, y)| format!("{x:.1}, {y:.1}")).collect();
            let more = p.points.len().saturating_sub(per_zone);
            let more = if more > 0 { format!("  +{more} more") } else { String::new() };
            out.push(format!("  {} ({}): {}{more}", p.zone_name, p.points.len(), shown.join(" | ")));
        }
    }
    if found.len() > limit {
        out.push(format!("...and {} more (narrow it with --zone).", found.len() - limit));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbor;
    use base64::Engine;

    fn line(key: &str, v: serde_json::Value) -> String {
        format!("## {key}: {}\n", base64::engine::general_purpose::STANDARD.encode(cbor::tests::encode(&v)))
    }

    fn db() -> QuestieDb {
        let toc = [
            line("X-Object-179487-S", serde_json::json!({ "1": "Waterlogged Footlocker" })),
            line("X-Object-179487-4", serde_json::json!({ "331": [[7.1, 14.2], [13.6, 30.9]], "44": [[34.3, 61.9]] })),
            line("X-Object-179486-S", serde_json::json!({ "1": "Battered Footlocker" })),
            line("X-Object-179486-4", serde_json::json!({ "11": [[23.2, 31.9]], "1581": [[-1.0, -1.0]] })),
            line("X-Object-1-S", serde_json::json!({ "1": "Old Lion Statue" })),
        ]
        .concat();
        QuestieDb::parse(&toc)
    }

    fn maps() -> HashMap<u32, u32> {
        parse_ui_maps("-- comment\n  [331] = 1440, -- Ashenvale\n  [44] = 1433,\n  [11]=1437,\n")
    }

    #[test]
    fn finds_objects_with_every_point() {
        let found = search(&db(), "Object", "footlocker", None);
        assert_eq!(found.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Battered Footlocker", "Waterlogged Footlocker"]);
        let water = &found[1];
        let ashenvale = water.places.iter().find(|p| p.zone == Some(331)).unwrap();
        assert_eq!(ashenvale.points, [(7.1, 14.2), (13.6, 30.9)]);
        // Narrowed to a zone: only places there, and things with none drop out.
        let only = search(&db(), "Object", "footlocker", Some("ashen"));
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].places.len(), 1);
    }

    #[test]
    fn pins_make_an_openmap_code() {
        let found = search(&db(), "Object", "footlocker", None);
        let p = pins(&found, &maps(), None);
        // The dungeon point (-1, -1, zone without a uiMap) is skipped.
        assert_eq!((p.pinned, p.skipped), (4, 1));
        assert_eq!(p.codes.len(), 1);
        let code = &p.codes[0];
        assert!(code.starts_with("OMPINS1:1437,0.2320,0.3190,4,Battered-Footlocker;"));
        assert!(code.contains("1440,0.0710,0.1420,3,Waterlogged-Footlocker"));
        assert!(!code.contains(' '));
        // One colour for all.
        assert!(pins(&found, &maps(), Some(2)).codes[0].split(';').all(|part| part.split(',').nth(3) == Some("2")));
    }

    #[test]
    fn notes_and_colours() {
        assert_eq!(note("Mag'har Grunt, 50%; x|y"), "Mag'har-Grunt%2C-50%25%3B-xy");
        assert_eq!(color("Purple"), Some(5));
        assert_eq!(color("3"), Some(3));
        assert_eq!(color("pink"), None);
    }

    #[test]
    fn big_sets_split_over_codes() {
        let place = Place { zone: Some(331), zone_name: "Ashenvale".into(), points: vec![(50.0, 50.0); 1200] };
        let found = [Found { kind: "object", id: 1, name: "Tin Vein".into(), title: None, places: vec![place] }];
        let p = pins(&found, &maps(), None);
        assert_eq!(p.codes.len(), 3);
        assert_eq!(p.codes[0].split(';').count(), MAX_PER_CODE);
    }

    #[test]
    fn report_lists_points() {
        let text = report(&search(&db(), "Object", "waterlogged", None), 10, 12);
        assert!(text.starts_with("Waterlogged Footlocker (object 179487, 3 spawns)"));
        assert!(text.contains("7.1, 14.2 | 13.6, 30.9"));
    }
}
