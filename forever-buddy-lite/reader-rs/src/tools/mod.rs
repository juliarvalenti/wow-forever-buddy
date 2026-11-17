//! The character commands (`characters`, `gear`, `quests`, `upgrades`, `prices`...), the same
//! on the command line and over MCP. Each returns JSON.

mod character;
pub mod prices;
pub mod talents;
pub mod upgrades;

use crate::format;
use crate::sources::addon::{self, AddonData, Character};
use crate::sources::Env;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// A tool's name, what it does, and its arguments (name, description; "optional" in the description
/// marks an optional one, which is how the MCP schema is built).
pub struct ToolInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub args: &'static [(&'static str, &'static str)],
}

pub const TOOLS: &[ToolInfo] = &[
    ToolInfo { name: "characters", description: "Every character ForeverBuddy Lite has seen: class, level, zone, gold and how fresh the data is. Start here.", args: &[] },
    ToolInfo { name: "summary", description: "One character at a glance: identity, XP, money, position (map coords), bind location, item level, unspent talent points.", args: &[("character", "Name (optional: the most recently saved)")] },
    ToolInfo { name: "stats", description: "Health, mana, Str/Agi/Sta/Int/Spi, spell power and crit by school, healing, melee/ranged crit, hit, attack power, armor, mana regen while casting.", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "gear", description: "Worn gear by slot: item name, id, quality, durability and the full link.", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "items", description: "Items one character has: bags, bank (as of the last bank visit), mailbox (last visit) and worn. Optional name or id filter.", args: &[("character", "Name (optional)"), ("query", "Item name part or id (optional)")] },
    ToolInfo { name: "find", description: "Where an item is across every character (bags, bank, mail, worn), with counts.", args: &[("query", "Item name part or id")] },
    ToolInfo { name: "quests", description: "The quest log with objectives and progress, plus how many quests are completed overall, reward gold and XP, and what each quest's reward items sell to a vendor for (when /fbl items saw them).", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "completed", description: "Whether specific quest ids are completed (turned in) on a character. Without ids: the full sorted list.", args: &[("character", "Name (optional)"), ("ids", "Quest ids, comma or space separated (optional)")] },
    ToolInfo { name: "skills", description: "Skill lines: weapon skills, Lockpicking, professions (with max), languages, armor.", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "talents", description: "Talent points by tree with every talent taken (rank/max), unspent points, and legacy talents.", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "spells", description: "Spells known (spellbook), highest rank first per name. Optional name filter.", args: &[("character", "Name (optional)"), ("query", "Spell name part (optional)")] },
    ToolInfo { name: "reputation", description: "Reputation with each faction: standing and progress into it.", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "lockouts", description: "Saved dungeon and raid lockouts with reset times.", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "buffs", description: "Buffs on the character when it was saved, with seconds left.", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "mail", description: "The mailbox as of the last visit: sender, subject, money, attachments, days left.", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "prices", description: "Auction house prices from Auctionator's scans: the last lowest buyout, the usual price (the median of each day's floor over 14 days, today left out) and the daily lows and highs. Query by item id, or by name for items any character has carried.", args: &[("query", "Item id or name part")] },
    ToolInfo { name: "flips", description: "Auction house bargains from Auctionator's daily scans: items whose cheapest listing now is well under their usual price, that are listed often enough to resell. Needs a few days of full scans to mean anything.", args: &[("budget", "Max buy price in gold (optional)"), ("ratio", "Now at most this share of the usual price (optional, default 0.7)"), ("min_days", "Days of history an item needs (optional, default 3)"), ("min_profit", "Least profit per item in gold after the 5% cut (optional, default 0.2)"), ("limit", "How many (optional, default 25)")] },
    ToolInfo { name: "value", description: "What a character's bags (and bank, if seen) would fetch on the auction house, priced from Auctionator, most valuable first. Soulbound items are left out.", args: &[("character", "Name (optional)")] },
    ToolInfo { name: "upgrades", description: "Auction house upgrades for a character, from the gear ForeverBuddy Lite kept during the last full scan. Any class: it checks armor and weapon skills, class restrictions and level, scores stats with the class's (or the top talent tree's) leveling weights, and compares with what's worn in that slot. Best gain per slot.", args: &[("character", "Name (optional)"), ("slot", "Only this slot, e.g. head, finger, main_hand (optional)"), ("budget", "Max price in gold (optional)"), ("ahead", "Also show items up to this many levels above you (optional, default 0)"), ("weights", "Override weights, e.g. int=1,sta=1,sp=0.8 (optional)"), ("per_slot", "How many per slot (optional, default 3)"), ("source", "ah (default) or items: the stats /fbl items saved for the ids in Wishlist.lua, e.g. quest rewards (optional)")] },
    ToolInfo { name: "recipes", description: "Recipes a character knows in a profession, as of the last time its window was open: colour then, reagents, tools (an anvil shows as a tool).", args: &[("character", "Name (optional)"), ("profession", "Profession name (optional: all)"), ("query", "Recipe name part (optional)")] },
    ToolInfo { name: "trainers", description: "Trainers visited: what each teaches, the skill and level it needs, cost, and whether the character who visited knows it.", args: &[("query", "Trainer, skill or recipe name part (optional)")] },
    ToolInfo { name: "raw", description: "One character's saved data, untouched, or one section of it (identity, stats, gear, bags, bank, mail, quests, skills, talents, spells, reputation, lockouts, buffs).", args: &[("character", "Name (optional)"), ("section", "Section name (optional)")] },
];

/// Tool arguments by name (all text, as on the command line).
pub type Args = HashMap<String, String>;

