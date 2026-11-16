//! XP-dense zones, hubs and chains for a character (`fbl-rs questxp`), from the QuestieDB addon's
//! Forever data (read only): quest XP, levels, races, classes, where quests start and end.
//!
//! - `questxp [character] [min level] [max level] [zone]`: zones by quest XP left, their hubs (turn-ins
//!   close together), and the best-paying chains. With a zone: every hub and quest there.
//! - `questxp [character] --chain "Quest name"`: the whole chain it's in: steps, XP left, travel, rewards.
//!
//! Quests are filtered to the character's faction and class.

use crate::cbor::Value;
use crate::sources::addon::Character;
use crate::sources::questie::QuestieDb;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Quest XP by quest id: (quest level, XP), from QuestieDB's xpDB-classic.lua.
pub fn load_xp(questie_dir: &Path) -> Result<HashMap<u32, (f64, f64)>, String> {
    let file = questie_dir.join("support/Forever/QuestXP/xpDB-classic.lua");
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let re = regex::Regex::new(r"\[([0-9]+)\] = \{([0-9]+), ([0-9]+)\}").unwrap();
    Ok(re.captures_iter(&text).filter_map(|m| Some((m[1].parse().ok()?, (m[2].parse().ok()?, m[3].parse().ok()?)))).collect())
}

/// The values of a list or a map (in key order).
fn vals(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(m @ Value::Map(_)) => m.map_entries().into_iter().map(|(_, v)| v).collect(),
        _ => Vec::new(),
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

fn id(v: Option<&Value>) -> Option<u32> {
    num(v).filter(|n| *n > 0.0).map(|n| n as u32)
}

/// Zone names by area id (the ones quests here come up in).
fn zone_name(zone: u32) -> Option<&'static str> {
    Some(match zone {
        33 => "Stranglethorn",
        45 => "Arathi Highlands",
        36 => "Alterac Mountains",
        267 => "Hillsbrad",
        11 => "Wetlands",
        405 => "Desolace",
        15 => "Dustwallow Marsh",
        400 => "Thousand Needles",
        3 => "Badlands",
        8 => "Swamp of Sorrows",
        47 => "Hinterlands",
        440 => "Tanaris",
        357 => "Feralas",
        331 => "Ashenvale",
        406 => "Stonetalon",
        17 => "Barrens",
        1519 => "Stormwind",
        1537 => "Ironforge",
        10 => "Duskwood",
        44 => "Redridge",
        38 => "Loch Modan",
        4 => "Blasted Lands",
        16 => "Azshara",
        361 => "Felwood",
        51 => "Searing Gorge",
        46 => "Burning Steppes",
        1657 => "Darnassus",
        148 => "Darkshore",
        130 => "Silverpine",
        85 => "Tirisfal",
        28 => "Western Plaguelands",
        139 => "Eastern Plaguelands",
        41 => "Deadwind Pass",
        490 => "Un'Goro",
        12 => "Elwynn",
        40 => "Westfall",
        1 => "Dun Morogh",
        1638 => "Thunder Bluff",
        1637 => "Orgrimmar",
        1497 => "Undercity",
        _ => return None,
    })
}

fn dungeon_name(zone: u32) -> Option<&'static str> {
    Some(match zone {
        796 => "Scarlet Monastery",
        491 => "Razorfen Kraul",
        722 => "Razorfen Downs",
        1337 => "Uldaman",
        721 => "Gnomeregan",
        1176 => "Zul'Farrak",
        2100 => "Maraudon",
        209 => "Shadowfang Keep",
        718 => "Wailing Caverns",
        1581 => "Deadmines",
        717 => "Stockade",
        719 => "Blackfathom Deeps",
        2557 => "Dire Maul",
        1584 => "Blackrock Depths",
        _ => return None,
    })
}

fn zname(zone: Option<u32>) -> String {
    match zone {
        Some(z) => zone_name(z).or_else(|| dungeon_name(z)).map_or_else(|| format!("zone {z}"), String::from),
        None => "an unknown zone".into(),
    }
}

/// Where an NPC or object stands.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub zone: Option<u32>,
    pub x: f64,
    pub y: f64,
}

/// Who may take a quest: race bits for the faction, the class bit.
#[derive(Debug, Clone, Copy)]
pub struct Eligibility {
    races: Option<i64>,
    class_bit: Option<i64>,
}

