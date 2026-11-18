//! Gear upgrades: reads an item's stats from its tooltip lines, scores them with the character's
//! leveling weights, checks the character can use it, and compares with what's worn.

use super::talents::talent_trees;

use crate::sources::addon::{Character, ItemRecord};
use regex::Regex;
use std::sync::OnceLock;

/// What a tooltip says about an item.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemStats {
    /// Stat totals in the order the tooltip lists them: "sta", "int", "sp", "sp_fire", "crit", "dps"...
    pub values: Vec<(String, f64)>,
    /// "Classes: Rogue, Warrior".
    pub classes: Option<Vec<String>>,
    /// "Requires Level 20".
    pub req: Option<f64>,
    /// "Requires Engineering (120)": a profession at a rank.
    pub skill_req: Vec<(String, f64)>,
    /// Any other "Requires ...": a reputation, a spec.
    pub other_req: Vec<String>,
}

impl ItemStats {
    fn add(&mut self, key: &str, amount: f64) {
        match self.values.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v += amount,
            None => self.values.push((key.to_string(), amount)),
        }
    }

    /// "sta 6, int 7".
    pub fn summary(&self) -> String {
        self.values.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")
    }
}

/// The tooltip patterns, tried in order; the first that matches a line wins.
struct Patterns {
    primary: Regex,
    spell_power: Regex,
    school_power: Regex,
    healing: Regex,
    spell_crit: Regex,
    crit: Regex,
    spell_hit: Regex,
    hit: Regex,
    mp5: Regex,
    attack_power: Regex,
    dps: Regex,
    armor: Regex,
    classes: Regex,
    level: Regex,
    skill: Regex,
    other: Regex,
    /// A bonus that only applies sometimes: against a creature type ("+18 Attack Power when fighting
    /// Mechanical units"), in a form, while something holds. Not an all-the-time stat.
    conditional: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| {
        let r = |s: &str| Regex::new(s).unwrap();
        Patterns {
            primary: r(r"^\+([0-9]+) (Stamina|Intellect|Spirit|Agility|Strength)$"),
            spell_power: r(r"damage and healing done by magical spells and effects by up to ([0-9]+)"),
            school_power: r(r"damage done by ([A-Za-z0-9_]+) spells and effects by up to ([0-9]+)"),
            healing: r(r"healing done by spells and effects by up to ([0-9]+)"),
            spell_crit: r(r"critical strike with spells by ([0-9]+)%"),
            crit: r(r"critical strike by ([0-9]+)%"),
            spell_hit: r(r"chance to hit with spells by ([0-9]+)%"),
            hit: r(r"chance to hit by ([0-9]+)%"),
            mp5: r(r"Restores ([0-9]+) mana per 5 sec"),
            attack_power: r(r"(?i)(?:\+([0-9]+) Attack Power|attack power by ([0-9]+))"),
            dps: r(r"\(([0-9.]+) damage per second\)"),
            armor: r(r"^([0-9]+) Armor$"),
            classes: r(r"^Classes: (.*)$"),
            level: r(r"^Requires Level ([0-9]+)$"),
            skill: r(r"^Requires (.+?) \(([0-9]+)\)$"),
            other: r(r"^Requires (.+)$"),
            conditional: r(
                r"(?i)\b(when fighting|against|versus|vs\.?)\b|\b(Undead|Demons?|Beasts?|Mechanicals?|Dragonkin|Elementals?|Giants?|Humanoids?)\b|forms? only|while in ",
            ),
        }
    })
}

fn num(s: &str) -> f64 {
    s.parse().unwrap_or(f64::NAN)
}

