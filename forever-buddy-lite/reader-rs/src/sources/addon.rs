//! ForeverBuddyLite.lua: what the addon saved about each character, its auction house gear scan,
//! item details (`/fbl items`), trainers and price ladders.
//!
//! Several accounts can each have a file; they merge: for each character, trainer and ladder the
//! newest copy wins, and the auction scan comes from whichever file scanned last.

use super::lists::{lua_keyed, lua_list, lua_list_opt, or_default, values, OrderedMap, Truthy};
use super::{links, Env};
use crate::{cbor, format, json, lua};
use base64::Engine;
use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Everything read from the addon's files.
#[derive(Debug, Default)]
pub struct AddonData {
    pub files: Vec<PathBuf>,
    /// By key ("Name-Realm"), in the order first seen.
    pub characters: OrderedMap<Character>,
    /// Other players' gear saved with /fbl inspect (a friend you group with), by "Name-Realm": only
    /// identity, gear, a class's armor and weapon skills, and the talent tree you named. Tools that
    /// take a character name find these too; lists of your characters leave them out.
    pub inspected: OrderedMap<Character>,
    /// The newest gear scan of the auction house (`/fbl ah`).
    pub ah: Option<AhScan>,
    /// Trainers by NPC name.
    pub trainers: OrderedMap<Trainer>,
    /// Item details by item key, from `/fbl items` (quest rewards, wishlist items).
    pub items: OrderedMap<ItemRecord>,
    /// Price ladders by item key ("id" or "id:suffix"), oldest search first.
    pub ladders: OrderedMap<Vec<LadderSnapshot>>,
    /// Auction house buys and sales from mail invoices, oldest first (every file's, together).
    pub journal: Vec<JournalEntry>,
    /// Battlegrounds played, oldest first (see the addon's Enemies.lua, match history).
    pub bg_matches: Vec<BgMatch>,
}

/// One battleground match: your own scoreboard row and each side's totals (no other player's row).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct BgMatch {
    /// When it ended, and when you entered (unix seconds); its length in seconds.
    pub at: f64,
    pub started: Option<f64>,
    pub duration: Option<f64>,
    pub map: String,
    /// "Name-Realm" of the character who played it.
    pub char: Option<String>,
    /// "win", "loss", "draw" or "unknown".
    pub result: String,
    pub me: Option<BgRow>,
    pub mine: Option<BgSide>,
    pub theirs: Option<BgSide>,
}

/// Your row. A value the client hid is missing.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct BgRow {
    pub kb: Option<f64>,
    pub hk: Option<f64>,
    pub deaths: Option<f64>,
    pub honor: Option<f64>,
    pub damage: Option<f64>,
    pub healing: Option<f64>,
    /// The battleground's own columns: flags captured, bases assaulted...
    #[serde(deserialize_with = "crate::sources::lists::lua_list")]
    pub stats: Vec<BgStat>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct BgStat {
    pub name: String,
    pub value: f64,
}

/// One side's totals.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct BgSide {
    pub players: f64,
    pub kb: f64,
    pub hk: f64,
    pub deaths: f64,
    pub damage: f64,
    pub healing: f64,
}

/// One auction house buy or sale, from the invoice on its mail (see the addon's Journal.lua).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct JournalEntry {
    /// When the mail was sent (unix seconds).
    pub t: f64,
    /// "buy" or "sell".
    pub kind: String,
    pub item: String,
    pub count: f64,
    /// What was paid, or what it sold for before the cut (copper, for the whole count).
    pub price: f64,
    /// A sale's deposit, returned with the money.
    pub deposit: f64,
    /// The auction house's cut of a sale.
    pub cut: f64,
    /// Gold on the mail.
    pub money: f64,
    /// Which character's mailbox it was in.
    pub char: Option<String>,
}

