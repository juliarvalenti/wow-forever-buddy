//! Battleground match history (the dashboard's Battlegrounds page, `/bgs.json`), from the matches
//! the addon records when each one ends: win rate, your averages, honor per hour, by battleground,
//! and the matches themselves, newest first. Only your own row and each side's totals are kept.

use crate::sources::addon::BgMatch;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone, PartialEq)]
struct Tally {
    matches: usize,
    wins: usize,
    losses: usize,
    kb: f64,
    hk: f64,
    deaths: f64,
    honor: f64,
    damage: f64,
    healing: f64,
    /// Matches whose damage and healing were readable (the averages divide by these).
    with_numbers: usize,
    seconds: f64,
}

impl Tally {
    fn add(&mut self, m: &BgMatch) {
        self.matches += 1;
        match m.result.as_str() {
            "win" => self.wins += 1,
            "loss" => self.losses += 1,
            _ => {}
        }
        if let Some(me) = &m.me {
            self.kb += me.kb.unwrap_or(0.0);
            self.hk += me.hk.unwrap_or(0.0);
            self.deaths += me.deaths.unwrap_or(0.0);
            self.honor += me.honor.unwrap_or(0.0);
            if me.damage.is_some() || me.healing.is_some() {
                self.with_numbers += 1;
                self.damage += me.damage.unwrap_or(0.0);
                self.healing += me.healing.unwrap_or(0.0);
            }
        }
        self.seconds += m.duration.unwrap_or(0.0);
    }

    fn json(&self) -> Value {
        let per = |total: f64, n: usize| if n > 0 { Some((total / n as f64 * 10.0).round() / 10.0) } else { None };
        let decided = self.wins + self.losses;
        json!({
            "matches": self.matches,
            "wins": self.wins,
            "losses": self.losses,
            "winRate": if decided > 0 { Some(self.wins as f64 / decided as f64) } else { None },
            "kb": per(self.kb, self.matches),
            "hk": per(self.hk, self.matches),
            "deaths": per(self.deaths, self.matches),
            "damage": per(self.damage, self.with_numbers),
            "healing": per(self.healing, self.with_numbers),
            "honor": self.honor,
            "honorPerHour": if self.seconds > 0.0 { Some((self.honor / (self.seconds / 3600.0)).round()) } else { None },
            "minutes": (self.seconds / 60.0).round(),
        })
    }
}

/// `{ totals, maps: [{ map, ...totals }], matches: [newest first], characters }`, optionally for one
/// character ("Name-Realm").
pub fn bgs_json(all: &[BgMatch], character: Option<&str>) -> Value {
    let matches: Vec<&BgMatch> = all.iter().filter(|m| character.is_none_or(|c| m.char.as_deref() == Some(c))).collect();
    let mut total = Tally::default();
    let mut by_map: BTreeMap<String, Tally> = BTreeMap::new();
    for m in &matches {
        total.add(m);
        by_map.entry(m.map.clone()).or_default().add(m);
    }
    let mut maps: Vec<Value> = by_map
        .iter()
        .map(|(map, t)| {
            let mut v = t.json();
            v["map"] = json!(map);
            v
        })
        .collect();
    maps.sort_by(|a, b| b["matches"].as_u64().cmp(&a["matches"].as_u64()));
    let mut characters: Vec<String> = all.iter().filter_map(|m| m.char.clone()).collect();
    characters.sort();
    characters.dedup();
    let recent: Vec<Value> = matches.iter().rev().take(200).map(|m| serde_json::to_value(m).unwrap_or(Value::Null)).collect();
    json!({ "totals": total.json(), "maps": maps, "matches": recent, "characters": characters })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::addon::{BgRow, BgSide};

    #[allow(clippy::too_many_arguments)]
    fn m(at: f64, map: &str, result: &str, kb: f64, damage: Option<f64>, honor: f64, minutes: f64, ch: &str) -> BgMatch {
        BgMatch {
            at,
            started: Some(at - minutes * 60.0),
            duration: Some(minutes * 60.0),
            map: map.into(),
            char: Some(ch.into()),
            result: result.into(),
            me: Some(BgRow {
                kb: Some(kb),
                hk: Some(kb * 3.0),
                deaths: Some(2.0),
                honor: Some(honor),
                damage,
                healing: None,
                stats: vec![],
            }),
            mine: Some(BgSide { players: 10.0, ..BgSide::default() }),
            theirs: Some(BgSide { players: 10.0, ..BgSide::default() }),
        }
    }

    #[test]
    fn totals_maps_and_newest_first() {
        let all = [
            m(100.0, "Warsong Gulch", "win", 4.0, Some(30000.0), 200.0, 20.0, "Rogue-X"),
            m(200.0, "Warsong Gulch", "loss", 2.0, None, 100.0, 10.0, "Rogue-X"),
            m(300.0, "Arathi Basin", "win", 6.0, Some(50000.0), 300.0, 30.0, "Rogue-X"),
            m(400.0, "Arathi Basin", "unknown", 0.0, None, 0.0, 0.0, "Priest-X"),
        ];
        let v = bgs_json(&all, Some("Rogue-X"));
        let t = &v["totals"];
        assert_eq!((t["matches"].as_u64(), t["wins"].as_u64(), t["losses"].as_u64()), (Some(3), Some(2), Some(1)));
        assert_eq!(t["winRate"].as_f64(), Some(2.0 / 3.0));
        assert_eq!(t["kb"].as_f64(), Some(4.0));
        // Damage averages only the matches where it was readable.
        assert_eq!(t["damage"].as_f64(), Some(40000.0));
        // 600 honor over 60 minutes.
        assert_eq!(t["honorPerHour"].as_f64(), Some(600.0));
        assert_eq!(v["maps"][0]["map"], "Warsong Gulch"); // played most
        assert_eq!(v["matches"][0]["at"].as_f64(), Some(300.0));
        assert_eq!(v["characters"], json!(["Priest-X", "Rogue-X"]));
        // Everyone: four matches, the unknown result counts in matches but not in the win rate.
        assert_eq!(bgs_json(&all, None)["totals"]["matches"].as_u64(), Some(4));
    }

    #[test]
    fn empty() {
        let v = bgs_json(&[], None);
        assert_eq!(v["totals"]["matches"].as_u64(), Some(0));
        assert!(v["totals"]["winRate"].is_null());
    }
}