/// Reads stats from the tooltip lines the addon keeps (the client's own text, so a random suffix's
/// stats are in them).
pub fn parse_stats(lines: &[String]) -> ItemStats {
    let p = patterns();
    let mut s = ItemStats::default();
    for line in lines {
        let t = line.as_str();
        // Requirement lines ("Requires Level 20") never are; anything else conditional is skipped.
        if !t.starts_with("Requires") && !t.starts_with("Classes:") && p.conditional.is_match(t) {
            continue;
        }
        if let Some(m) = p.primary.captures(t) {
            let key = match &m[2] {
                "Stamina" => "sta",
                "Intellect" => "int",
                "Spirit" => "spi",
                "Agility" => "agi",
                _ => "str",
            };
            s.add(key, num(&m[1]));
        } else if let Some(m) = p.spell_power.captures(t) {
            s.add("sp", num(&m[1]));
        } else if let Some(m) = p.school_power.captures(t) {
            s.add(&format!("sp_{}", m[1].to_lowercase()), num(&m[2]));
        } else if let Some(m) = p.healing.captures(t) {
            s.add("heal", num(&m[1]));
        } else if let Some(m) = p.spell_crit.captures(t) {
            s.add("spellcrit", num(&m[1]));
        } else if let Some(m) = p.crit.captures(t) {
            s.add("crit", num(&m[1]));
        } else if let Some(m) = p.spell_hit.captures(t) {
            s.add("spellhit", num(&m[1]));
        } else if let Some(m) = p.hit.captures(t) {
            s.add("hit", num(&m[1]));
        } else if let Some(m) = p.mp5.captures(t) {
            s.add("mp5", num(&m[1]));
        } else if let Some(m) = p.attack_power.captures(t) {
            s.add("ap", num(m.get(1).or(m.get(2)).unwrap().as_str()));
        } else if let Some(m) = p.dps.captures(t) {
            s.add("dps", num(&m[1]));
        } else if let Some(m) = p.armor.captures(t) {
            s.add("armor", num(&m[1]));
        } else if let Some(m) = p.classes.captures(t) {
            s.classes = Some(m[1].split(',').map(|c| c.trim_start().to_string()).collect());
        } else if let Some(m) = p.level.captures(t) {
            s.req = Some(num(&m[1]));
        } else if let Some(m) = p.skill.captures(t) {
            s.skill_req.push((m[1].to_string(), num(&m[2])));
        } else if let Some(m) = p.other.captures(t) {
            s.other_req.push(m[1].to_string());
        }
    }
    s
}

/// How a character levels: which stats matter, and how much.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Caster,
    Healer,
    Agility,
    Hunter,
    Strength,
    Tank,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Caster => "caster",
            Role::Healer => "healer",
            Role::Agility => "agility",
            Role::Hunter => "hunter",
            Role::Strength => "strength",
            Role::Tank => "tank",
        }
    }

    /// Leveling stat weights per point. Casters follow Venruki's rule: Stamina and Intellect first,
    /// spell power a bonus, Spirit near nothing. `sp_main` is spell damage of the spec's own school;
    /// other schools count for nothing. Crit and hit are per percent, so they weigh more.
    pub fn weights(self) -> Vec<(String, f64)> {
        let w: &[(&str, f64)] = match self {
            Role::Caster => &[
                ("sta", 1.0),
                ("int", 1.0),
                ("sp", 0.8),
                ("sp_main", 0.8),
                ("spi", 0.1),
                ("spellcrit", 6.0),
                ("spellhit", 6.0),
                ("mp5", 1.5),
            ],
            Role::Healer => &[("int", 1.0), ("sta", 0.7), ("heal", 0.6), ("sp", 0.6), ("spi", 0.4), ("mp5", 2.0), ("spellcrit", 5.0)],
            Role::Agility => &[("agi", 1.0), ("sta", 0.6), ("str", 0.5), ("ap", 0.5), ("crit", 12.0), ("hit", 12.0), ("dps", 3.0)],
            Role::Hunter => &[("agi", 1.0), ("sta", 0.6), ("int", 0.3), ("ap", 0.4), ("crit", 10.0), ("hit", 10.0), ("dps", 2.0)],
            Role::Strength => &[("str", 1.0), ("agi", 0.6), ("sta", 0.7), ("ap", 0.5), ("crit", 10.0), ("hit", 10.0), ("dps", 3.0)],
            Role::Tank => &[("sta", 1.2), ("str", 0.8), ("agi", 0.6), ("armor", 0.05), ("dps", 1.5), ("hit", 5.0)],
        };
        w.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    /// The role for a class and its top talent tree (None before any points are spent).
    pub fn of(class: Option<&str>, tree: Option<&str>) -> Role {
        match (class.unwrap_or(""), tree) {
            ("MAGE" | "WARLOCK", _) => Role::Caster,
            ("ROGUE", _) => Role::Agility,
            ("HUNTER", _) => Role::Hunter,
            ("PRIEST", Some("Shadow") | None) => Role::Caster,
            ("PRIEST", Some(_)) => Role::Healer,
            ("DRUID", Some("Restoration")) => Role::Healer,
            ("DRUID", Some("Feral Combat")) => Role::Agility,
            ("DRUID", _) => Role::Caster,
            ("SHAMAN", Some("Restoration")) => Role::Healer,
            ("SHAMAN", Some("Enhancement")) => Role::Strength,
            ("SHAMAN", _) => Role::Caster,
            ("PALADIN", Some("Holy")) => Role::Healer,
            ("PALADIN", Some("Protection")) => Role::Tank,
            ("WARRIOR", Some("Protection")) => Role::Tank,
            _ => Role::Strength,
        }
    }
}