impl Eligibility {
    /// From the character's faction and class (unknown ones don't filter).
    pub fn of(c: &Character) -> Eligibility {
        // Race bits: Alliance human 1, dwarf 4, night elf 8, gnome 64, draenei 1024; Horde orc 2,
        // undead 16, tauren 32, troll 128, blood elf 512.
        let races = match c.identity.faction.as_deref() {
            Some("Alliance") => Some(1101),
            Some("Horde") => Some(690),
            _ => None,
        };
        let class_id = match c.identity.class.as_deref().unwrap_or("") {
            "WARRIOR" => 1,
            "PALADIN" => 2,
            "HUNTER" => 3,
            "ROGUE" => 4,
            "PRIEST" => 5,
            "SHAMAN" => 7,
            "MAGE" => 8,
            "WARLOCK" => 9,
            "DRUID" => 11,
            _ => 0,
        };
        Eligibility { races, class_bit: (class_id > 0).then(|| 1 << (class_id - 1)) }
    }

    /// A quest record's race and class masks (0 or missing: anyone).
    fn allows(&self, record: Option<&Value>) -> bool {
        let mask = |k: &str| num(record.and_then(|r| r.get(k))).map(|n| n as i64).filter(|&n| n != 0);
        let race_ok = match (mask("6"), self.races) {
            (Some(r), Some(ours)) => (r % 2048) & ours != 0,
            _ => true,
        };
        let class_ok = match (mask("7"), self.class_bit) {
            (Some(c), Some(ours)) => c & ours != 0,
            _ => true,
        };
        race_ok && class_ok
    }
}

pub struct QuestXp<'a> {
    db: &'a QuestieDb,
    xp: &'a HashMap<u32, (f64, f64)>,
    level: f64,
    done: HashSet<u32>,
    in_log: HashSet<u32>,
    who: Eligibility,
}

#[derive(Debug, Clone)]
struct Quest {
    id: u32,
    name: String,
    level: f64,
    xp: f64,
    end: Place,
    start: Option<Place>,
    dungeon: Option<&'static str>,
    in_log: bool,
    next: Option<u32>,
    prerequisites: Vec<u32>,
}