/// One character as of its last logout or /reload.
#[derive(Debug, Clone, Default)]
pub struct Character {
    /// "Name-Realm".
    pub key: String,
    /// The file it came from, and when the game wrote that file (ISO 8601).
    pub file: PathBuf,
    pub written: String,
    pub saved_at: Option<f64>,
    pub identity: Identity,
    /// Worn items by inventory slot number (as a string key: "16" is the main hand).
    pub gear: Vec<(String, GearItem)>,
    pub bags: Option<Container>,
    /// As of the last bank visit.
    pub bank: Option<Container>,
    pub quests: Quests,
    pub skills: Vec<Skill>,
    pub talents: Option<Talents>,
    pub spells: Vec<Spell>,
    pub reputation: Vec<Faction>,
    pub mail: Option<Mailbox>,
    pub professions: Vec<(String, Profession)>,
    /// The character's table as saved, for commands that show sections untouched (stats, buffs,
    /// lockouts, raw). Includes `_key`, `_file` and `_written` (where it came from).
    pub raw: Value,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Identity {
    pub name: Option<String>,
    pub realm: Option<String>,
    pub class: Option<String>,
    pub race: Option<String>,
    pub level: Option<f64>,
    /// Copper.
    pub money: Option<f64>,
    pub zone: Option<String>,
    /// "Alliance" or "Horde".
    pub faction: Option<String>,
    pub subzone: Option<String>,
    pub xp: Option<f64>,
    pub xp_max: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GearItem {
    pub link: Option<String>,
    pub id: Option<f64>,
    /// The inventory type, e.g. "INVTYPE_2HWEAPON".
    pub equip: Option<String>,
    /// Tooltip lines, when the snapshot kept them (stats are read from these).
    #[serde(deserialize_with = "lua_list_opt")]
    pub tip: Option<Vec<String>>,
    #[serde(deserialize_with = "lua_list")]
    pub durability: Vec<f64>,
}

/// Bags or bank: containers by bag number (0-4 bags, -1 the bank's main slots, 5+ bank bags).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Container {
    pub at: Option<f64>,
    #[serde(deserialize_with = "lua_keyed")]
    pub bags: Vec<(String, Bag)>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Bag {
    pub free: Option<f64>,
    /// Items by slot number.
    #[serde(deserialize_with = "lua_keyed")]
    pub items: Vec<(String, BagItem)>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BagItem {
    pub link: Option<String>,
    pub name: Option<String>,
    pub id: Option<f64>,
    pub count: Option<f64>,
    /// Soulbound, as saved (shown as is; any truthy value counts).
    pub bound: Option<Value>,
}

/// Where an item is.
#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    Bags {
        bag: f64,
        slot: f64,
    },
    Bank {
        bag: f64,
        slot: f64,
    },
    /// Worn, in a named slot ("main_hand") or the raw slot key when unknown.
    Worn {
        slot: String,
    },
    /// Attached to the `letter`th letter (1-based) in the mailbox.
    Mail {
        letter: usize,
        from: Option<String>,
    },
}

/// One stack a character has, wherever it is.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemRow {
    pub place: Place,
    pub id: Option<f64>,
    pub name: Option<String>,
    pub quality: Option<&'static str>,
    pub count: f64,
    pub bound: Option<Value>,
    pub link: Option<String>,
}

impl ItemRow {
    pub fn is_bound(&self) -> bool {
        self.bound.as_ref().is_some_and(json::truthy)
    }

    pub fn is_worn(&self) -> bool {
        matches!(self.place, Place::Worn { .. })
    }
}

/// Which places `Character::item_rows` covers (bags always).
#[derive(Debug, Clone, Copy)]
pub struct Places {
    pub bank: bool,
    pub mail: bool,
    pub worn: bool,
}

impl Places {
    pub const ALL: Places = Places { bank: true, mail: true, worn: true };
}

/// Inventory slot names by slot number.
pub const SLOT_NAMES: [&str; 20] = [
    "ammo",
    "head",
    "neck",
    "shoulder",
    "shirt",
    "chest",
    "waist",
    "legs",
    "feet",
    "wrist",
    "hands",
    "finger1",
    "finger2",
    "trinket1",
    "trinket2",
    "back",
    "main_hand",
    "off_hand",
    "ranged",
    "tabard",
];

/// A slot key ("16") as its name ("main_hand"); unknown keys stay as they are.
pub fn slot_name(key: &str) -> String {
    key.parse::<usize>().ok().and_then(|n| SLOT_NAMES.get(n)).map_or_else(|| key.to_string(), |s| s.to_string())
}