/// The spell schools whose damage counts for a tree's spec.
fn main_schools(tree: Option<&str>) -> &'static [&'static str] {
    match tree.unwrap_or("") {
        "Arcane" => &["arcane"],
        "Fire" => &["fire"],
        "Frost" => &["frost"],
        "Shadow" | "Affliction" | "Demonology" => &["shadow"],
        "Destruction" => &["fire", "shadow"],
        "Balance" => &["nature", "arcane"],
        "Elemental" => &["nature", "fire", "frost"],
        _ => &[],
    }
}

/// A character's scoring profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub role: Role,
    /// The tree with the most points, once any are spent.
    pub tree: Option<String>,
    /// Weights in order (overrides replace or append).
    pub weights: Vec<(String, f64)>,
    pub schools: &'static [&'static str],
}

impl Profile {
    fn weight(&self, key: &str) -> Option<f64> {
        self.weights.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
    }
}

/// A number typed by the user ("0.8", " 2 "); None when it isn't one.
pub fn parse_number(text: &str) -> Option<f64> {
    text.trim().parse().ok().filter(|x: &f64| x.is_finite())
}

/// The profile from class and talents, with `overrides` like "int=1,sta=1,sp=0.8" applied.
pub fn profile(c: &Character, overrides: Option<&str>) -> Profile {
    let trees = talent_trees(c).map(|t| t.trees).unwrap_or_default();
    // The first tree with the most points (ties keep the earlier tree).
    let top = trees.iter().fold(None::<&super::talents::Tree>, |best, t| {
        if t.points.unwrap_or(0.0) > best.and_then(|b| b.points).unwrap_or(0.0) {
            Some(t)
        } else {
            best
        }
    });
    let tree = top.filter(|t| t.points.unwrap_or(0.0) > 0.0).and_then(|t| t.name.clone());
    let role = Role::of(c.identity.class.as_deref(), tree.as_deref());
    let mut weights = role.weights();
    for part in overrides.unwrap_or("").split(',').filter(|p| !p.is_empty()) {
        let mut kv = part.split('=');
        // "name=number"; anything else is ignored.
        let (Some(k), Some(v)) = (kv.next().map(str::trim).filter(|k| !k.is_empty()), kv.next().and_then(parse_number)) else { continue };
        let k = k.to_string();
        match weights.iter_mut().find(|(name, _)| *name == k) {
            Some(slot) => slot.1 = v,
            None => weights.push((k, v)),
        }
    }
    let schools = main_schools(tree.as_deref());
    Profile { role, tree, weights, schools }
}

/// An item's worth to this character: each stat times its weight, to one decimal.
pub fn score(stats: &ItemStats, p: &Profile) -> f64 {
    let total: f64 = stats
        .values
        .iter()
        .map(|(k, v)| {
            let weight = match k.strip_prefix("sp_") {
                Some(school) if p.schools.contains(&school) => p.weight("sp_main").unwrap_or(0.0),
                Some(_) => 0.0,
                None => p.weight(k).filter(|w| *w != 0.0 && !w.is_nan()).unwrap_or(0.0),
            };
            v * weight
        })
        .sum();
    f64::round(total * 10.0) / 10.0
}