impl<'a> QuestXp<'a> {
    pub fn new(db: &'a QuestieDb, xp: &'a HashMap<u32, (f64, f64)>, c: &Character) -> QuestXp<'a> {
        QuestXp {
            db,
            xp,
            level: c.identity.level.unwrap_or(1.0),
            done: c.quests.completed.iter().map(|&q| q as u32).collect(),
            in_log: c.quests.log.iter().filter_map(|q| q.id).map(|q| q as u32).collect(),
            who: Eligibility::of(c),
        }
    }

    /// Classic: full XP up to quest level + 5, then 80/60/40/20/10%.
    pub fn earn(&self, quest_level: f64, xp: f64) -> f64 {
        let over = self.level - quest_level;
        let share =
            if over <= 5.0 { 1.0 } else { [0.8, 0.6, 0.4, 0.2].get((over - 6.0) as usize).copied().filter(|_| over >= 6.0).unwrap_or(0.1) };
        f64::round(xp * share)
    }

    fn record(&self, id: u32) -> Option<Value> {
        self.db.field("Quest", id, "S")
    }

    /// Where a quest starts (field 2) or is turned in (field 3): its first NPC (or object) and that
    /// one's first spawn.
    fn place(&self, field: &str, quest: u32) -> Option<Place> {
        let t = self.db.field("Quest", quest, field);
        let lists = vals(t.as_ref());
        let npc = id(lists.first().and_then(|l| vals(Some(l)).first().copied()));
        let object = id(lists.get(1).and_then(|l| vals(Some(l)).first().copied()));
        let (kind, who, spawn_field, zone_field) = match (npc, object) {
            (Some(n), _) => ("Npc", n, "7", "9"),
            (None, Some(o)) => ("Object", o, "4", "5"),
            _ => return None,
        };
        let spawns = self.db.field(kind, who, spawn_field);
        let summary = self.db.field(kind, who, "S");
        // Spawns by zone id. A table keyed 1..n comes out as a list: its first entry is zone 1.
        let first_zone = match &spawns {
            Some(Value::Array(by_zone)) => by_zone.first().map(|pts| ("1".to_string(), pts)),
            Some(s) => s.map_entries().into_iter().next(),
            None => None,
        };
        let point = first_zone.as_ref().and_then(|(_, pts)| vals(Some(pts)).first().copied());
        let coord = |i: usize| point.and_then(|p| vals(Some(p)).get(i).copied()).and_then(Value::as_f64).unwrap_or(f64::NAN);
        let zone = match &first_zone {
            Some((z, _)) => z.parse().ok(),
            None => id(summary.as_ref().and_then(|s| s.get(zone_field))),
        };
        let name = summary.as_ref().and_then(|s| s.get("1")).and_then(Value::as_text).unwrap_or_default();
        Some(Place { name, zone, x: coord(0), y: coord(1) })
    }

    /// Quests at levels lo..=hi the character can still do, can take (faction, class, level + 3) and
    /// that aren't profession quests.
    fn quests(&self, lo: f64, hi: f64) -> Vec<Quest> {
        let mut out = Vec::new();
        for qid in self.db.ids_with("Quest", "S") {
            let sc = self.record(qid);
            let get = |k: &str| sc.as_ref().and_then(|s| s.get(k));
            let Some(qlvl) = num(get("5")).filter(|l| *l != 0.0) else { continue };
            let req = num(get("4")).unwrap_or(0.0);
            let Some(&(_, xp)) = self.xp.get(&qid) else { continue };
            if qlvl < lo || qlvl > hi || req > self.level + 3.0 || self.done.contains(&qid) || !self.who.allows(sc.as_ref()) {
                continue;
            }
            if self.db.field("Quest", qid, "18").is_some_and(|v| !matches!(v, Value::Unsigned(0) | Value::Null | Value::Bool(false))) {
                continue; // profession quests
            }
            let Some(end) = self.place("3", qid) else { continue };
            let prerequisites = ["12", "13"]
                .iter()
                .flat_map(|f| vals(self.db.field("Quest", qid, f).as_ref()).into_iter().filter_map(|v| id(Some(v))).collect::<Vec<_>>())
                .collect();
            out.push(Quest {
                id: qid,
                name: get("1").and_then(Value::as_text).unwrap_or_default(),
                level: qlvl,
                xp: self.earn(qlvl, xp),
                end,
                start: self.place("2", qid),
                dungeon: id(get("17")).and_then(dungeon_name),
                in_log: self.in_log.contains(&qid),
                next: id(get("22")),
                prerequisites,
            });
        }
        out
    }

    /// Zones, hubs and chains, as printed.
    pub fn overview(&self, who: &str, lo: f64, hi: f64, only: Option<&str>) -> String {
        let n = |x: f64| x.to_string();
        let quests = self.quests(lo, hi);
        struct Hub {
            name: String,
            zone: Option<u32>,
            x: f64,
            y: f64,
            xp: f64,
            quests: Vec<String>,
        }
        // Zones by where you turn quests in; dungeon quests apart (they need a group).
        let mut zones: Vec<(String, f64, usize, Vec<Hub>)> = Vec::new();
        for q in &quests {
            let key = q.dungeon.map_or_else(|| zname(q.end.zone), |d| format!("[dungeon] {d}"));
            let zi = zones.iter().position(|z| z.0 == key).unwrap_or_else(|| {
                zones.push((key, 0.0, 0, Vec::new()));
                zones.len() - 1
            });
            let zone = &mut zones[zi];
            zone.1 += q.xp;
            zone.2 += 1;
            // Turn-ins within ~8 map units of each other count as one place.
            let near = zone.3.iter().position(|h| h.zone == q.end.zone && (h.x - q.end.x).hypot(h.y - q.end.y) < 8.0);
            let hi = near.unwrap_or_else(|| {
                let name = format!("{} ({}, {})", q.end.name, n(f64::round(q.end.x)), n(f64::round(q.end.y)));
                zone.3.push(Hub { name, zone: q.end.zone, x: q.end.x, y: q.end.y, xp: 0.0, quests: vec![] });
                zone.3.len() - 1
            });
            let hub = &mut zone.3[hi];
            hub.xp += q.xp;
            hub.quests.push(format!("{} ({}{}) {}", q.name, n(q.level), if q.in_log { ", in log" } else { "" }, n(q.xp)));
        }
        let total: f64 = quests.iter().map(|q| q.xp).sum();
        let mut out =
            vec![format!("{who}: level {}, quests {}-{} not done: {}, {} XP\n", n(self.level), n(lo), n(hi), quests.len(), n(total))];
        zones.sort_by(|a, b| b.1.total_cmp(&a.1));
        let wanted = only.map(str::to_lowercase);
        let shown = zones.iter_mut().filter(|z| wanted.as_ref().is_none_or(|w| z.0.to_lowercase().contains(w.as_str())));
        for (name, xp, count, hubs) in shown.take(if only.is_some() { 99 } else { 14 }) {
            out.push(format!("{name}: {} XP in {count} quests", n(*xp)));
            hubs.sort_by(|a, b| b.xp.total_cmp(&a.xp));
            for h in hubs.iter().take(if only.is_some() { 99 } else { 3 }) {
                out.push(format!("   hub {}: {} XP, {} quests", h.name, n(h.xp), h.quests.len()));
                if only.is_some() {
                    out.extend(h.quests.iter().map(|q| format!("      {q}")));
                }
            }
        }
        if only.is_some() {
            return out.join("\n");
        }
        // Chains: follow next-in-chain from quests with no prerequisite in this set.
        let by_id: HashMap<u32, &Quest> = quests.iter().map(|q| (q.id, q)).collect();
        let mut chains: Vec<(f64, Vec<&Quest>)> = Vec::new();
        for q in &quests {
            if q.prerequisites.iter().any(|p| by_id.contains_key(p)) {
                continue;
            }
            let mut chain = vec![q];
            let mut next = q.next;
            while let Some(nq) = next.and_then(|id| by_id.get(&id)) {
                if chain.len() > 15 {
                    break;
                }
                chain.push(nq);
                next = nq.next;
            }
            if chain.len() > 1 {
                chains.push((chain.iter().map(|q| q.xp).sum(), chain));
            }
        }
        chains.sort_by(|a, b| b.0.total_cmp(&a.0));
        out.push("\nLongest-paying chains:".into());
        for (xp, chain) in chains.iter().take(10) {
            let steps: Vec<String> = chain.iter().map(|q| format!("{} ({})", q.name, n(q.level))).collect();
            let from = zname(chain[0].start.as_ref().and_then(|s| s.zone));
            let to = zname(chain.last().unwrap().end.zone);
            let dungeon = if chain.iter().any(|q| q.dungeon.is_some()) { " [dungeon]" } else { "" };
            out.push(format!("  {} XP: {}  [{from} -> {to}]{dungeon}", n(*xp), steps.join(" -> ")));
        }
        out.join("\n")
    }

    /// The chain a quest is in, start to end. `gains`: reward item name -> score gain over your gear.
    pub fn chain(&self, quest_name: &str, gains: &HashMap<String, f64>) -> Result<String, String> {
        let n = |x: f64| x.to_string();
        let want = quest_name.to_lowercase();
        let mut start = None;
        for qid in self.db.ids_with("Quest", "S") {
            let sc = self.record(qid);
            let name = sc.as_ref().and_then(|s| s.get("1")).and_then(Value::as_text).unwrap_or_default();
            if name.to_lowercase() == want && self.who.allows(sc.as_ref()) {
                start = Some(qid);
                // Prefer the one you can still do.
                if self.in_log.contains(&qid) || !self.done.contains(&qid) {
                    break;
                }
            }
        }
        let Some(start) = start else { return Err(format!("No quest named \"{quest_name}\" for you in Questie's data.")) };
        let first_of = |f: &str, q: u32| vals(self.db.field("Quest", q, f).as_ref()).first().and_then(|v| id(Some(v)));
        let before = |q: u32| first_of("13", q).or_else(|| first_of("12", q));
        let after = |q: u32| id(self.record(q).as_ref().and_then(|s| s.get("22"))).or_else(|| first_of("14", q));
        let mut first = start;
        for _ in 0..20 {
            match before(first) {
                Some(p) => first = p,
                None => break,
            }
        }
        let mut steps = Vec::new();
        let mut q = Some(first);
        while let Some(id) = q {
            if steps.len() >= 25 {
                break;
            }
            steps.push(id);
            q = after(id);
        }
        // Reward items by quest (QuestieDB lists, per item, the quests that reward it).
        let mut rewards: HashMap<u32, Vec<String>> = HashMap::new();
        for item in self.db.ids_with("Item", "6") {
            for v in vals(self.db.field("Item", item, "6").as_ref()) {
                let Some(qid) = id(Some(v)).filter(|q| steps.contains(q)) else { continue };
                let name = self.db.field("Item", item, "S").and_then(|s| s.get("1").and_then(Value::as_text)).unwrap_or_default();
                rewards.entry(qid).or_default().push(name);
            }
        }
        let (mut left, mut best) = (0.0, 0.0f64);
        let mut zones: Vec<String> = Vec::new();
        let mut out = Vec::new();
        for &qid in &steps {
            let sc = self.record(qid);
            let get = |k: &str| sc.as_ref().and_then(|s| s.get(k));
            let (start, end) = (self.place("2", qid), self.place("3", qid));
            let done = self.done.contains(&qid);
            let status = if done {
                "done"
            } else if self.in_log.contains(&qid) {
                "in log"
            } else {
                ""
            };
            let qlvl = num(get("5")).unwrap_or(f64::NAN);
            let xp = self.xp.get(&qid).map_or(0.0, |&(_, xp)| self.earn(qlvl, xp));
            if !done {
                left += xp;
                for p in [&start, &end].into_iter().flatten() {
                    if p.zone.is_some() && !zones.contains(&zname(p.zone)) {
                        zones.push(zname(p.zone));
                    }
                }
            }
            let loot: Vec<String> = rewards
                .get(&qid)
                .into_iter()
                .flatten()
                .map(|name| match gains.get(name).copied().filter(|g| *g != 0.0) {
                    Some(g) => {
                        if g > best && !done {
                            best = g;
                        }
                        format!("{name} (+{})", n(g))
                    }
                    None => name.clone(),
                })
                .collect();
            let needs = num(get("4")).filter(|r| *r != 0.0).map_or(String::new(), |r| format!(", needs {}", n(r)));
            let from = start.as_ref().map_or(String::new(), |p| format!("  from {}, {}", p.name, zname(p.zone)));
            let to = end.as_ref().map_or(String::new(), |p| format!("  to {}, {}", p.name, zname(p.zone)));
            let dungeon = id(get("17")).and_then(dungeon_name).map_or(String::new(), |d| format!("  [{d}]"));
            let loot = if loot.is_empty() { String::new() } else { format!("\n         rewards: {}", loot.join(", ")) };
            let name = get("1").and_then(Value::as_text).unwrap_or_default();
            out.push(format!("{status:<6} {name} ({}{needs}) {} XP{from}{to}{dungeon}{loot}", n(qlvl), n(xp)));
        }
        let remaining = steps.iter().filter(|q| !self.done.contains(q)).count();
        let zones = if zones.is_empty() { "-".to_string() } else { zones.join(", ") };
        let best = if best > 0.0 { format!("; best reward +{} over your gear", n(best)) } else { String::new() };
        out.push(format!("\n{} steps, {remaining} left: {} XP across {zones}{best}", steps.len(), n(left)));
        Ok(out.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_falls_off_above_the_quest_level() {
        let db = QuestieDb::default();
        let xp = HashMap::new();
        let mut c = Character::default();
        c.identity.level = Some(40.0);
        let q = QuestXp::new(&db, &xp, &c);
        assert_eq!(q.earn(35.0, 1000.0), 1000.0);
        assert_eq!(q.earn(34.0, 1000.0), 800.0);
        assert_eq!(q.earn(31.0, 1000.0), 200.0);
        assert_eq!(q.earn(20.0, 1000.0), 100.0);
    }

    #[test]
    fn eligibility_from_faction_and_class() {
        let mut c = Character::default();
        c.identity.faction = Some("Alliance".into());
        c.identity.class = Some("ROGUE".into());
        let e = Eligibility::of(&c);
        let rec = |races: u64, classes: u64| {
            Value::Map(vec![(Value::Unsigned(6), Value::Unsigned(races)), (Value::Unsigned(7), Value::Unsigned(classes))])
        };
        assert!(e.allows(Some(&rec(0, 0))));
        assert!(e.allows(Some(&rec(1, 8)))); // human, rogue
        assert!(!e.allows(Some(&rec(2, 0)))); // orc only
        assert!(!e.allows(Some(&rec(0, 1)))); // warrior only
        assert!(e.allows(None));
    }
}