impl Character {
    /// Everything the character has: bags, then bank (as of the last visit), worn, then mail.
    pub fn item_rows(&self, places: Places) -> Vec<ItemRow> {
        let mut rows = Vec::new();
        let containers = [(self.bags.as_ref(), false), (self.bank.as_ref().filter(|_| places.bank), true)];
        for (container, is_bank) in containers {
            let Some(container) = container else { continue };
            for (bag_key, bag) in &container.bags {
                for (slot_key, item) in &bag.items {
                    let link = links::parse_link(item.link.as_deref());
                    let (bag, slot) = (number_key(bag_key), number_key(slot_key));
                    rows.push(ItemRow {
                        place: if is_bank { Place::Bank { bag, slot } } else { Place::Bags { bag, slot } },
                        id: item.id.or(link.id.map(f64::from)),
                        name: link.name.or_else(|| item.name.clone()),
                        quality: link.quality,
                        count: item.count.unwrap_or(1.0),
                        bound: item.bound.clone(),
                        link: item.link.clone(),
                    });
                }
            }
        }
        if places.worn {
            for (slot_key, g) in &self.gear {
                let link = links::parse_link(g.link.as_deref());
                rows.push(ItemRow {
                    place: Place::Worn { slot: slot_name(slot_key) },
                    id: link.id.map(f64::from),
                    name: link.name,
                    quality: link.quality,
                    count: 1.0,
                    bound: None,
                    link: g.link.clone(),
                });
            }
        }
        if places.mail {
            for (n, letter) in self.mail.iter().flat_map(|m| m.letters.iter()).enumerate() {
                for item in &letter.items {
                    let link = links::parse_link(item.link.as_deref());
                    rows.push(ItemRow {
                        place: Place::Mail { letter: n + 1, from: letter.sender.clone() },
                        id: link.id.map(f64::from),
                        name: link.name,
                        quality: link.quality,
                        count: item.count.unwrap_or(1.0),
                        bound: None,
                        link: item.link.clone(),
                    });
                }
            }
        }
        rows
    }
}