/// A number argument: its default when not given, an error when it isn't a number.
pub fn number_arg(args: &Args, name: &str, default: f64) -> Result<f64, String> {
    match args.get(name).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(default),
        Some(text) => upgrades::parse_number(text).ok_or_else(|| format!("{name} wants a number, not \"{text}\"")),
    }
}

/// What every command works from.
pub struct Ctx<'a> {
    pub data: &'a AddonData,
    pub env: &'a Env,
    /// Seconds since 1970, for ages ("5 min ago").
    pub now: f64,
}

impl Ctx<'_> {
    fn when(&self, sec: Option<f64>) -> Option<Value> {
        format::when(sec, self.now)
    }

    /// The fields every per-character result starts with: who, and how fresh.
    fn header(&self, c: &Character) -> Obj {
        Obj::new().set("character", c.key.as_str()).opt("saved", self.when(c.saved_at)).set("file_written", c.written.as_str())
    }
}

/// Runs a tool against the files `env` points at.
pub fn call(tool: &str, args: &Args, env: &Env) -> Result<Value, String> {
    call_at(tool, args, env, format::now_seconds())
}

/// `call` at a given time.
pub fn call_at(tool: &str, args: &Args, env: &Env, now: f64) -> Result<Value, String> {
    if !TOOLS.iter().any(|t| t.name == tool) {
        let names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        return Err(format!("Unknown command \"{tool}\". Commands: {}", names.join(", ")));
    }
    let data = addon::load(env)?;
    let ctx = Ctx { data: &data, env, now };
    let a = |name: &str| args.get(name).map(String::as_str).filter(|s| !s.is_empty());
    let result = match tool {
        "characters" => character::characters(&ctx),
        "summary" => character::summary(&ctx, a("character"))?,
        "stats" => character::stats(&ctx, a("character"))?,
        "gear" => character::gear(&ctx, a("character"))?,
        "items" => character::items(&ctx, a("character"), a("query"))?,
        "find" => character::find(&ctx, a("query"))?,
        "quests" => character::quests(&ctx, a("character"))?,
        "completed" => character::completed(&ctx, a("character"), a("ids"))?,
        "skills" => character::skills(&ctx, a("character"))?,
        "talents" => character::talents(&ctx, a("character"))?,
        "spells" => character::spells(&ctx, a("character"), a("query"))?,
        "reputation" => character::reputation(&ctx, a("character"))?,
        "lockouts" => character::lockouts(&ctx, a("character"))?,
        "buffs" => character::buffs(&ctx, a("character"))?,
        "mail" => character::mail(&ctx, a("character"))?,
        "recipes" => character::recipes(&ctx, a("character"), a("profession"), a("query"))?,
        "trainers" => character::trainers(&ctx, a("query"))?,
        "raw" => character::raw(&ctx, a("character"), a("section"))?,
        "prices" => prices::prices(&ctx, a("query"))?,
        "flips" => prices::flips(&ctx, args)?,
        "value" => prices::value(&ctx, a("character"))?,
        "upgrades" => prices::upgrades(&ctx, args)?,
        _ => unreachable!("checked above"),
    };
    Ok(crate::json::tidy_numbers(result))
}

/// Finds a character: by full key, by name (with or without the realm), or by a unique key prefix.
/// Without a name: the most recently saved.
pub fn pick<'a>(data: &'a AddonData, name: Option<&str>) -> Result<&'a Character, String> {
    let all: Vec<&Character> = data.characters.values().collect();
    if all.is_empty() {
        return Err("No ForeverBuddy Lite data yet. Install the addon, log in, and type /fbl r in game.".into());
    }
    let Some(name) = name else {
        // The newest; on a tie, the first seen.
        let newest = all.iter().copied().fold(None::<&Character>, |best, c| match best {
            Some(b) if c.saved_at.unwrap_or(0.0) <= b.saved_at.unwrap_or(0.0) => Some(b),
            _ => Some(c),
        });
        return Ok(newest.unwrap());
    };
    let n = name.to_lowercase();
    let exact: Vec<&Character> = all
        .iter()
        .copied()
        .filter(|c| {
            let key = c.key.to_lowercase();
            key == n || key.starts_with(&format!("{n}-")) || c.identity.name.as_deref().unwrap_or("").to_lowercase() == n
        })
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    let loose: Vec<&Character> = all.iter().copied().filter(|c| c.key.to_lowercase().starts_with(&n)).collect();
    if loose.len() == 1 {
        return Ok(loose[0]);
    }
    let kind = if exact.is_empty() && loose.is_empty() { "Unknown" } else { "Ambiguous" };
    let known: Vec<&str> = all.iter().map(|c| c.key.as_str()).collect();
    Err(format!("{kind} character \"{name}\". Known: {}", known.join(", ")))
}

/// A JSON object built field by field; `opt` leaves a field out when there is no value.
#[derive(Debug, Default, Clone)]
pub struct Obj(Map<String, Value>);

impl Obj {
    pub fn new() -> Obj {
        Obj(Map::new())
    }

    pub fn set(mut self, key: &str, value: impl Into<Value>) -> Obj {
        self.0.insert(key.to_string(), value.into());
        self
    }

    pub fn opt<V: Into<Value>>(mut self, key: &str, value: Option<V>) -> Obj {
        match value {
            Some(v) => {
                self.0.insert(key.to_string(), v.into());
            }
            None => {
                self.0.shift_remove(key);
            }
        }
        self
    }

    /// Copies in every field of `other` when it is an object.
    pub fn spread(mut self, other: Option<&Value>) -> Obj {
        if let Some(Value::Object(o)) = other {
            for (k, v) in o {
                self.0.insert(k.clone(), v.clone());
            }
        }
        self
    }

    pub fn build(self) -> Value {
        Value::Object(self.0)
    }
}
