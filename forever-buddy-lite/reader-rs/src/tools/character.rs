//! Commands about one character (or all of them): identity, gear, items, quests, skills and so on.

use super::talents::talent_trees;
use super::{pick, Ctx, Obj};
use crate::format::{iso_from_seconds, money_long};
use crate::json::{self, number};
use crate::sources::addon::{Character, ItemRow, Place, Places};
use crate::sources::links::parse_link;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Copper as gold with `digits` decimals: 123456 -> 12.35.
fn gold(copper: Option<f64>, digits: i32) -> Option<Value> {
    let scale = 10f64.powi(digits);
    copper.map(|c| number((c / 10000.0 * scale).round() / scale))
}

pub fn characters(ctx: &Ctx) -> Value {
    let mut all: Vec<&Character> = ctx.data.characters.values().collect();
    all.sort_by(|a, b| b.saved_at.unwrap_or(0.0).partial_cmp(&a.saved_at.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
    Value::Array(
        all.iter()
            .map(|c| {
                let id = &c.identity;
                Obj::new()
                    .set("character", c.key.as_str())
                    .opt("class", id.class.clone())
                    .opt("race", id.race.clone())
                    .opt("level", id.level)
                    .opt("zone", id.zone.clone())
                    .opt("subzone", id.subzone.clone())
                    .opt("gold", gold(id.money, 2))
                    .opt("saved", ctx.when(c.saved_at))
                    .build()
            })
            .collect(),
    )
}

pub fn summary(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let id = &c.identity;
    let xp_pct = id.xp_max.filter(|m| *m != 0.0).map(|max| {
        let pct = (id.xp.unwrap_or(0.0) / max * 1000.0).round() / 10.0;
        number(pct)
    });
    let unspent = c.raw.get("talents").and_then(|t| t.get("unspent")).cloned();
    Ok(ctx
        .header(c)
        .spread(c.raw.get("identity"))
        .opt("gold", gold(id.money, 4))
        .opt("xp_pct", xp_pct)
        .opt("unspent_talents", unspent)
        .opt("errors", c.raw.get("errors").cloned())
        .build())
}

pub fn stats(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    Ok(ctx.header(c).spread(c.raw.get("stats")).build())
}

/// The link's id, name and quality as JSON fields.
fn link_fields(link: Option<&str>) -> Obj {
    let l = parse_link(link);
    Obj::new().opt("id", l.id).opt("name", l.name).opt("quality", l.quality)
}

pub fn gear(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let pieces = c.gear.iter().map(|(slot, g)| {
        let durability = match g.durability.as_slice() {
            [now, max, ..] => Some(format!("{now}/{max}")),
            _ => None,
        };
        let fields = link_fields(g.link.as_deref()).opt("id", g.id).opt("durability", durability);
        (crate::sources::addon::slot_name(slot), fields.build())
    });
    Ok(ctx.header(c).set("gear", Value::Object(pieces.collect())).build())
}

/// An item row as JSON; `with_link` keeps the full link.
fn row_json(r: &ItemRow, with_link: bool) -> Obj {
    let mut o = match &r.place {
        Place::Bags { bag, slot } => Obj::new().set("where", "bags").set("bag", number(*bag)).set("slot", number(*slot)),
        Place::Bank { bag, slot } => Obj::new().set("where", "bank").set("bag", number(*bag)).set("slot", number(*slot)),
        Place::Worn { slot } => Obj::new().set("where", "worn").set("slot", slot.as_str()),
        Place::Mail { letter, from } => Obj::new().set("where", "mail").set("letter", *letter).opt("from", from.clone()),
    };
    o = o.opt("id", r.id.map(number)).opt("name", r.name.clone()).opt("quality", r.quality).set("count", number(r.count));
    if matches!(r.place, Place::Bags { .. } | Place::Bank { .. }) {
        o = o.opt("bound", r.bound.clone());
    }
    if with_link {
        o = o.opt("link", r.link.clone());
    }
    o
}

/// Rows whose name contains the query (any case), or whose id is the query.
fn matches(query: Option<&str>) -> impl Fn(&ItemRow) -> bool + '_ {
    let q = query.map(str::to_lowercase);
    move |r| match &q {
        None => true,
        Some(q) => r.name.as_deref().unwrap_or("").to_lowercase().contains(q.as_str()) || r.id.is_some_and(|id| id.to_string() == *q),
    }
}

pub fn items(ctx: &Ctx, name: Option<&str>, query: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let rows: Vec<Value> = c.item_rows(Places::ALL).iter().filter(|r| matches(query)(r)).map(|r| row_json(r, false).build()).collect();
    let free: f64 = c.bags.iter().flat_map(|b| b.bags.iter()).map(|(_, bag)| bag.free.unwrap_or(0.0)).sum();
    Ok(ctx
        .header(c)
        .opt("bank_seen", ctx.when(c.bank.as_ref().and_then(|b| b.at)))
        .opt("mail_seen", ctx.when(c.mail.as_ref().and_then(|m| m.at)))
        .set("free_bag_slots", number(free))
        .set("items", rows)
        .build())
}

pub fn find(ctx: &Ctx, query: Option<&str>) -> Result<Value, String> {
    let query = query.ok_or("query is required")?;
    let mut out = Vec::new();
    for c in ctx.data.characters.values() {
        for r in c.item_rows(Places::ALL).iter().filter(|r| matches(Some(query))(r)) {
            let row = row_json(r, false).build();
            out.push(Obj::new().set("character", c.key.as_str()).spread(Some(&row)).build());
        }
    }
    Ok(Value::Array(out))
}

pub fn quests(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let log = &c.quests.log;
    // Reward items by quest id, from /fbl items. You pick one reward, so the best vendor price is
    // what the quest is worth in coin.
    let mut rewards: HashMap<u64, Vec<&crate::sources::addon::ItemRecord>> = HashMap::new();
    for item in ctx.data.items.values() {
        for id in item.quests.iter().flatten().filter_map(|q| q.id) {
            rewards.entry(id as u64).or_default().push(item);
        }
    }
    let rewards_of = |q: &crate::sources::addon::QuestEntry| q.id.and_then(|id| rewards.get(&(id as u64)));
    let best_sell =
        |q: &crate::sources::addon::QuestEntry| rewards_of(q).into_iter().flatten().map(|it| it.sell.unwrap_or(0.0)).fold(0.0, f64::max);
    let done: Vec<_> = log.iter().filter(|q| q.is_done()).collect();
    let sum = |qs: &mut dyn Iterator<Item = f64>| qs.sum::<f64>();
    let totals = log.iter().any(|q| q.money.is_some()).then(|| {
        json!({
            "gold_all": money_long(sum(&mut log.iter().map(|q| q.money.unwrap_or(0.0)))),
            "gold_ready": money_long(sum(&mut done.iter().map(|q| q.money.unwrap_or(0.0)))),
            "vendor_all": money_long(sum(&mut log.iter().map(best_sell))),
            "vendor_ready": money_long(sum(&mut done.iter().map(|q| best_sell(q)))),
            "xp_all": number(sum(&mut log.iter().map(|q| q.xp.unwrap_or(0.0)))),
            "xp_ready": number(sum(&mut done.iter().map(|q| q.xp.unwrap_or(0.0)))),
        })
    });
    let entries: Vec<Value> = log
        .iter()
        .map(|q| {
            let reward_items = rewards_of(q).map(|items| {
                items
                    .iter()
                    .map(|it| {
                        let name = it.name.clone().unwrap_or_else(|| it.id.map_or("an item".into(), |id| format!("item {id}")));
                        match it.sell.filter(|s| *s != 0.0) {
                            Some(s) => format!("{name} (vendor {})", money_long(s)),
                            None => name,
                        }
                    })
                    .collect::<Vec<_>>()
            });
            let objectives: Vec<String> = q
                .objectives
                .iter()
                .map(|o| format!("{} {}", if o.done.0 { "[x]" } else { "[ ]" }, o.text.as_deref().unwrap_or("")).trim().to_string())
                .collect();
            Obj::new()
                .opt("id", q.id)
                .opt("title", q.title.clone())
                .opt("level", q.level)
                .opt("group", q.suggested_group.clone())
                .set("complete", q.is_done())
                .opt("failed", q.failed.clone())
                .opt("gold", q.money.filter(|m| *m != 0.0).map(money_long))
                .opt("xp", q.xp.filter(|x| *x != 0.0 && !x.is_nan()))
                .opt("reward_items", reward_items)
                .set("objectives", objectives)
                .build()
        })
        .collect();
    Ok(ctx.header(c).set("completed_count", c.quests.completed.len()).opt("rewards", totals).set("log", entries).build())
}

pub fn completed(ctx: &Ctx, name: Option<&str>, ids: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let done = &c.quests.completed;
    let mut asked: Vec<f64> = Vec::new();
    for word in ids.unwrap_or("").split(|ch: char| ch.is_whitespace() || ch == ',').filter(|s| !s.is_empty()) {
        asked.push(super::upgrades::parse_number(word).ok_or_else(|| format!("\"{word}\" isn't a quest id"))?);
    }
    let h = ctx.header(c);
    if asked.is_empty() {
        let mut unique: Vec<f64> = Vec::new();
        for q in done {
            if !unique.contains(q) {
                unique.push(*q);
            }
        }
        return Ok(h.set("completed", unique.into_iter().map(number).collect::<Vec<_>>()).build());
    }
    let answers = asked.iter().map(|q| (q.to_string(), Value::Bool(done.contains(q)))).collect();
    Ok(h.set("quests", Value::Object(answers)).build())
}

pub fn skills(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let lines: Vec<String> = c
        .skills
        .iter()
        .filter_map(|s| {
            let name = s.name.as_deref()?;
            if s.collapsed.0 {
                return Some(format!("{name}: (collapsed in the skill window, not read)"));
            }
            let rank = s.rank.map_or(String::new(), |r| format!(" {r}"));
            let bonus = s.bonus.filter(|b| *b != 0.0).map_or(String::new(), |b| format!("(+{b})"));
            let max = s.max.filter(|m| *m != 0.0).map_or(String::new(), |m| format!("/{m}"));
            Some(format!("{name}{rank}{bonus}{max}"))
        })
        .collect();
    Ok(ctx.header(c).set("skills", lines).build())
}

pub fn talents(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let summary = talent_trees(c).map(|t| serde_json::to_value(t).unwrap());
    Ok(ctx.header(c).spread(summary.as_ref()).build())
}

pub fn spells(ctx: &Ctx, name: Option<&str>, query: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let q = query.unwrap_or("").to_lowercase();
    let list: Vec<Value> = c
        .spells
        .iter()
        .filter(|s| q.is_empty() || s.name.to_lowercase().contains(&q))
        .map(|s| {
            Obj::new()
                .set("name", s.name.as_str())
                .opt("rank", s.rank.clone())
                .opt("id", s.id.clone())
                .opt("tab", s.tab.clone())
                .opt("passive", s.passive.clone())
                .build()
        })
        .collect();
    Ok(ctx.header(c).set("spells", list).build())
}

pub fn reputation(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let lines: Vec<String> = c
        .reputation
        .iter()
        .filter_map(|f| {
            let name = f.name.as_deref()?;
            let standing = f.standing.as_deref().map_or(String::new(), |s| format!(": {s}"));
            let progress = match (f.value, f.max.filter(|m| *m != 0.0)) {
                (Some(value), Some(max)) => format!(" {value}/{max}"),
                _ => String::new(),
            };
            Some(format!("{name}{standing}{progress}"))
        })
        .collect();
    Ok(ctx.header(c).set("factions", lines).build())
}

pub fn lockouts(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let list: Vec<Value> = json::list(c.raw.get("lockouts"))
        .into_iter()
        .map(|l| {
            let resets = l.get("resets_at").filter(|v| json::truthy(v)).and_then(Value::as_f64).map(iso_from_seconds);
            Obj::new().spread(Some(l)).opt("resets_at", resets).build()
        })
        .collect();
    Ok(ctx.header(c).set("lockouts", list).build())
}

pub fn buffs(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let list: Vec<Value> = json::list(c.raw.get("buffs")).into_iter().cloned().collect();
    Ok(ctx.header(c).set("buffs", list).build())
}

pub fn mail(ctx: &Ctx, name: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let letters: Vec<Value> = c
        .mail
        .iter()
        .flat_map(|m| m.letters.iter())
        .map(|l| {
            let items: Vec<Value> = l.items.iter().map(|it| link_fields(it.link.as_deref()).opt("count", it.count).build()).collect();
            Obj::new().spread(Some(&l.raw)).set("items", items).build()
        })
        .collect();
    Ok(ctx.header(c).opt("seen", ctx.when(c.mail.as_ref().and_then(|m| m.at))).set("letters", letters).build())
}

pub fn recipes(ctx: &Ctx, name: Option<&str>, profession: Option<&str>, query: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    let wanted = profession.map(str::to_lowercase);
    let profs: Vec<_> =
        c.professions.iter().filter(|(n, _)| wanted.as_ref().is_none_or(|w| n.to_lowercase().contains(w.as_str()))).collect();
    if profs.is_empty() {
        return Err("No profession window saved yet: open it in game, then /fbl r.".into());
    }
    let q = query.unwrap_or("").to_lowercase();
    let out = profs.iter().map(|(n, p)| {
        let recipes: Vec<Value> = p
            .recipes
            .iter()
            .filter(|r| q.is_empty() || r.get("name").and_then(Value::as_str).unwrap_or("").to_lowercase().contains(&q))
            .map(|r| {
                let as_list = |k: &str| Value::Array(json::list(r.get(k)).into_iter().cloned().collect());
                Obj::new().spread(Some(r)).set("reagents", as_list("reagents")).set("tools", as_list("tools")).build()
            })
            .collect();
        let entry = Obj::new().opt("seen", ctx.when(p.at)).opt("rank", p.rank.clone()).opt("max", p.max.clone()).set("recipes", recipes);
        (n.clone(), entry.build())
    });
    Ok(ctx.header(c).set("professions", Value::Object(out.collect())).build())
}

pub fn trainers(ctx: &Ctx, query: Option<&str>) -> Result<Value, String> {
    let q = query.unwrap_or("").to_lowercase();
    let text = |v: Option<&Value>| match v {
        Some(Value::String(s)) => s.to_lowercase(),
        Some(v) if json::truthy(v) => v.to_string().to_lowercase(),
        _ => String::new(),
    };
    let mut out = Vec::new();
    for (npc, t) in ctx.data.trainers.iter() {
        let services: Vec<Value> = t
            .services
            .iter()
            .filter(|s| {
                q.is_empty() || npc.to_lowercase().contains(&q) || text(s.get("name")).contains(&q) || text(s.get("skill")).contains(&q)
            })
            .map(|s| {
                let cost = s.get("cost").and_then(Value::as_f64).map(money_long);
                Obj::new().spread(Some(s)).opt("cost", cost).build()
            })
            .collect();
        if services.is_empty() {
            continue;
        }
        out.push(
            Obj::new().set("trainer", npc.as_str()).opt("seen", ctx.when(t.at)).opt("by", t.by.clone()).set("services", services).build(),
        );
    }
    if out.is_empty() {
        return Err("No trainer saved yet: talk to one in game, then /fbl r.".into());
    }
    Ok(Value::Array(out))
}

pub fn raw(ctx: &Ctx, name: Option<&str>, section: Option<&str>) -> Result<Value, String> {
    let c = pick(ctx.data, name)?;
    Ok(match section {
        Some(s) => ctx.header(c).opt(s, c.raw.get(s).cloned()).build(),
        None => c.raw.clone(),
    })
}
