//! Commands that use auction prices: `prices`, `flips`, `value` (Auctionator) and `upgrades` (the
//! addon's gear scan).

use super::upgrades::{can_use, parse_stats, profile, required_level, score, slots_for};
use super::{number_arg, pick, Args, Ctx, Obj};
use crate::format::{auctionator_date, money_long};
use crate::json::number;
use crate::model::usual_price;
use crate::sources::addon::{ItemRecord, Places, SLOT_NAMES};
use crate::sources::auctionator::{self, PriceDb, PriceEntry};
use crate::sources::links::parse_link;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

/// How many days of history the price commands look at.
const RECENT_DAYS: usize = 14;

fn load_prices(ctx: &Ctx, hint: &str) -> Result<PriceDb, String> {
    let db = auctionator::load(ctx.env)?;
    if db.realms.is_empty() {
        return Err(format!("No Auctionator prices found. {hint}"));
    }
    Ok(db)
}

/// The last `RECENT_DAYS` days of an item's history.
fn recent(entry: &PriceEntry) -> &[auctionator::DayPrice] {
    &entry.days[entry.days.len().saturating_sub(RECENT_DAYS)..]
}

/// The days as JSON: date, the day's low and high cheapest, and how many were up.
fn history(entry: &PriceEntry) -> Vec<Value> {
    recent(entry)
        .iter()
        // A day's low can't be above its high (Auctionator sometimes records one that is).
        .map(|d| {
            Obj::new()
                .set("day", auctionator_date(d.day))
                .set("low", d.cheapest().min(d.high))
                .set("high", d.high)
                .opt("listed", d.listed)
                .build()
        })
        .collect()
}

/// Item ids to names, from every item any character has (first seen first; a later name wins).
fn names_from_characters(ctx: &Ctx) -> Vec<(u32, Option<String>)> {
    let mut out: Vec<(u32, Option<String>)> = Vec::new();
    let mut at: HashMap<u32, usize> = HashMap::new();
    for c in ctx.data.characters.values() {
        for r in c.item_rows(Places::ALL) {
            let Some(id) = r.id.filter(|&id| id > 0.0).map(|id| id as u32) else { continue };
            match at.get(&id) {
                Some(&i) => out[i].1 = r.name.clone().or(out[i].1.take()),
                None => {
                    at.insert(id, out.len());
                    out.push((id, r.name.clone()));
                }
            }
        }
    }
    out
}

pub fn prices(ctx: &Ctx, query: Option<&str>) -> Result<Value, String> {
    let query = query.ok_or("query is required")?;
    let db = load_prices(ctx, "Install Auctionator, run a full scan at the auction house, then /reload.")?;
    let ids = names_from_characters(ctx);
    let name_of = |id: u32| ids.iter().find(|(i, _)| *i == id).and_then(|(_, n)| n.clone());
    let q = query.to_lowercase();
    let wanted: Vec<u32> = match q.parse::<u32>() {
        Ok(id) => vec![id],
        Err(_) => ids.iter().filter(|(_, n)| n.as_deref().unwrap_or("").to_lowercase().contains(&q)).map(|(id, _)| *id).collect(),
    };
    if wanted.is_empty() {
        return Err(format!("No item named like \"{query}\" in your characters' items; ask by item id instead."));
    }
    let mut out = Vec::new();
    for realm in &db.realms {
        for &id in &wanted {
            let row = Obj::new().set("realm", realm.realm.as_str()).set("id", id).opt("name", name_of(id));
            out.push(match realm.get(&id.to_string()) {
                Some(entry) => row
                    .opt("last", entry.now.map(money_long))
                    .opt("last_copper", entry.now)
                    .opt("usual", usual_price(recent(entry)).map(|u| money_long(u as f64)))
                    .set("history", history(entry))
                    .build(),
                None => row.set("last", Value::Null).set("note", "not seen on the auction house").build(),
            });
        }
    }
    Ok(Obj::new().opt("full_scan", ctx.when(db.scan_at)).set("prices", out).build())
}

/// `flips` options, from the tool's arguments.
struct FlipOptions {
    /// The most to pay, copper.
    budget: f64,
    /// Now at most this share of usual.
    ratio: f64,
    /// Days of history an item needs.
    min_days: usize,
    /// The least profit per item after the cut, copper.
    min_profit: f64,
    limit: usize,
}

