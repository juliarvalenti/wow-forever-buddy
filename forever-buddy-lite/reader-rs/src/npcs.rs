//! Finding NPCs in QuestieDB by name or title: "where's a First Aid trainer?", "where's Fahrad?".
//!
//! QuestieDB keeps, per NPC, a summary record (`X-Npc-<id>-S`: field 1 the name, 14 the title
//! under it, like "First Aid Trainer") and where it stands (`X-Npc-<id>-7`: spawn points by zone
//! id, each `[x, y]` in map percent).

use crate::cbor::Value;
use crate::sources::questie::QuestieDb;
use crate::zones;
use serde::Serialize;

/// Where an NPC stands in one zone: its first spawn point there, and how many it has.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spot {
    pub zone: Option<u32>,
    pub zone_name: String,
    pub x: f64,
    pub y: f64,
    /// Spawn points in this zone (a wandering or many-placed NPC has several).
    pub points: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Npc {
    pub id: u32,
    pub name: String,
    /// The title under the name: "First Aid Trainer", "Poison Supplies".
    pub title: Option<String>,
    pub spots: Vec<Spot>,
}

/// A list's items, or a map's values in key order.
fn items(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(list) => list.iter().collect(),
        other => other.map_entries().into_iter().map(|(_, v)| v).collect(),
    }
}

/// Spawn points by zone. A table keyed 1..n arrives as a list, so its first entry is zone 1.
fn spots(spawns: Option<Value>) -> Vec<Spot> {
    let by_zone: Vec<(String, &Value)> = match &spawns {
        Some(Value::Array(list)) => list.iter().enumerate().map(|(i, pts)| ((i + 1).to_string(), pts)).collect(),
        Some(map) => map.map_entries(),
        None => return Vec::new(),
    };
    by_zone
        .into_iter()
        .filter_map(|(zone, pts)| {
            let points = items(pts);
            let first = items(points.first()?);
            let coord = |i: usize| first.get(i).and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
            let zone = zone.parse().ok();
            Some(Spot { zone, zone_name: zones::describe(zone), x: coord(0), y: coord(1), points: points.len() })
        })
        .collect()
}

/// NPCs whose name or title contains `query` (any case), optionally only those standing in a zone
/// whose name contains `zone`. Named matches come before title matches, then by name.
pub fn search(db: &QuestieDb, query: &str, zone: Option<&str>) -> Vec<Npc> {
    let query = query.to_lowercase();
    let zone = zone.map(str::to_lowercase);
    let mut found: Vec<(bool, Npc)> = Vec::new();
    for id in db.ids_with("Npc", "S") {
        let Some(summary) = db.field("Npc", id, "S") else { continue };
        let name = summary.get("1").and_then(Value::as_text).unwrap_or_default();
        let title = summary.get("14").and_then(Value::as_text).filter(|t| !t.is_empty());
        let by_name = name.to_lowercase().contains(&query);
        let by_title = title.as_deref().is_some_and(|t| t.to_lowercase().contains(&query));
        if !by_name && !by_title {
            continue;
        }
        let mut spots = spots(db.field("Npc", id, "7"));
        if let Some(z) = &zone {
            spots.retain(|s| s.zone_name.to_lowercase().contains(z.as_str()));
            if spots.is_empty() {
                continue;
            }
        }
        found.push((by_name, Npc { id, name, title, spots }));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    found.into_iter().map(|(_, npc)| npc).collect()
}

/// One line per NPC: "Fremal Doohickey <First Aid Trainer>  Wetlands (10.8, 61.4)".
pub fn report(npcs: &[Npc], limit: usize) -> String {
    if npcs.is_empty() {
        return "No NPC like that in QuestieDB.".into();
    }
    let mut out: Vec<String> = npcs
        .iter()
        .take(limit)
        .map(|n| {
            let title = n.title.as_ref().map_or(String::new(), |t| format!(" <{t}>"));
            let places: Vec<String> = n
                .spots
                .iter()
                .map(|s| {
                    let more = if s.points > 1 { format!(" +{} more", s.points - 1) } else { String::new() };
                    if s.x.is_nan() {
                        s.zone_name.clone()
                    } else {
                        format!("{} ({:.1}, {:.1}){more}", s.zone_name, s.x, s.y)
                    }
                })
                .collect();
            let places = if places.is_empty() { "no known spot".to_string() } else { places.join("; ") };
            format!("{}{title}  {places}", n.name)
        })
        .collect();
    if npcs.len() > limit {
        out.push(format!("...and {} more (narrow it with --zone).", npcs.len() - limit));
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
            line("X-Npc-1-S", serde_json::json!({ "1": "Fremal Doohickey", "14": "First Aid Trainer" })),
            line("X-Npc-1-7", serde_json::json!({ "11": [[10.8, 61.4]] })),
            line("X-Npc-2-S", serde_json::json!({ "1": "Shaina Fuller", "14": "First Aid Trainer" })),
            line("X-Npc-2-7", serde_json::json!({ "1519": [[53.0, 44.7], [53.5, 45.0]] })),
            line("X-Npc-3-S", serde_json::json!({ "1": "Doctor Aid", "14": "" })),
            line("X-Npc-3-7", serde_json::json!({ "45": [[1.0, 2.0]] })),
        ]
        .concat();
        QuestieDb::parse(&toc)
    }

    #[test]
    fn finds_by_name_and_title_names_first() {
        let found = search(&db(), "aid", None);
        assert_eq!(found.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(), ["Doctor Aid", "Fremal Doohickey", "Shaina Fuller"]);
        assert_eq!(found[0].title, None); // an empty title is no title
        assert_eq!(found[2].spots[0].points, 2);
    }

    #[test]
    fn zone_filter_and_report() {
        let found = search(&db(), "first aid", Some("wetl"));
        assert_eq!(found.len(), 1);
        assert_eq!(report(&found, 10), "Fremal Doohickey <First Aid Trainer>  Wetlands (10.8, 61.4)");
        assert_eq!(report(&search(&db(), "nobody", None), 10), "No NPC like that in QuestieDB.");
    }
}
