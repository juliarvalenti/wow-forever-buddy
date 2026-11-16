//! The cheapest way up a profession, priced from Auctionator's last scan (`fbl-rs skillup`).
//!
//! For each skill point it picks the recipe with the lowest expected cost: reagent cost / chance of a
//! point. Reagents are priced at the cheapest of: auction house, vendor, crafting them, smelting them
//! (if Mining allows). What your characters hold (bags, bank, mail) is free and used up first.
//! What the game told the addon wins over the recipe file: learn levels from trainers you visited,
//! recipes you know and the tools they need from your profession window. Unconfirmed steps get a "?".

use crate::sources::addon::{AddonData, Character, Places};
use crate::sources::auctionator::PriceDb;
use crate::sources::lists::OrderedMap;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Recipe data per profession, shipped with the reader.
pub fn recipe_file(profession: &str) -> Option<&'static str> {
    match profession.to_lowercase().as_str() {
        "engineering" => Some(include_str!("../data/engineering.json")),
        _ => None,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfessionData {
    pub profession: String,
    pub caps: Vec<RankCap>,
    /// Item ids by name, for pricing.
    pub items: HashMap<String, u32>,
    /// How to smelt bars (needs Mining); other entries (notes) are ignored.
    #[serde(deserialize_with = "smelting_only")]
    pub smelting: HashMap<String, Smelt>,
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RankCap {
    pub cap: f64,
    pub rank: String,
    pub needs: Option<String>,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Smelt {
    pub mining: f64,
    pub makes: f64,
    pub reagents: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub name: String,
    /// The skill colours: orange from [0], yellow from [1], green from [2], grey from [3].
    pub skill: [f64; 4],
    #[serde(deserialize_with = "ordered_reagents")]
    pub reagents: Vec<(String, f64)>,
    pub makes: Option<f64>,
    /// Filled from what the game told the addon.
    #[serde(skip)]
    pub confirmed: bool,
    #[serde(skip)]
    pub learn: bool,
    #[serde(skip)]
    pub anvil: bool,
}

/// Reagents as `{ "name": count }`, kept in order.
fn ordered_reagents<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<(String, f64)>, D::Error> {
    let map = serde_json::Map::<String, Value>::deserialize(d)?;
    Ok(map.into_iter().filter_map(|(k, v)| Some((k, v.as_f64()?))).collect())
}

fn smelting_only<'de, D: serde::Deserializer<'de>>(d: D) -> Result<HashMap<String, Smelt>, D::Error> {
    #[derive(Deserialize)]
    struct Raw {
        mining: f64,
        makes: f64,
        #[serde(deserialize_with = "ordered_reagents")]
        reagents: Vec<(String, f64)>,
    }
    let map = serde_json::Map::<String, Value>::deserialize(d)?;
    Ok(map
        .into_iter()
        .filter_map(|(k, v)| Raw::deserialize(v).ok().map(|r| (k, Smelt { mining: r.mining, makes: r.makes, reagents: r.reagents })))
        .collect())
}

/// Vendor prices in copper (approximate; the AH is used when cheaper).
fn vendor_price(item: &str) -> Option<f64> {
    Some(match item {
        "Weak Flux" => 200.0,
        "Coal" => 500.0,
        "Wooden Stock" => 200.0,
        "Heavy Stock" => 800.0,
        "Nightcrawlers" => 50.0,
        _ => return None,
    })
}

/// Classic odds of a point: orange and yellow always, green falls off to grey.
pub fn chance(skill: &[f64; 4], at: f64) -> f64 {
    let [_, yellow, _, grey] = *skill;
    if at < yellow {
        1.0
    } else if at >= grey {
        0.0
    } else {
        ((grey - at) / (grey - yellow)).min(1.0)
    }
}

/// An insertion-ordered tally (what's made, used, bought), so output keeps the order things came up.
#[derive(Debug, Clone)]
struct Tally<V>(Vec<(String, V)>);

impl<V> Default for Tally<V> {
    fn default() -> Self {
        Tally(Vec::new())
    }
}

impl<V: Clone> Tally<V> {
    fn get_mut_or(&mut self, key: &str, default: V) -> &mut V {
        let i = match self.0.iter().position(|(k, _)| k == key) {
            Some(i) => i,
            None => {
                self.0.push((key.to_string(), default));
                self.0.len() - 1
            }
        };
        &mut self.0[i].1
    }
}

#[derive(Debug, Clone)]
struct Purchase {
    n: f64,
    each: f64,
    how: &'static str,
}

/// The planner's world: prices, what's held, the recipes and Mining.
struct Planner<'a> {
    ah: HashMap<String, f64>,
    recipes: HashMap<String, &'a Recipe>,
    smelting: &'a HashMap<String, Smelt>,
    mining: f64,
}

struct Ledger {
    bought: Tally<Purchase>,
    used: Tally<f64>,
    made: Tally<f64>,
}

impl Ledger {
    fn new() -> Ledger {
        Ledger { bought: Tally::default(), used: Tally::default(), made: Tally::default() }
    }
}

#[derive(Clone, Copy)]
enum Way<'a> {
    Buy(f64, &'static str),
    Craft(&'a Recipe),
    Smelt(&'a Smelt),
}

impl<'a> Planner<'a> {
    /// Gets `qty` of an item: from stock first (free), then the cheapest of buying, crafting or
    /// smelting. Changes `stock` and records what's bought, used and made. Returns the copper spent.
    fn acquire(
        &self,
        item: &str,
        qty: f64,
        skill: f64,
        stock: &mut HashMap<String, f64>,
        ledger: &mut Ledger,
        seen: &HashSet<String>,
    ) -> f64 {
        let held = stock.get(item).copied().unwrap_or(0.0);
        let free = held.min(qty);
        let mut qty = qty;
        if free > 0.0 {
            stock.insert(item.to_string(), held - free);
            *ledger.used.get_mut_or(item, 0.0) += free;
            qty -= free;
        }
        if qty <= 1e-9 {
            return 0.0;
        }
        if seen.contains(item) {
            return f64::INFINITY; // a loop: can't make it from itself
        }
        let mut seen = seen.clone();
        seen.insert(item.to_string());

        let mut ways: Vec<Way> = Vec::new();
        let ah = self.ah.get(item).copied().unwrap_or(f64::INFINITY);
        let buy_at = ah.min(vendor_price(item).unwrap_or(f64::INFINITY));
        if buy_at < f64::INFINITY {
            ways.push(Way::Buy(buy_at, if ah == buy_at { "AH" } else { "vendor" }));
        }
        if let Some(r) = self.recipes.get(item).filter(|r| r.skill[0] <= skill) {
            ways.push(Way::Craft(r));
        }
        if let Some(sm) = self.smelting.get(item).filter(|s| s.mining <= self.mining) {
            ways.push(Way::Smelt(sm));
        }
        // Price each way on a copy of the stock, then do the cheapest for real.
        let mut best: Option<(Way, f64)> = None;
        for way in &ways {
            let cost = self.run(*way, item, qty, skill, &mut stock.clone(), &mut Ledger::new(), &seen);
            if best.as_ref().is_none_or(|(_, c)| cost < *c) {
                best = Some((*way, cost));
            }
        }
        match best {
            Some((way, cost)) if cost < f64::INFINITY => self.run(way, item, qty, skill, stock, ledger, &seen),
            _ => f64::INFINITY,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        way: Way,
        item: &str,
        qty: f64,
        skill: f64,
        stock: &mut HashMap<String, f64>,
        ledger: &mut Ledger,
        seen: &HashSet<String>,
    ) -> f64 {
        let (reagents, per) = match way {
            Way::Buy(each, how) => {
                ledger.bought.get_mut_or(item, Purchase { n: 0.0, each, how }).n += qty;
                return qty * each;
            }
            Way::Craft(r) => {
                let per = r.makes.unwrap_or(1.0);
                *ledger.made.get_mut_or(item, 0.0) += qty / per;
                (&r.reagents, per)
            }
            Way::Smelt(s) => (&s.reagents, s.makes),
        };
        reagents.iter().map(|(k, n)| self.acquire(k, qty * n / per, skill, stock, ledger, seen)).sum()
    }
}

/// One stretch of the plan: crafting one recipe from `from` to `to`.
#[derive(Debug, Clone)]
pub struct Step {
    pub from: f64,
    pub to: f64,
    /// None when nothing trainable gives a point here.
    pub recipe: Option<String>,
    pub crafts: f64,
    pub total: f64,
    /// Intermediates made on the way (they give points too).
    pub made: Vec<(String, f64)>,
}

/// What the game says about the character's profession.
struct GameKnowledge {
    /// The profession window, when seen: recipe name -> its saved entry.
    known: Option<OrderedMap<Value>>,
    rank_seen: Option<f64>,
    /// Learn levels from trainers visited.
    taught: HashMap<String, f64>,
}

fn game_knowledge(addon: &AddonData, c: &Character, profession: &str) -> GameKnowledge {
    let window = c.professions.iter().find(|(n, _)| n == profession).map(|(_, p)| p);
    let known = window.map(|p| {
        let mut known = OrderedMap::default();
        for r in &p.recipes {
            if let Some(name) = r.get("name").and_then(Value::as_str) {
                known.insert(name.to_string(), r.clone());
            }
        }
        known
    });
    let mut taught: HashMap<String, f64> = HashMap::new();
    for t in addon.trainers.values() {
        for s in &t.services {
            let (Some(skill), Some(req), Some(name)) =
                (s.get("skill").and_then(Value::as_str), s.get("skill_req").and_then(Value::as_f64), s.get("name").and_then(Value::as_str))
            else {
                continue;
            };
            if skill == profession {
                let entry = taught.entry(name.to_string()).or_insert(f64::INFINITY);
                *entry = entry.min(req);
            }
        }
    }
    GameKnowledge { known, rank_seen: window.and_then(|p| p.rank.as_ref().and_then(Value::as_f64)), taught }
}

/// Applies what the game said to the recipe file: trainer levels, known recipes and tools, and the
/// window's colours (the truth at the rank it was seen at: stretch or cut the file's bands to fit).
fn apply_knowledge(recipes: &mut [Recipe], g: &GameKnowledge) {
    for r in recipes.iter_mut() {
        if let Some(&level) = g.taught.get(&r.name) {
            r.skill[0] = level;
        }
        let known = g.known.as_ref().and_then(|k| k.get(&r.name));
        r.confirmed = known.is_some() || g.taught.contains_key(&r.name);
        r.learn = g.known.is_some() && known.is_none();
        r.anvil = known.is_some_and(|k| {
            crate::json::list(k.get("tools")).iter().any(|t| t.as_str().is_some_and(|t| t.to_lowercase().contains("anvil")))
        });
    }
    let Some(seen) = g.rank_seen else { return };
    for r in recipes.iter_mut() {
        let colour = g.known.as_ref().and_then(|k| k.get(&r.name)).and_then(|k| k.get("color")).and_then(Value::as_str);
        let k = &mut r.skill;
        match colour {
            Some("orange") => k[1] = k[1].max(seen + 1.0),
            Some("yellow") => {
                k[1] = k[1].min(seen);
                k[2] = k[2].max(seen + 1.0);
            }
            Some("green") => {
                k[2] = k[2].min(seen);
                k[3] = k[3].max(seen + 1.0);
            }
            Some("grey") => k[3] = k[3].min(seen),
            _ => {}
        }
        for j in 1..4 {
            k[j] = k[j].max(k[j - 1]);
        }
    }
}

/// "12g 34s".
fn gs(c: f64) -> String {
    format!("{}g {}s", (c / 10000.0).floor(), ((c % 10000.0) / 100.0).floor())
}

pub struct SkillupOptions {
    pub to: f64,
    pub from: Option<f64>,
    /// Copper a craft's time is worth (0 for pure cash).
    pub craft_time: f64,
}

/// Plans the way up and returns the printed report.
pub fn plan(addon: &AddonData, prices: &PriceDb, c: &Character, data: &ProfessionData, o: &SkillupOptions) -> Result<String, String> {
    let rank = |name: &str| c.skills.iter().find(|s| s.name.as_deref() == Some(name)).and_then(|s| s.rank);
    if c.skills.is_empty() {
        return Err(format!("No skills saved for {}: log in, then /fbl r", c.key));
    }
    let start = o.from.or_else(|| rank(&data.profession)).ok_or_else(|| format!("{} has no {}", c.key, data.profession))?;
    let mining = rank("Mining").unwrap_or(0.0);

    // Prices (the first realm that has one) and what the characters hold.
    let mut ah = HashMap::new();
    let mut owned: HashMap<String, f64> = HashMap::new();
    let rows: Vec<_> = addon.characters.values().flat_map(|ch| ch.item_rows(Places::ALL)).collect();
    for (name, &id) in &data.items {
        if let Some(price) = prices.realms.iter().find_map(|r| r.get(&id.to_string()).and_then(|e| e.now).filter(|&p| p != 0.0)) {
            ah.insert(name.clone(), price);
        }
        let held: f64 = rows.iter().filter(|r| r.id == Some(id as f64)).map(|r| if r.count != 0.0 { r.count } else { 1.0 }).sum();
        owned.insert(name.clone(), held);
    }

    let knowledge = game_knowledge(addon, c, &data.profession);
    let mut recipes = data.recipes.clone();
    apply_knowledge(&mut recipes, &knowledge);
    let planner = Planner { ah, recipes: recipes.iter().map(|r| (r.name.clone(), r)).collect(), smelting: &data.smelting, mining };

    let mut stock = owned.clone();
    let mut ledger = Ledger::new();
    let mut steps: Vec<Step> = Vec::new();
    let no_seen = HashSet::new();
    // Skill is fractional: crafting intermediates (blasting powder, gizmos...) adds expected points on the way.
    let mut skill = start;
    while skill < o.to {
        let s = skill.floor();
        let mut best: Option<(&Recipe, f64, f64, f64, f64)> = None; // recipe, chance, cost, score, gain
        for r in &recipes {
            if r.skill[0] > s {
                continue;
            }
            let p = chance(&r.skill, s);
            if p == 0.0 {
                continue;
            }
            let mut trial = Ledger::new();
            let cost: f64 = r.reagents.iter().map(|(k, n)| planner.acquire(k, n / p, s, &mut stock.clone(), &mut trial, &no_seen)).sum();
            let bonus: f64 = trial.made.0.iter().map(|(k, n)| n * planner.recipes.get(k).map_or(0.0, |r| chance(&r.skill, s))).sum();
            let crafts: f64 = trial.made.0.iter().map(|(_, n)| n).sum();
            // Each craft also costs time: with free mats, a sure point beats a 1-in-3 green.
            let score = (cost + o.craft_time * (1.0 / p + crafts)) / (1.0 + bonus);
            if best.is_none_or(|b| score < b.3) {
                best = Some((r, p, cost, score, 1.0 + bonus));
            }
        }
        let Some((r, p, cost, _, gain)) = best.filter(|b| b.2 < f64::INFINITY) else {
            steps.push(Step { from: s, to: s + 1.0, recipe: None, crafts: 0.0, total: 0.0, made: vec![] });
            skill = s + 1.0;
            continue;
        };
        let made_before = ledger.made.0.clone();
        ledger.made = Tally::default();
        for (k, n) in &r.reagents {
            planner.acquire(k, n / p, s, &mut stock, &mut ledger, &no_seen);
        }
        let made = std::mem::replace(&mut ledger.made, Tally(made_before)).0;
        // What you make for points goes in your bags: later steps can use it.
        *stock.entry(r.name.clone()).or_insert(0.0) += (1.0 / p) * r.makes.unwrap_or(1.0);
        skill = o.to.min(skill + gain);
        let to = skill.floor();
        match steps.last_mut() {
            Some(last) if last.recipe.as_deref() == Some(r.name.as_str()) => {
                last.to = to;
                last.crafts += 1.0 / p;
                last.total += cost;
                for (k, n) in made {
                    match last.made.iter_mut().find(|(m, _)| *m == k) {
                        Some(entry) => entry.1 += n,
                        None => last.made.push((k, n)),
                    }
                }
            }
            _ => steps.push(Step { from: s, to, recipe: Some(r.name.clone()), crafts: 1.0 / p, total: cost, made }),
        }
    }
    Ok(render(data, &recipes, &knowledge, &steps, &ledger, &owned, start, mining, o.to))
}

#[allow(clippy::too_many_arguments)]
fn render(
    data: &ProfessionData,
    recipes: &[Recipe],
    g: &GameKnowledge,
    steps: &[Step],
    ledger: &Ledger,
    owned: &HashMap<String, f64>,
    start: f64,
    mining: f64,
    to: f64,
) -> String {
    let n = crate::json::number_string;
    let mut out = vec![format!("{} {} -> {}, Mining {}", data.profession, n(start), n(to), n(mining))];
    let window = match &g.known {
        Some(k) => format!("{} known recipes (window seen)", k.len()),
        None => "profession window not seen".into(),
    };
    out.push(format!("  game data: {window}, {} trainer entries; ? = level from the web, not confirmed in game", g.taught.len()));
    let unlisted: Vec<&String> =
        g.known.iter().flat_map(|k| k.iter().map(|(name, _)| name)).filter(|name| !recipes.iter().any(|r| &r.name == *name)).collect();
    if !unlisted.is_empty() {
        out.push(format!(
            "  known but not in the recipe file (no colour data, not planned): {}",
            unlisted.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    // Each rank is trained when the one before it is maxed.
    for (k, cap) in data.caps.iter().enumerate().skip(1) {
        let at = data.caps[k - 1].cap;
        if at >= start && at < to {
            out.push(format!(
                "  at {}: train {} ({}, {})",
                n(at),
                cap.rank,
                cap.needs.as_deref().unwrap_or("?"),
                gs(cap.cost.unwrap_or(0.0))
            ));
        }
    }
    let mut total = 0.0;
    for st in steps {
        let Some(name) = &st.recipe else {
            out.push(format!("  {}: nothing trainable gives a point", n(st.from)));
            continue;
        };
        total += st.total;
        let r = recipes.iter().find(|r| &r.name == name).unwrap();
        let first: Vec<String> = st.made.iter().map(|(k, m)| format!("{} {k}", m.ceil())).collect();
        let tag = format!(
            "{}{}{}",
            if r.confirmed { "" } else { "?" },
            if r.anvil { " [anvil]" } else { "" },
            if r.learn { " (learn it)" } else { "" }
        );
        let first = if first.is_empty() { String::new() } else { format!("  (first make {})", first.join(", ")) };
        out.push(format!(
            "  {}-{}  {:<28} ~{} crafts  {}{first}",
            n(st.from),
            n(st.to),
            format!("{name}{tag}"),
            st.crafts.ceil(),
            gs(st.total)
        ));
    }
    out.push(format!("  expected cost {}", gs(total)));
    out.push("From what you hold:".into());
    let mut used = ledger.used.0.clone();
    used.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (k, u) in used {
        out.push(format!("  {:>4} {k}  (of {})", u.ceil(), n(owned.get(&k).copied().unwrap_or(0.0))));
    }
    out.push("To buy:".into());
    let mut bought = ledger.bought.0.clone();
    bought.sort_by(|a, b| (b.1.n * b.1.each).total_cmp(&(a.1.n * a.1.each)));
    let mut spend = 0.0;
    for (k, b) in bought {
        let count = b.n.ceil();
        spend += count * b.each;
        out.push(format!(
            "  {:>4} {:<24} {:<6} @ {}s  = {}",
            count,
            k,
            b.how,
            crate::json::to_fixed(b.each / 100.0, 2),
            gs(count * b.each)
        ));
    }
    out.push(format!("  to spend ~{}", gs(spend)));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odds_of_a_point() {
        let k = [30.0, 45.0, 52.0, 60.0];
        assert_eq!(chance(&k, 40.0), 1.0); // orange
        assert_eq!(chance(&k, 45.0), 1.0); // yellow starts: (60 - 45) / 15
        assert!((chance(&k, 55.0) - 5.0 / 15.0).abs() < 1e-12);
        assert_eq!(chance(&k, 60.0), 0.0); // grey
    }

    #[test]
    fn recipe_file_reads() {
        let data: ProfessionData = serde_json::from_str(recipe_file("Engineering").unwrap()).unwrap();
        assert_eq!(data.profession, "Engineering");
        assert!(data.recipes.iter().any(|r| r.name == "Handful of Copper Bolts"));
        assert_eq!(data.smelting["Copper Bar"].reagents, [("Copper Ore".to_string(), 1.0)]);
    }

    #[test]
    fn plans_with_free_stock_first() {
        let data: ProfessionData = serde_json::from_str(
            r#"{"profession":"Engineering","caps":[{"cap":75,"rank":"Apprentice"}],"items":{"Copper Bar":2840},
                "smelting":{"note":"x"},
                "recipes":[{"name":"Handful of Copper Bolts","skill":[1,30,45,60],"reagents":{"Copper Bar":1}}]}"#,
        )
        .unwrap();
        let c = Character {
            key: "Someone-Realm".into(),
            skills: vec![serde_json::from_value(serde_json::json!({"name": "Engineering", "rank": 1, "max": 75})).unwrap()],
            ..Character::default()
        };
        let mut prices = PriceDb::default();
        prices
            .realms
            .push(crate::sources::auctionator::realm_from_json("R", &serde_json::json!({"version": 2, "2840": {"m": 50, "h": {"1": 50}}})));
        let text = plan(&AddonData::default(), &prices, &c, &data, &SkillupOptions { to: 4.0, from: None, craft_time: 0.0 }).unwrap();
        assert!(text.contains("  1-4  Handful of Copper Bolts?     ~3 crafts  0g 1s"), "{text}");
        assert!(text.contains("     3 Copper Bar               AH     @ 0.50s  = 0g 1s"), "{text}");
    }
}