/// Item subtypes named differently in the skill list.
fn skill_for(subtype: &str) -> &str {
    match subtype {
        "One-Handed Swords" => "Swords",
        "One-Handed Axes" => "Axes",
        "One-Handed Maces" => "Maces",
        "Plate" => "Plate Mail",
        "Shields" => "Shield",
        other => other,
    }
}

/// Slots anyone can wear, whatever their skills.
const ANYONE: [&str; 5] = ["INVTYPE_CLOAK", "INVTYPE_FINGER", "INVTYPE_NECK", "INVTYPE_TRINKET", "INVTYPE_HOLDABLE"];

/// Whether the character can use the item, or why not ("class", "needs Leather", "needs Engineering 120").
pub fn can_use(c: &Character, item: &ItemRecord, stats: &ItemStats) -> Result<(), String> {
    if let (Some(classes), Some(class)) = (&stats.classes, c.identity.class.as_deref().filter(|s| !s.is_empty())) {
        let matches = |x: &String| x.to_uppercase().chars().filter(|ch| !ch.is_whitespace()).collect::<String>() == class;
        if !classes.iter().any(matches) {
            return Err("class".into());
        }
    }
    for (name, rank) in &stats.skill_req {
        let have = c.skills.iter().find(|s| s.name.as_deref() == Some(name.as_str()));
        if have.is_none_or(|s| s.rank.unwrap_or(0.0) < *rank) {
            return Err(format!("needs {name} {rank}"));
        }
    }
    if let Some(first) = stats.other_req.first() {
        return Err(format!("needs {first}"));
    }
    let skills: Vec<&str> = c.skills.iter().filter_map(|s| s.name.as_deref()).collect();
    if skills.is_empty() || item.slot.as_deref().is_some_and(|s| ANYONE.contains(&s)) {
        return Ok(());
    }
    // Without a subtype there's no skill to check.
    let Some(subtype) = item.subtype.as_deref() else { return Ok(()) };
    if item.class == Some(4.0) && (subtype == "Miscellaneous" || subtype == "Cosmetic") {
        return Ok(()); // armor anyone can wear: rings, trinkets, shirts
    }
    let need = skill_for(subtype);
    if skills.contains(&need) {
        Ok(())
    } else {
        Err(format!("needs {need}"))
    }
}

/// Inventory slot numbers an item type goes in.
pub fn slots_for(inventory_type: &str) -> Option<&'static [usize]> {
    Some(match inventory_type {
        "INVTYPE_HEAD" => &[1],
        "INVTYPE_NECK" => &[2],
        "INVTYPE_SHOULDER" => &[3],
        "INVTYPE_CHEST" | "INVTYPE_ROBE" => &[5],
        "INVTYPE_WAIST" => &[6],
        "INVTYPE_LEGS" => &[7],
        "INVTYPE_FEET" => &[8],
        "INVTYPE_WRIST" => &[9],
        "INVTYPE_HAND" => &[10],
        "INVTYPE_FINGER" => &[11, 12],
        "INVTYPE_TRINKET" => &[13, 14],
        "INVTYPE_CLOAK" => &[15],
        "INVTYPE_WEAPON" | "INVTYPE_WEAPONMAINHAND" | "INVTYPE_2HWEAPON" => &[16],
        "INVTYPE_WEAPONOFFHAND" | "INVTYPE_HOLDABLE" | "INVTYPE_SHIELD" => &[17],
        "INVTYPE_RANGED" | "INVTYPE_RANGEDRIGHT" | "INVTYPE_THROWN" => &[18],
        _ => return None,
    })
}