fn number_key(key: &str) -> f64 {
    key.parse().unwrap_or(f64::NAN)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Quests {
    #[serde(deserialize_with = "lua_list")]
    pub log: Vec<QuestEntry>,
    /// Ids of quests turned in.
    #[serde(deserialize_with = "lua_list")]
    pub completed: Vec<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct QuestEntry {
    pub id: Option<f64>,
    pub title: Option<String>,
    pub level: Option<f64>,
    pub suggested_group: Option<Value>,
    pub complete: Truthy,
    pub ready: Truthy,
    pub failed: Option<Value>,
    /// Reward copper, when the addon could read it.
    pub money: Option<f64>,
    pub xp: Option<f64>,
    #[serde(deserialize_with = "lua_list")]
    pub objectives: Vec<Objective>,
}

impl QuestEntry {
    /// Ready to turn in.
    pub fn is_done(&self) -> bool {
        self.complete.0 || self.ready.0
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Objective {
    pub text: Option<String>,
    pub done: Truthy,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Skill {
    pub name: Option<String>,
    pub rank: Option<f64>,
    pub bonus: Option<f64>,
    pub max: Option<f64>,
    /// A collapsed header in the skill window: its lines weren't read.
    pub collapsed: Truthy,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Talents {
    /// "traits" for the newer talent UI (nodes with x/y positions); otherwise classic tabs.
    pub system: Option<String>,
    pub unspent: Option<Value>,
    #[serde(deserialize_with = "lua_list")]
    pub tabs: Vec<TalentTab>,
    #[serde(deserialize_with = "lua_list")]
    pub nodes: Vec<TalentNode>,
    #[serde(deserialize_with = "lua_list")]
    pub legacy: Vec<TalentNode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TalentTab {
    pub name: Option<String>,
    pub points: Option<f64>,
    #[serde(deserialize_with = "lua_list")]
    pub talents: Vec<TalentNode>,
}

/// A talent: in a classic tab, or a node of the traits UI (with its position).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TalentNode {
    pub name: Option<String>,
    pub spell: Option<Value>,
    pub rank: Option<f64>,
    pub max: Option<f64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Spell {
    pub name: String,
    pub rank: Option<Value>,
    pub id: Option<Value>,
    pub tab: Option<Value>,
    pub passive: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Faction {
    pub name: Option<String>,
    pub standing: Option<String>,
    pub value: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct Mailbox {
    pub at: Option<f64>,
    pub letters: Vec<Letter>,
}

#[derive(Debug, Clone, Default)]
pub struct Letter {
    pub sender: Option<String>,
    pub items: Vec<MailItem>,
    /// The letter as saved (subject, money, days left...), shown as is.
    pub raw: Value,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MailItem {
    pub link: Option<String>,
    pub count: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Profession {
    pub at: Option<f64>,
    pub rank: Option<Value>,
    pub max: Option<Value>,
    /// Recipes as saved (name, colour, reagents, tools), shown as is.
    #[serde(deserialize_with = "lua_list")]
    pub recipes: Vec<Value>,
}

/// An item with its tooltip: from the auction house gear scan or from `/fbl items`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ItemRecord {
    pub id: Option<f64>,
    pub name: Option<String>,
    pub link: Option<String>,
    /// The inventory type ("INVTYPE_HEAD").
    pub slot: Option<String>,
    /// Item class: 2 weapon, 4 armor.
    pub class: Option<f64>,
    /// "Cloth", "Staves", "Miscellaneous"...
    pub subtype: Option<String>,
    /// The subclass number (for trade goods: 5 cloth, 9 herbs...).
    pub subclass: Option<f64>,
    /// Cheapest buyout in copper (auction scan).
    pub buyout: Option<f64>,
    pub listings: Option<Value>,
    #[serde(deserialize_with = "lua_list")]
    pub tip: Vec<String>,
    /// Required level as the game reports it (0 when none).
    pub req: Option<f64>,
    pub ilvl: Option<f64>,
    /// Quests that reward it; present (even empty) means it's a quest reward.
    #[serde(deserialize_with = "lua_list_opt")]
    pub quests: Option<Vec<QuestRef>>,
    #[serde(deserialize_with = "lua_list_opt")]
    pub drops: Option<Vec<DropSource>>,
    /// Vendor sell price in copper.
    pub sell: Option<f64>,
    pub at: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct QuestRef {
    pub id: Option<f64>,
    pub level: Option<f64>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DropSource {
    pub npc: Option<Value>,
    pub dungeon: Option<Value>,
}

/// The addon's auction house gear scan.
#[derive(Debug, Clone, Default)]
pub struct AhScan {
    pub at: Option<f64>,
    /// false when the scan stopped early.
    pub complete: Option<bool>,
    /// The last scan error the addon logged, or why the packed scan couldn't be read.
    pub error: Option<String>,
    /// Items by key ("id:suffix"); None when the scan couldn't be read.
    pub items: Option<Vec<(String, ItemRecord)>>,
}

#[derive(Debug, Clone, Default)]
pub struct Trainer {
    pub at: Option<f64>,
    /// Who visited.
    pub by: Option<Value>,
    /// What it teaches, as saved (name, skill, cost, status...).
    pub services: Vec<Value>,
}

/// One search of an item at the auction house: every price level and its quantity.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LadderSnapshot {
    pub at: Option<f64>,
    /// Listings in total.
    pub total: Option<f64>,
    /// "price:qty;price:qty*;..." cheapest first, `*` on your own listings.
    pub levels: String,
}

/// Reads every addon file `env` points at and merges them.
pub fn load(env: &Env) -> Result<AddonData, String> {
    let mut data = AddonData { files: env.addon_files(), ..AddonData::default() };
    for file in data.files.clone() {
        let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let db = lua::global(&bytes, "ForeverBuddyLiteDB").map_err(|e| e.to_string())?;
        let db = db.map(|v| v.to_json()).unwrap_or(Value::Null);
        let written = std::fs::metadata(&file)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| format::iso_from_millis(d.as_millis() as f64))
            .unwrap_or_default();
        merge_file(&mut data, &db, &file, &written);
    }
    Ok(data)
}

fn merge_file(data: &mut AddonData, db: &Value, file: &Path, written: &str) {
    merge_ah(data, db);
    for (key, item) in json::entries(db.get("items")) {
        if let Ok(item) = ItemRecord::deserialize(item) {
            data.items.insert(key, item);
        }
    }
    for (key, log) in json::entries(db.get("ladders")) {
        let log: Vec<LadderSnapshot> = values(Some(log));
        let newest = |l: &Vec<LadderSnapshot>| l.last().and_then(|s| s.at).unwrap_or(0.0);
        if data.ladders.get(&key).is_none_or(|old| newest(&log) > newest(old)) {
            data.ladders.insert(key, log);
        }
    }
    if let Some(history) = db.get("bgHistory") {
        data.bg_matches.extend(values::<BgMatch>(history.get("matches")));
        data.bg_matches.sort_by(|a, b| a.at.total_cmp(&b.at));
        data.bg_matches.dedup_by(|a, b| a.at == b.at && a.char == b.char);
    }
    if let Some(journal) = db.get("journal") {
        data.journal.extend(values::<JournalEntry>(journal.get("entries")));
        data.journal.sort_by(|a, b| a.t.total_cmp(&b.t));
    }
    for (npc, t) in json::entries(db.get("trainers")) {
        let trainer = Trainer {
            at: t.get("at").and_then(Value::as_f64),
            by: t.get("by").cloned(),
            services: json::list(t.get("services")).into_iter().cloned().collect(),
        };
        if data.trainers.get(&npc).is_none_or(|old| trainer.at.unwrap_or(0.0) > old.at.unwrap_or(0.0)) {
            data.trainers.insert(npc, trainer);
        }
    }
    for (key, raw) in json::entries(db.get("inspected")) {
        let c = character_from_json(&key, raw, file, written);
        if data.inspected.get(&key).is_none_or(|old| c.saved_at.unwrap_or(0.0) > old.saved_at.unwrap_or(0.0)) {
            data.inspected.insert(key, c);
        }
    }
    for (key, raw) in json::entries(db.get("characters")) {
        let c = character_from_json(&key, raw, file, written);
        let newer = data.characters.get(&key).is_none_or(|old| c.saved_at.unwrap_or(0.0) > old.saved_at.unwrap_or(0.0));
        if newer {
            data.characters.insert(key, c);
        }
    }
}

/// Keeps the newest scan. Schema 2 keeps the last scan error under `debug.ah_error`; schema 1 had
/// it at the top as `ahError`.
fn merge_ah(data: &mut AddonData, db: &Value) {
    let Some(ah) = db.get("ah").filter(|v| json::truthy(v)) else { return };
    let at = ah.get("at").and_then(Value::as_f64);
    if data.ah.as_ref().is_some_and(|old| at.unwrap_or(0.0) <= old.at.unwrap_or(0.0)) {
        return;
    }
    let logged_error = db
        .get("debug")
        .and_then(|d| d.get("ah_error"))
        .filter(|v| !v.is_null())
        .or_else(|| db.get("ahError"))
        .and_then(|v| v.as_str().map(String::from));
    data.ah = Some(match unpack_ah(ah) {
        // The error the addon logged; else the one the scan itself kept.
        Ok(scan) => AhScan { error: logged_error.or(scan.error.clone()), ..scan },
        Err(e) => AhScan { at, error: Some(e), ..AhScan::default() },
    });
}

/// Reads the gear scan. The addon stores its items packed (CBOR, deflated, base64, via
/// `C_EncodingUtil`); older files have a plain `items` table.
pub fn unpack_ah(ah: &Value) -> Result<AhScan, String> {
    let mut scan = AhScan {
        at: ah.get("at").and_then(Value::as_f64),
        complete: ah.get("complete").and_then(Value::as_bool),
        error: ah.get("error").and_then(Value::as_str).map(String::from),
        items: None,
    };
    let plain = ah.get("items").filter(|v| json::truthy(v));
    let items_json = match (plain, ah.get("packed").and_then(Value::as_str)) {
        (None, Some(packed)) => unpack_items(packed)?,
        (plain, _) => plain.cloned().unwrap_or(Value::Object(Default::default())),
    };
    scan.items =
        Some(json::entries(Some(&items_json)).into_iter().map(|(k, v)| (k, ItemRecord::deserialize(v).unwrap_or_default())).collect());
    Ok(scan)
}

/// base64 -> inflate (raw deflate, zlib or gzip, whichever reads) -> CBOR -> JSON.
fn unpack_items(packed: &str) -> Result<Value, String> {
    let cleaned: String = packed.chars().filter(|c| !c.is_whitespace()).collect();
    let engine = base64::engine::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    let raw = engine.decode(cleaned.as_bytes()).map_err(|e| format!("AH data: bad base64 ({e})"))?;
    let bytes = inflate_any(&raw).ok_or("AH data: can't inflate the packed scan")?;
    let value = cbor::decode(&bytes).map_err(|e| e.to_string())?;
    Ok(value.to_lua_json())
}

fn inflate_any(raw: &[u8]) -> Option<Vec<u8>> {
    type Inflate = fn(&[u8]) -> std::io::Result<Vec<u8>>;
    let attempts: [Inflate; 3] = [
        |b| read_all(flate2::read::DeflateDecoder::new(b)),
        |b| read_all(flate2::read::ZlibDecoder::new(b)),
        |b| read_all(flate2::read::GzDecoder::new(b)),
    ];
    attempts.iter().find_map(|inflate| inflate(raw).ok())
}

fn read_all(mut r: impl Read) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    r.read_to_end(&mut out)?;
    Ok(out)
}

fn character_from_json(key: &str, raw: &Value, file: &Path, written: &str) -> Character {
    let section = |name: &str| raw.get(name);
    let mail = section("mail").filter(|v| v.is_object() || v.is_array()).map(|m| Mailbox {
        at: m.get("at").and_then(Value::as_f64),
        letters: json::list(m.get("letters"))
            .into_iter()
            .map(|l| Letter {
                sender: l.get("sender").and_then(Value::as_str).map(String::from),
                items: values(l.get("items")),
                raw: l.clone(),
            })
            .collect(),
    });
    let mut raw_with_source = raw.clone();
    if let Value::Object(o) = &mut raw_with_source {
        o.insert("_key".into(), Value::String(key.to_string()));
        o.insert("_file".into(), Value::String(file.display().to_string()));
        o.insert("_written".into(), Value::String(written.to_string()));
    }
    Character {
        key: key.to_string(),
        file: file.to_path_buf(),
        written: written.to_string(),
        saved_at: section("saved_at").and_then(Value::as_f64),
        identity: or_default(section("identity")),
        gear: super::lists::keyed(section("gear")),
        bags: section("bags").and_then(|v| Container::deserialize(v).ok()),
        bank: section("bank").and_then(|v| Container::deserialize(v).ok()),
        quests: or_default(section("quests")),
        skills: values(section("skills")),
        talents: section("talents").and_then(|v| Talents::deserialize(v).ok()),
        spells: values(section("spells")),
        reputation: values(section("reputation")),
        mail,
        professions: super::lists::keyed(section("professions")),
        raw: raw_with_source,
    }
}

/// One level of a price ladder.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct LadderLevel {
    /// Copper each.
    pub price: i64,
    pub qty: i64,
    /// Your own listing.
    pub mine: bool,
}

/// "price:qty;price:qty*" -> levels, in the order written (cheapest first). Malformed parts are skipped.
pub fn parse_ladder(s: &str) -> Vec<LadderLevel> {
    s.split(';').filter(|p| !p.is_empty()).filter_map(parse_level).collect()
}

fn parse_level(part: &str) -> Option<LadderLevel> {
    let (rest, mine) = match part.strip_suffix('*') {
        Some(r) => (r, true),
        None => (part, false),
    };
    let (price, qty) = rest.split_once(':')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(price) || !digits(qty) {
        return None;
    }
    Some(LadderLevel { price: price.parse().ok()?, qty: qty.parse().ok()?, mine })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladders_parse_cheapest_first_with_your_own_marked() {
        let l = parse_ladder("2100:5;3800:20*;3900:40");
        assert_eq!(
            l,
            [
                LadderLevel { price: 2100, qty: 5, mine: false },
                LadderLevel { price: 3800, qty: 20, mine: true },
                LadderLevel { price: 3900, qty: 40, mine: false },
            ]
        );
    }

    #[test]
    fn ladder_junk_is_skipped() {
        assert_eq!(parse_ladder("").len(), 0);
        assert_eq!(parse_ladder(";;5:1;x:2;3:;:4;7:8**;-1:2;9:9").len(), 2);
    }

    #[test]
    fn unpacks_a_plain_items_table() {
        let scan = unpack_ah(&serde_json::json!({"items": {"a": {"id": 1}}})).unwrap();
        assert_eq!(scan.items.unwrap()[0].1.id, Some(1.0));
    }

    #[test]
    fn the_scan_error_is_the_logged_one_else_the_scans_own() {
        let error_of = |db: Value| {
            let mut data = AddonData::default();
            merge_ah(&mut data, &db);
            data.ah.unwrap().error
        };
        let scan = serde_json::json!({"at": 1, "items": {}, "error": "scan stopped"});
        assert_eq!(error_of(serde_json::json!({"ah": scan})).as_deref(), Some("scan stopped"));
        assert_eq!(error_of(serde_json::json!({"ah": scan, "debug": {"ah_error": "browse failed"}})).as_deref(), Some("browse failed"));
        assert_eq!(error_of(serde_json::json!({"ah": scan, "ahError": "old schema"})).as_deref(), Some("old schema"));
        assert_eq!(error_of(serde_json::json!({"ah": {"at": 1, "items": {}}})), None);
    }
}