impl FlipOptions {
    fn from_args(args: &Args) -> Result<FlipOptions, String> {
        Ok(FlipOptions {
            budget: number_arg(args, "budget", f64::INFINITY)? * 10000.0,
            ratio: number_arg(args, "ratio", 0.7)?,
            min_days: number_arg(args, "min_days", 3.0)?.max(1.0) as usize,
            min_profit: number_arg(args, "min_profit", 0.2)? * 10000.0,
            limit: number_arg(args, "limit", 25.0)?.max(0.0) as usize,
        })
    }
}

/// Bargains: the cheapest now is well under the item's usual price, it's listed often, and the
/// resale clears the cut.
pub fn flips(ctx: &Ctx, args: &Args) -> Result<Value, String> {
    let db = load_prices(ctx, "Run a full scan at the auction house, then /reload.")?;
    let o = FlipOptions::from_args(args)?;
    let mut names: HashMap<u32, String> = names_from_characters(ctx).into_iter().filter_map(|(id, n)| Some((id, n?))).collect();
    for (_, it) in ctx.data.ah.iter().flat_map(|ah| ah.items.iter().flatten()) {
        if let (Some(id), Some(name)) = (it.id.filter(|&i| i > 0.0), it.name.as_ref().filter(|n| !n.is_empty())) {
            names.entry(id as u32).or_insert_with(|| name.clone());
        }
    }
    let mut found: Vec<(f64, Value)> = Vec::new();
    let mut deepest = 0;
    for realm in &db.realms {
        for e in realm.items.iter().filter(|e| e.has_history) {
            let days = recent(e);
            deepest = deepest.max(days.len());
            let (Some(now), Some(id)) = (e.now.filter(|&m| m != 0.0), e.id()) else { continue };
            if days.len() < o.min_days.max(2) || now > o.budget {
                continue;
            }
            let Some(usual) = usual_price(days).map(|u| u as f64) else { continue };
            let avg_listed = days.iter().map(|d| d.listed.unwrap_or(0.0)).sum::<f64>() / days.len() as f64;
            let profit = (usual * 0.95).floor() - now;
            if now > usual * o.ratio || profit < o.min_profit || avg_listed < 3.0 {
                continue;
            }
            let row = Obj::new()
                .set("realm", realm.realm.as_str())
                .set("id", id)
                .opt("name", names.get(&id).cloned())
                .set("now", money_long(now))
                .set("usual", money_long(usual))
                .set("profit", money_long(profit))
                .set("share", format!("{}%", (now / usual * 100.0).round()))
                .set("days", days.len())
                .set("avg_listed", number(avg_listed.round()));
            found.push((profit, row.build()));
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    let note = (deepest < o.min_days)
        .then(|| format!("Only {deepest} day(s) of scans so far; flips need {}. Run a full Auctionator scan once a day.", o.min_days));
    Ok(Obj::new()
        .opt("full_scan", ctx.when(db.scan_at))
        .set("days_of_history", deepest)
        .opt("note", note)
        .set("flips", found.into_iter().take(o.limit).map(|(_, v)| v).collect::<Vec<_>>())
        .build())
}

/// What a character's bags and bank would fetch at today's prices. Soulbound items are left out.
pub fn value(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let db = load_prices(ctx, "Run a full scan with Auctionator, then /reload.")?;
    let home = c.identity.realm.as_deref().unwrap_or("").to_lowercase();
    let realm = db.realms.iter().find(|r| r.realm.to_lowercase().starts_with(&home)).unwrap_or(&db.realms[0]);
    struct Holding {
        name: Option<String>,
        count: f64,
        each: f64,
        places: Vec<&'static str>,
    }
    // By id, in id order.
    let mut held: BTreeMap<i64, Holding> = BTreeMap::new();
    for r in c.item_rows(Places { bank: true, mail: false, worn: false }) {
        let Some(id) = r.id.filter(|&i| i != 0.0) else { continue };
        if r.is_bound() {
            continue;
        }
        let Some(each) = realm.get(&id.to_string()).and_then(|e| e.now).filter(|&p| p != 0.0) else { continue };
        let h = held.entry(id as i64).or_insert(Holding { name: r.name.clone(), count: 0.0, each, places: Vec::new() });
        h.count += if r.count != 0.0 { r.count } else { 1.0 };
        let place = if matches!(r.place, crate::sources::addon::Place::Bank { .. }) { "bank" } else { "bags" };
        if !h.places.contains(&place) {
            h.places.push(place);
        }
    }
    let mut rows: Vec<(i64, Holding)> = held.into_iter().collect();
    let total = |h: &Holding| h.each * h.count;
    rows.sort_by(|a, b| total(&b.1).partial_cmp(&total(&a.1)).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = rows.iter().map(|(_, h)| total(h)).sum();
    let items: Vec<Value> = rows
        .iter()
        .map(|(id, h)| {
            Obj::new()
                .opt("name", h.name.clone())
                .set("id", *id)
                .set("count", number(h.count))
                .set("each", money_long(h.each))
                .set("total", money_long(total(h)))
                .set("where", h.places.join("+"))
                .build()
        })
        .collect();
    Ok(ctx
        .header(c)
        .set("realm", realm.realm.as_str())
        .opt("full_scan", ctx.when(db.scan_at))
        .set("total", money_long(sum))
        .set("items", items)
        .build())
}

/// What's worn in a slot, scored.
struct Worn {
    name: Option<String>,
    equip: Option<String>,
    score: f64,
}

/// A possible upgrade: its gain over what's worn, its price, and the row shown.
struct Candidate {
    gain: f64,
    price: f64,
    row: Value,
}

/// "Quest Name (21) / Other Quest (24)": the quests that reward the item.
fn quest_sources(it: &ItemRecord) -> Option<String> {
    let quests = it.quests.as_ref()?;
    let names: Vec<String> = quests
        .iter()
        .map(|q| {
            let name = q.name.clone().unwrap_or_else(|| q.id.map_or("a quest".into(), |id| format!("quest {id}")));
            q.level.map_or(name.clone(), |level| format!("{name} ({level})"))
        })
        .collect();
    Some(names.join(" / "))
}

/// "Boss (Dungeon) / ...": who drops the item, each once.
fn drop_sources(it: &ItemRecord) -> Option<String> {
    let show = |v: &Option<Value>| match v {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Null) | None => None,
        Some(v) => Some(v.to_string()),
    };
    let mut lines: Vec<String> = Vec::new();
    for d in it.drops.as_ref()? {
        let line = match (show(&d.npc), show(&d.dungeon)) {
            (Some(npc), Some(dungeon)) => format!("{npc} ({dungeon})"),
            (Some(npc), None) => npc,
            (None, Some(dungeon)) => dungeon,
            (None, None) => continue,
        };
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    Some(lines.join(" / "))
}

/// Gear upgrades from the auction scan (or from `/fbl items` with `source=items`): the best gain per slot.
pub fn upgrades(ctx: &Ctx, args: &Args) -> Result<Value, String> {
    let arg = |k: &str| args.get(k).map(String::as_str).filter(|s| !s.is_empty());
    let c = pick(ctx.data, arg("character"))?;
    let from_items = arg("source") == Some("items");
    let (items, scan_at, scan) = if from_items {
        if ctx.data.items.is_empty() {
            return Err("No item stats saved yet: /fbl items in game, then /fbl r.".into());
        }
        let at = ctx.data.items.values().map(|i| i.at.unwrap_or(0.0)).fold(0.0, f64::max);
        let items: Vec<&ItemRecord> = ctx.data.items.values().collect();
        (items, Some(at), None)
    } else {
        let ah = ctx.data.ah.as_ref();
        let Some(items) = ah.and_then(|a| a.items.as_ref()) else {
            let last = ah.and_then(|a| a.error.as_ref()).map_or(String::new(), |e| format!(" (Last error: {e})"));
            return Err(format!(
                "No auction house gear yet. Open the auction house, type /fbl ah, wait for its chat line, then /fbl r.{last}"
            ));
        };
        (items.iter().map(|(_, i)| i).collect(), ah.and_then(|a| a.at), ah)
    };
    let p = profile(c, arg("weights"));
    let level = c.identity.level.filter(|l| *l != 0.0).unwrap_or(60.0);
    let ahead = number_arg(args, "ahead", 0.0)?;
    let budget = number_arg(args, "budget", f64::INFINITY)? * 10000.0;
    let per_slot = number_arg(args, "per_slot", 3.0)?.max(0.0) as usize;
    let want_slot = arg("slot").map(str::to_lowercase);

    let mut worn: HashMap<usize, Worn> = HashMap::new();
    let mut without_tips = 0;
    for (slot, g) in &c.gear {
        if g.tip.is_none() {
            without_tips += 1;
        }
        let stats = parse_stats(g.tip.as_deref().unwrap_or(&[]));
        if let Ok(n) = slot.parse::<usize>() {
            worn.insert(n, Worn { name: parse_link(g.link.as_deref()).name, equip: g.equip.clone(), score: score(&stats, &p) });
        }
    }
    let score_in = |slot: usize| worn.get(&slot).map(|w| w.score);
    let two_hander = worn.get(&16).and_then(|w| w.equip.as_deref()) == Some("INVTYPE_2HWEAPON");

    // Candidates by slot name, slots in the order first found.
    let mut by_slot: Vec<(&str, Vec<Candidate>)> = Vec::new();
    let mut not_usable: Vec<(String, usize)> = Vec::new();
    let mut skip = |reason: String| match not_usable.iter_mut().find(|(r, _)| *r == reason) {
        Some((_, n)) => *n += 1,
        None => not_usable.push((reason, 1)),
    };
    for it in items {
        let stats = parse_stats(&it.tip);
        let Some(req) = required_level(it, &stats) else {
            skip("level unknown".into());
            continue;
        };
        if req > level + ahead || it.buyout.is_some_and(|b| b > budget) {
            continue;
        }
        if let Err(reason) = can_use(c, it, &stats) {
            skip(reason);
            continue;
        }
        let Some(slots) = it.slot.as_deref().and_then(slots_for) else { continue };
        let sc = score(&stats, &p);
        // A ring or trinket replaces the weaker of the two; a two-hander replaces main and off hand.
        let mut target = slots[0];
        if slots.len() == 2 && score_in(slots[0]).unwrap_or(-1.0) > score_in(slots[1]).unwrap_or(-1.0) {
            target = slots[1];
        }
        let inv = it.slot.as_deref().unwrap_or("");
        let current = if inv == "INVTYPE_2HWEAPON" {
            score_in(16).unwrap_or(0.0) + if two_hander { 0.0 } else { score_in(17).unwrap_or(0.0) }
        } else {
            score_in(target).unwrap_or(0.0)
        };
        // An off-hand would mean giving up the two-hander; that pair isn't scored.
        if two_hander && matches!(inv, "INVTYPE_WEAPONOFFHAND" | "INVTYPE_HOLDABLE" | "INVTYPE_SHIELD") {
            continue;
        }
        let gain = ((sc - current) * 10.0).round() / 10.0;
        if gain <= 0.0 {
            continue;
        }
        let slot_name = SLOT_NAMES[target];
        if want_slot.as_ref().is_some_and(|w| !slot_name.starts_with(w.as_str())) {
            continue;
        }
        let row = Obj::new()
            .opt("name", it.name.clone().filter(|n| !n.is_empty()).or_else(|| parse_link(it.link.as_deref()).name))
            .opt("id", it.id)
            .set("gain", gain)
            .set("score", sc)
            .set("req", req)
            .opt("price", it.buyout.map(money_long))
            .opt("listings", it.listings.clone())
            .set("stats", stats.summary())
            .set("replaces", worn.get(&target).and_then(|w| w.name.clone()).unwrap_or_else(|| "(empty)".into()))
            .opt("quests", quest_sources(it))
            .opt("drops", drop_sources(it))
            .build();
        let candidate = Candidate { gain, price: it.buyout.unwrap_or(0.0), row };
        match by_slot.iter_mut().find(|(s, _)| *s == slot_name) {
            Some((_, rows)) => rows.push(candidate),
            None => by_slot.push((slot_name, vec![candidate])),
        }
    }
    let mut out = serde_json::Map::new();
    for (slot, mut rows) in by_slot {
        // Biggest gain first; the cheaper of equal gains.
        rows.sort_by(|a, b| b.gain.total_cmp(&a.gain).then(a.price.total_cmp(&b.price)));
        out.insert(slot.to_string(), Value::Array(rows.into_iter().take(per_slot).map(|c| c.row).collect()));
    }
    let weights: serde_json::Map<String, Value> = p.weights.iter().map(|(k, v)| (k.clone(), number(*v))).collect();
    let stopped_early = scan.is_some_and(|s| s.complete == Some(false));
    Ok(ctx
        .header(c)
        .opt("scan", ctx.when(scan_at))
        .opt("scan_note", stopped_early.then_some("The last /fbl ah scan stopped early; some items may be missing."))
        .opt("scan_error", scan.and_then(|s| s.error.clone()))
        .set("role", p.role.name())
        .opt("tree", p.tree.clone())
        .set("weights", Value::Object(weights))
        .opt(
            "note",
            (without_tips > 0).then_some("Some worn items have no stats saved yet (older snapshot): /fbl r once so they compare fairly."),
        )
        .set("not_usable", Value::Object(not_usable.into_iter().map(|(k, n)| (k, json!(n))).collect()))
        .set("upgrades", Value::Object(out))
        .build())
}