/// The level an item needs: the tooltip's, the game's, the quest's that rewards it (quest rewards
/// often have none: they're yours when the quest is), or about item level - 5. None when unknown.
pub fn required_level(item: &ItemRecord, stats: &ItemStats) -> Option<f64> {
    let quest_level = item
        .quests
        .as_ref()
        .map(|qs| qs.iter().map(|q| q.level.filter(|l| *l != 0.0).unwrap_or(f64::INFINITY)).fold(f64::INFINITY, f64::min))
        .filter(|l| l.is_finite());
    stats.req.or(item.req.filter(|r| *r > 0.0)).or(quest_level).or(item.ilvl.filter(|i| *i != 0.0).map(|i| (i - 5.0).max(1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_tooltip_stats() {
        let s = parse_stats(&lines(&[
            "+6 Stamina",
            "+7 Intellect",
            "Equip: Increases damage done by Fire spells and effects by up to 40.",
            "Equip: Increases damage and healing done by magical spells and effects by up to 30.",
            "Equip: Improves your chance to get a critical strike by 1%.",
            "Equip: +20 Attack Power.",
            "(12.5 damage per second)",
            "91 Armor",
            "Classes: Rogue, Warrior",
            "Requires Level 20",
            "Requires Engineering (120)",
            "Requires Honored with Stormwind",
        ]));
        assert_eq!(s.summary(), "sta 6, int 7, sp_fire 40, sp 30, crit 1, ap 20, dps 12.5, armor 91");
        assert_eq!(s.classes, Some(vec!["Rogue".to_string(), "Warrior".to_string()]));
        assert_eq!(s.req, Some(20.0));
        assert_eq!(s.skill_req, [("Engineering".to_string(), 120.0)]);
        assert_eq!(s.other_req, ["Honored with Stormwind"]);
    }

    #[test]
    fn conditional_bonuses_are_not_stats() {
        // Gnomish Universal Remote and friends: only against some targets, or in a form.
        let s = parse_stats(&lines(&[
            "Equip: +18 Attack Power when fighting Mechanical units.",
            "Equip: Increases attack power by 45 when fighting Undead.",
            "Equip: +120 Attack Power in Cat, Bear, and Dire Bear forms only.",
            "Equip: +10 Attack Power.",
            "+5 Agility",
        ]));
        assert_eq!(s.summary(), "ap 10, agi 5");
    }

    #[test]
    fn roles_by_class_and_tree() {
        assert_eq!(Role::of(Some("PRIEST"), None), Role::Caster);
        assert_eq!(Role::of(Some("PRIEST"), Some("Holy")), Role::Healer);
        assert_eq!(Role::of(Some("DRUID"), Some("Feral Combat")), Role::Agility);
        assert_eq!(Role::of(Some("WARRIOR"), Some("Arms")), Role::Strength);
        assert_eq!(Role::of(None, None), Role::Strength);
    }

    #[test]
    fn scores_with_weights_and_schools() {
        let c = Character::default();
        let mut p = profile(&c, Some("sp=2, sta = 3,x=,=5"));
        assert_eq!(p.role, Role::Strength);
        assert_eq!(p.weights.iter().find(|(k, _)| k == "sp").unwrap().1, 2.0);
        assert_eq!(p.weights.iter().find(|(k, _)| k == " sta").map(|w| w.1), None); // keys are trimmed
        assert_eq!(p.weights.iter().find(|(k, _)| k == "sta").unwrap().1, 3.0);
        assert!(p.weights.iter().all(|(k, _)| k != "x" && !k.is_empty())); // no number, no name: ignored
        p.schools = &["fire"];
        p.weights.push(("sp_main".into(), 0.5));
        let s = parse_stats(&lines(&[
            "+2 Stamina",
            "Equip: Increases damage done by Fire spells and effects by up to 10.",
            "Equip: Increases damage done by Frost spells and effects by up to 10.",
        ]));
        assert_eq!(score(&s, &p), 2.0 * 3.0 + 10.0 * 0.5);
    }

    #[test]
    fn required_level_falls_back() {
        let none = ItemStats::default();
        let mut item = ItemRecord { ilvl: Some(25.0), ..ItemRecord::default() };
        assert_eq!(required_level(&item, &none), Some(20.0));
        item.req = Some(0.0);
        assert_eq!(required_level(&item, &none), Some(20.0));
        item.quests = Some(vec![]);
        assert_eq!(required_level(&item, &none), Some(20.0));
        item.quests = Some(vec![crate::sources::addon::QuestRef { level: Some(18.0), ..Default::default() }]);
        assert_eq!(required_level(&item, &none), Some(18.0));
        item.req = Some(22.0);
        assert_eq!(required_level(&item, &none), Some(22.0));
        assert_eq!(required_level(&ItemRecord::default(), &none), None);
    }
}
