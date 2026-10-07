//! Reading one `ForeverBuddy.lua` (docs/specs/v0.2-addon.md §2 "The
//! SavedVariables contract", §4 "Validate, then apply" steps 1–3) into plain
//! structs. Nothing here touches the db.
//!
//! The file is untrusted text (anyone can drop one in WTF, and mail holds
//! other players' words), so every field is optional except the few the
//! contract guarantees, and rejection reasons name the field, never a value.

use crate::sv::{self, LuaTable, LuaValue};

/// The `_meta.schema` versions this build understands.
const SCHEMAS: &[i64] = &[1];

/// Why a file was left out. Stored in `ingest_state.status`; the file is
/// retried when it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// Not a single `ForeverBuddyDB` table (cut off mid-write, or not ours).
    Parse,
    /// Unknown schema, or `_meta.counts` don't match what's in the file
    /// (a half-applied write).
    Integrity,
    /// The character in the file isn't the one whose folder it's in.
    Mismatch,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Parse => "skipped (parse)",
            Status::Integrity => "skipped (integrity)",
            Status::Mismatch => "skipped (mismatch)",
        }
    }
}

/// A file that was left out, and why. `error` names the field at fault and
/// never quotes a value from the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Rejected {
    pub status: Status,
    pub error: String,
}

fn rejected(status: Status, error: impl Into<String>) -> Rejected {
    Rejected {
        status,
        error: error.into(),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterInfo {
    /// UnitName's first return; `None` if the addon couldn't read it.
    pub name: Option<String>,
    /// UnitName's second return on Forever.
    pub surname: Option<String>,
    /// GetRealmName.
    pub realm: Option<String>,
    pub guid: Option<String>,
    pub class: Option<String>,
    pub race: Option<String>,
    pub sex: Option<i64>,
    pub faction: Option<String>,
    pub level: Option<i64>,
    pub guild: Option<String>,
    pub guild_rank: Option<String>,
}

/// One item in a slot: equipped (container 0), a bag, a bank tab, or a mail
/// message (container = message index).
#[derive(Debug, Clone, PartialEq)]
pub struct SlotItem {
    pub container: i64,
    pub slot: i64,
    pub item_id: i64,
    pub link: String,
    pub count: i64,
}

/// A bag or bank tab itself: what the cards' "3 free" and the sheet's
/// "68 of 80 used" come from.
#[derive(Debug, Clone, PartialEq)]
pub struct Container {
    pub container: i64,
    pub name: Option<String>,
    pub size: Option<i64>,
    pub free: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bank {
    pub at: i64,
    pub items: Vec<SlotItem>,
    pub tabs: Vec<Container>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MailMessage {
    pub idx: i64,
    pub sender: Option<String>,
    pub subject: Option<String>,
    pub money: i64,
    pub cod: i64,
    pub days_left: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mail {
    pub at: i64,
    pub messages: Vec<MailMessage>,
    pub items: Vec<SlotItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profession {
    pub name: String,
    pub skill: Option<i64>,
    pub max: Option<i64>,
    pub line: Option<i64>,
    pub spec: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lockout {
    pub name: String,
    pub difficulty: String,
    pub reset_at: Option<i64>,
    pub raid: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub at: i64,
    pub money: Option<i64>,
    pub xp: Option<i64>,
    pub xp_max: Option<i64>,
    pub rested: Option<i64>,
    pub rest_state: Option<String>,
    pub level: Option<i64>,
    pub ilvl_avg: Option<f64>,
    pub ilvl_equipped: Option<f64>,
    pub played_total: Option<i64>,
    pub played_level: Option<i64>,
    pub zone: Option<String>,
    pub subzone: Option<String>,
    /// uiMapID.
    pub map: Option<i64>,
    /// `None` when the file has no such section (an older addon, or a part
    /// V2 doesn't capture yet), as opposed to an empty one.
    pub equipped: Option<Vec<SlotItem>>,
    pub bags: Option<Vec<SlotItem>>,
    /// The bags themselves; present whenever `bags` is.
    pub bag_info: Vec<Container>,
    pub bank: Option<Bank>,
    pub mail: Option<Mail>,
    pub professions: Option<Vec<Profession>>,
    pub lockouts: Option<Vec<Lockout>>,
    /// Every quest the character has completed (addon 0.4.0 on), sorted
    /// ids; `None` from older files or a client without the API. Read, not
    /// stored yet: the quest planner (#94) is what will use it.
    pub quests_done: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemInfo {
    pub item_id: i64,
    pub name: Option<String>,
    pub quality: Option<i64>,
    pub ilvl: Option<i64>,
    pub icon_file_id: Option<i64>,
    pub class_id: Option<i64>,
    pub subclass_id: Option<i64>,
    pub sell_price: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub seq: i64,
    pub at: i64,
    pub kind: String,
    /// Every field but `kind` and `t`, as JSON.
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub login: i64,
    pub logout: Option<i64>,
    pub start_money: Option<i64>,
    pub start_xp: Option<i64>,
    pub start_level: Option<i64>,
    pub start_zone: Option<String>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AddonFile {
    /// `_meta.written`: when the addon wrote the file.
    pub written: Option<i64>,
    pub character: CharacterInfo,
    pub snapshot: Option<Snapshot>,
    pub items: Vec<ItemInfo>,
    pub sessions: Vec<Session>,
    /// Bridge receipts (`bridge`): what each data slot carried when this
    /// character's addon loaded it. Only the known slots are kept.
    pub receipts: Vec<Receipt>,
    /// Login notes the briefing showed (`briefed = { [note id] = time }`,
    /// addon 0.6.0 on), so the app archives the once notes.
    pub briefed: Vec<(i64, i64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Receipt {
    pub slot: crate::bridge::Slot,
    pub stamp: Option<i64>,
    pub schema: Option<i64>,
    pub seen: i64,
}

impl AddonFile {
    /// The file's own time: the snapshot's, else when it was written.
    pub fn at(&self) -> Option<i64> {
        self.snapshot.as_ref().map(|s| s.at).or(self.written)
    }
}

// ---- reading LuaTables --------------------------------------------------

fn tbl<'a>(t: &'a LuaTable, key: &str) -> Option<&'a LuaTable> {
    t.get(key).and_then(LuaValue::as_table)
}

fn text(t: &LuaTable, key: &str) -> Option<String> {
    t.get(key)
        .and_then(LuaValue::to_string_lossy)
        .filter(|s| !s.is_empty())
}

fn int(t: &LuaTable, key: &str) -> Option<i64> {
    t.get(key).and_then(as_int)
}

fn num(t: &LuaTable, key: &str) -> Option<f64> {
    t.get(key).and_then(LuaValue::as_f64)
}

fn as_int(v: &LuaValue) -> Option<i64> {
    match *v {
        LuaValue::Int(i) => Some(i),
        LuaValue::Num(n) if n.fract() == 0.0 && n.abs() < 9e15 => Some(n as i64),
        _ => None,
    }
}

/// Entries in a table, positional and keyed: what the addon's `count` gives.
fn entries(t: Option<&LuaTable>) -> i64 {
    t.map_or(0, |t| (t.array.len() + t.hash.len()) as i64)
}

/// `(key, value)` for every entry: positional ones get 1-based indices.
fn pairs(t: &LuaTable) -> impl Iterator<Item = (Option<i64>, &LuaValue)> {
    t.array
        .iter()
        .enumerate()
        .map(|(i, v)| (Some(i as i64 + 1), v))
        .chain(t.hash.iter().map(|(k, v)| (as_int(k), v)))
}

/// The item id in an item link (`|Hitem:2589:…`), or `None`.
pub fn item_id(link: &str) -> Option<i64> {
    let rest = &link[link.find("item:")? + 5..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn slot_item(container: i64, slot: i64, v: &LuaValue) -> Option<SlotItem> {
    // `equipped[slot] = link`, or `{ link = …, count = … }`.
    let (link, count) = match v {
        LuaValue::Str(_) => (v.to_string_lossy()?, 1),
        LuaValue::Table(t) => (text(t, "link")?, int(t, "count").unwrap_or(1)),
        _ => return None,
    };
    Some(SlotItem {
        container,
        slot,
        item_id: item_id(&link)?,
        link,
        count,
    })
}

/// `{ [container] = { items = { [slot] = … } } }`: bags and bank tabs. A
/// container may also hold its slots directly.
fn containers(t: &LuaTable) -> Vec<SlotItem> {
    let mut out = Vec::new();
    for (container, v) in pairs(t) {
        let (Some(container), Some(c)) = (container, v.as_table()) else {
            continue;
        };
        let slots = tbl(c, "items").unwrap_or(c);
        for (slot, item) in pairs(slots) {
            if let Some(slot) = slot {
                out.extend(slot_item(container, slot, item));
            }
        }
    }
    out
}

/// The containers in a bags or bank-tabs table: `{ [i] = { name, size, free } }`.
fn container_info(t: &LuaTable) -> Vec<Container> {
    pairs(t)
        .filter_map(|(container, v)| {
            let c = v.as_table()?;
            Some(Container {
                container: container?,
                name: text(c, "name"),
                size: int(c, "size"),
                free: int(c, "free"),
            })
        })
        .collect()
}

fn json(v: &LuaValue) -> serde_json::Value {
    use serde_json::Value;
    match v {
        LuaValue::Nil => Value::Null,
        LuaValue::Bool(b) => Value::Bool(*b),
        LuaValue::Int(i) => Value::from(*i),
        LuaValue::Num(n) => serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number),
        LuaValue::Str(_) => Value::String(v.to_string_lossy().unwrap_or_default()),
        LuaValue::Table(t) => {
            if t.hash.is_empty() {
                Value::Array(t.array.iter().map(json).collect())
            } else {
                let mut map = serde_json::Map::new();
                for (k, v) in pairs(t) {
                    let key = match k {
                        Some(i) => i.to_string(),
                        None => continue,
                    };
                    map.insert(key, json(v));
                }
                for (k, v) in &t.hash {
                    if let Some(s) = k.to_string_lossy() {
                        map.insert(s, json(v));
                    }
                }
                Value::Object(map)
            }
        }
    }
}

// ---- decoding ------------------------------------------------------------

/// Steps 1–2 of the spec: exactly one `ForeverBuddyDB` table, a known schema,
/// and `_meta.counts` that match. Step 3 (the folder) is `matches_folder`.
pub fn decode(bytes: &[u8]) -> Result<AddonFile, Rejected> {
    // The parser's message can quote the file's text; only the line is kept.
    let globals = sv::parse(bytes).map_err(|e| {
        rejected(
            Status::Parse,
            format!("not a complete saved-variables file (line {})", e.line),
        )
    })?;
    let db = match globals.as_slice() {
        [(name, value)] if name == "ForeverBuddyDB" => value
            .as_table()
            .ok_or_else(|| rejected(Status::Parse, "ForeverBuddyDB isn't a table"))?,
        _ => {
            return Err(rejected(
                Status::Parse,
                "expected exactly one global, ForeverBuddyDB",
            ))
        }
    };

    let meta = tbl(db, "_meta").ok_or_else(|| rejected(Status::Integrity, "_meta is missing"))?;
    match int(meta, "schema") {
        Some(s) if SCHEMAS.contains(&s) => {}
        _ => {
            return Err(rejected(
                Status::Integrity,
                "_meta.schema is missing or unknown",
            ))
        }
    }
    check_counts(db, meta)?;

    let character = tbl(db, "character").map(character).unwrap_or_default();
    // The addon keeps the level on `character`, written at the same moment
    // as the snapshot; the snapshot row records it too.
    let snapshot = tbl(db, "snapshot").and_then(snapshot).map(|mut s| {
        s.level = s.level.or(character.level);
        s
    });
    let items = tbl(db, "items").map(items).unwrap_or_default();
    let sessions = tbl(db, "sessions").map(sessions).unwrap_or_default();
    let receipts = tbl(db, "bridge").map(receipts).unwrap_or_default();
    let briefed = tbl(db, "briefed").map(briefed).unwrap_or_default();
    Ok(AddonFile {
        written: int(meta, "written"),
        character,
        snapshot,
        items,
        sessions,
        receipts,
        briefed,
    })
}

/// `briefed = { [note id] = time shown }`: positive ids and times only, and
/// at most 100 (a character sees one note per login).
fn briefed(t: &LuaTable) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = pairs(t)
        .filter_map(|(id, v)| Some((id.filter(|&i| i > 0)?, as_int(v).filter(|&t| t > 0)?)))
        .collect();
    out.sort_unstable();
    out.truncate(100);
    out
}

/// `bridge = { Tooltip1 = { stamp, schema, seen }, … }`; unknown slots and
/// receipts without a time are skipped.
fn receipts(t: &LuaTable) -> Vec<Receipt> {
    crate::bridge::SLOTS
        .iter()
        .filter_map(|&slot| {
            let r = tbl(t, slot.name())?;
            Some(Receipt {
                slot,
                stamp: int(r, "stamp"),
                schema: int(r, "schema"),
                seen: int(r, "seen")?,
            })
        })
        .collect()
}

/// `_meta.counts` against a recount of the tables they describe (spec §2).
fn check_counts(db: &LuaTable, meta: &LuaTable) -> Result<(), Rejected> {
    let counts = tbl(meta, "counts")
        .ok_or_else(|| rejected(Status::Integrity, "_meta.counts is missing"))?;
    let sessions = tbl(db, "sessions");
    let events: i64 = sessions
        .into_iter()
        .flat_map(|t| pairs(t).map(|(_, v)| v))
        .map(|s| entries(s.as_table().and_then(|s| tbl(s, "events"))))
        .sum();
    let bag_items: i64 = tbl(db, "snapshot")
        .and_then(|s| tbl(s, "bags"))
        .into_iter()
        .flat_map(|t| pairs(t).map(|(_, v)| v))
        .map(|b| entries(b.as_table().and_then(|b| tbl(b, "items"))))
        .sum();
    let checks = [
        ("sessions", entries(sessions)),
        ("events", events),
        ("items", entries(tbl(db, "items"))),
        ("bag_items", bag_items),
    ];
    for (field, actual) in checks {
        if int(counts, field) != Some(actual) {
            return Err(rejected(
                Status::Integrity,
                format!("_meta.counts.{field} doesn't match the file"),
            ));
        }
    }
    // Only files with the list (0.4.0 on) count it; either both are there
    // or neither is.
    let quests_done = tbl(db, "snapshot").and_then(|s| tbl(s, "quests_done"));
    if int(counts, "quests_done") != quests_done.map(|q| entries(Some(q))) {
        return Err(rejected(
            Status::Integrity,
            "_meta.counts.quests_done doesn't match the file",
        ));
    }
    Ok(())
}

fn character(t: &LuaTable) -> CharacterInfo {
    let guild = tbl(t, "guild");
    CharacterInfo {
        name: text(t, "name"),
        surname: text(t, "surname"),
        realm: text(t, "realm"),
        guid: text(t, "guid"),
        class: text(t, "class"),
        race: text(t, "race"),
        sex: int(t, "sex"),
        faction: text(t, "faction"),
        level: int(t, "level"),
        guild: guild.and_then(|g| text(g, "name")),
        guild_rank: guild.and_then(|g| text(g, "rank")),
    }
}

fn snapshot(t: &LuaTable) -> Option<Snapshot> {
    let at = int(t, "at")?;
    let ilvl = tbl(t, "ilvl");
    let played = tbl(t, "played");
    let zone = tbl(t, "zone");
    let equipped = tbl(t, "equipped").map(|e| {
        pairs(e)
            .filter_map(|(slot, v)| slot_item(0, slot?, v))
            .collect()
    });
    // `bank = { at, bags = { [bagID] = … } }`. A bank without `bags` reads
    // as no bank at all, so a shape this decoder doesn't know keeps the
    // stored bank instead of replacing it with nothing.
    let bank = tbl(t, "bank").and_then(|b| {
        Some(Bank {
            at: int(b, "at")?,
            items: containers(tbl(b, "bags")?),
            tabs: tbl(b, "bags").map(container_info).unwrap_or_default(),
        })
    });
    let mail = tbl(t, "mail").and_then(|m| {
        let at = int(m, "at")?;
        let mut messages = Vec::new();
        let mut items = Vec::new();
        for (idx, v) in tbl(m, "items").into_iter().flat_map(pairs) {
            let (Some(idx), Some(msg)) = (idx, v.as_table()) else {
                continue;
            };
            messages.push(MailMessage {
                idx,
                sender: text(msg, "sender"),
                subject: text(msg, "subject"),
                money: int(msg, "money").unwrap_or(0),
                cod: int(msg, "cod").unwrap_or(0),
                days_left: num(msg, "days_left"),
            });
            for (slot, item) in tbl(msg, "items").into_iter().flat_map(pairs) {
                if let Some(slot) = slot {
                    items.extend(slot_item(idx, slot, item));
                }
            }
        }
        Some(Mail {
            at,
            messages,
            items,
        })
    });
    let professions = tbl(t, "professions").map(|p| {
        pairs(p)
            .filter_map(|(_, v)| {
                let p = v.as_table()?;
                Some(Profession {
                    name: text(p, "name")?,
                    skill: int(p, "skill"),
                    max: int(p, "max"),
                    line: int(p, "line"),
                    spec: int(p, "spec"),
                })
            })
            .collect()
    });
    let lockouts = tbl(t, "lockouts").map(|l| {
        pairs(l)
            .filter_map(|(_, v)| {
                let l = v.as_table()?;
                Some(Lockout {
                    name: text(l, "name")?,
                    difficulty: text(l, "difficulty").unwrap_or_default(),
                    reset_at: int(l, "reset_at"),
                    raid: matches!(l.get("raid"), Some(LuaValue::Bool(true))),
                })
            })
            .collect()
    });
    Some(Snapshot {
        at,
        money: int(t, "money"),
        xp: int(t, "xp"),
        xp_max: int(t, "xp_max"),
        rested: int(t, "rested"),
        rest_state: text(t, "rest_state"),
        level: int(t, "level"),
        ilvl_avg: ilvl.and_then(|i| num(i, "avg")),
        ilvl_equipped: ilvl.and_then(|i| num(i, "equipped")),
        played_total: played.and_then(|p| int(p, "total")),
        played_level: played.and_then(|p| int(p, "level")),
        zone: zone.and_then(|z| text(z, "zone")),
        subzone: zone.and_then(|z| text(z, "subzone")),
        map: zone.and_then(|z| int(z, "map")),
        equipped,
        bags: tbl(t, "bags").map(containers),
        bag_info: tbl(t, "bags").map(container_info).unwrap_or_default(),
        bank,
        mail,
        professions,
        lockouts,
        quests_done: tbl(t, "quests_done").map(|q| q.array.iter().filter_map(as_int).collect()),
    })
}

fn items(t: &LuaTable) -> Vec<ItemInfo> {
    pairs(t)
        .filter_map(|(id, v)| {
            let i = v.as_table()?;
            Some(ItemInfo {
                item_id: id?,
                name: text(i, "name"),
                quality: int(i, "quality"),
                ilvl: int(i, "ilvl"),
                icon_file_id: int(i, "icon"),
                class_id: int(i, "class"),
                subclass_id: int(i, "subclass"),
                sell_price: int(i, "sell"),
            })
        })
        .collect()
}

fn sessions(t: &LuaTable) -> Vec<Session> {
    pairs(t)
        .filter_map(|(_, v)| {
            let s = v.as_table()?;
            let start = tbl(s, "start");
            let events = tbl(s, "events")
                .into_iter()
                .flat_map(pairs)
                .enumerate()
                .filter_map(|(i, (_, e))| {
                    let e = e.as_table()?;
                    let mut data = match json(&LuaValue::Table(Box::new(e.clone()))) {
                        serde_json::Value::Object(m) => m,
                        _ => serde_json::Map::new(),
                    };
                    data.remove("kind");
                    data.remove("t");
                    Some(Event {
                        seq: i as i64 + 1,
                        at: int(e, "t")?,
                        kind: text(e, "kind")?,
                        data: serde_json::Value::Object(data),
                    })
                })
                .collect();
            Some(Session {
                login: int(s, "login").or_else(|| int(s, "id"))?,
                logout: int(s, "logout"),
                start_money: start.and_then(|t| int(t, "money")),
                start_xp: start.and_then(|t| int(t, "xp")),
                start_level: start.and_then(|t| int(t, "level")),
                start_zone: start.and_then(|t| text(t, "zone")),
                events,
            })
        })
        .collect()
}

/// Step 3: the character folder is `name`, or `name-surname` when there's a
/// surname, compared case-insensitively (probe run 1). The realm isn't
/// compared (Forever's group folder is an id). A name the addon couldn't
/// read isn't a mismatch: the folder stands.
pub fn matches_folder(character: &CharacterInfo, char_dir: &str) -> bool {
    let Some(name) = &character.name else {
        return true;
    };
    let folder = char_dir.to_lowercase();
    if folder == name.to_lowercase() {
        return true;
    }
    character
        .surname
        .as_ref()
        .is_some_and(|s| folder == format!("{name}-{s}").to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/addon")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    #[test]
    fn every_harness_fixture_decodes() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/addon");
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let file = decode(&std::fs::read(entry.path()).unwrap())
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert!(file.at().is_some(), "{name}: has a time");
        }
    }

    #[test]
    fn completed_quests_are_read_and_counted() {
        let bytes = fixture("quests.lua");
        let file = decode(&bytes).unwrap();
        let snap = file.snapshot.unwrap();
        assert_eq!(snap.quests_done, Some(vec![7, 176, 783]));
        let accepted = &file.sessions[0].events[0];
        assert_eq!(accepted.kind, "quest_accepted");
        assert_eq!(accepted.data["id"], 176);

        // A count that doesn't match the list is a half-applied write.
        let text = String::from_utf8(bytes).unwrap();
        let wrong = text.replace("[\"quests_done\"] = 3,", "[\"quests_done\"] = 4,");
        assert_ne!(wrong, text);
        let err = decode(wrong.as_bytes()).unwrap_err();
        assert_eq!(err.status, Status::Integrity);
    }

    #[test]
    fn bridge_receipts_are_read() {
        use crate::bridge::Slot;
        let file = decode(&fixture("bridge.lua")).unwrap();
        assert_eq!(
            file.receipts,
            [
                Receipt {
                    slot: Slot::Tooltip1,
                    stamp: Some(1790960000),
                    schema: Some(1),
                    seen: 1790964000
                },
                Receipt {
                    slot: Slot::Tooltip2,
                    stamp: Some(1790960000),
                    schema: Some(2),
                    seen: 1790964000
                },
            ]
        );
        // Files from before 0.4.0 (and the stubs) have none.
        assert!(decode(&fixture("first_login.lua"))
            .unwrap()
            .receipts
            .is_empty());
    }

    #[test]
    fn briefed_note_receipts_are_read() {
        // Addon 0.6.0's login briefing showed note 7 five seconds in.
        let file = decode(&fixture("briefing.lua")).unwrap();
        assert_eq!(file.briefed, [(7, 1790964005)]);
        assert!(decode(&fixture("first_login.lua"))
            .unwrap()
            .briefed
            .is_empty());
    }

    #[test]
    fn the_adventure_fixture_reads_in_full() {
        let file = decode(&fixture("adventure.lua")).unwrap();
        assert_eq!(file.character.name.as_deref(), Some("Thrandor"));
        assert_eq!(file.character.surname.as_deref(), Some("Vargur"));
        assert_eq!(file.character.realm.as_deref(), Some("Classic Beta PvP 2"));
        let s = &file.sessions[0];
        assert_eq!((s.login, s.logout), (1790964000, Some(1790967250)));
        assert_eq!(s.start_money, Some(25000));
        assert_eq!(s.events.len(), 14);
        let quest = &s.events[9];
        assert_eq!(quest.kind, "quest");
        assert_eq!(quest.data["title"], "Wanted: Hogger");
        assert!(quest.data.get("kind").is_none() && quest.data.get("t").is_none());
        assert_eq!(s.events.iter().map(|e| e.seq).max(), Some(14));
    }

    #[test]
    fn a_cut_or_foreign_file_is_a_parse_rejection() {
        let full = fixture("adventure.lua");
        let cut = &full[..full.len() / 2];
        let err = decode(cut).unwrap_err();
        assert_eq!(err.status, Status::Parse);
        let two = b"ForeverBuddyDB = {}\nOther = {}\n";
        assert_eq!(decode(two).unwrap_err().status, Status::Parse);
    }

    #[test]
    fn counts_that_dont_add_up_are_an_integrity_rejection() {
        let text = String::from_utf8(fixture("adventure.lua")).unwrap();
        let wrong = text.replacen("[\"events\"] = 14", "[\"events\"] = 13", 1);
        let err = decode(wrong.as_bytes()).unwrap_err();
        assert_eq!(err.status, Status::Integrity);
        assert_eq!(err.error, "_meta.counts.events doesn't match the file");
        let unknown = text.replacen("[\"schema\"] = 1", "[\"schema\"] = 9", 1);
        assert_eq!(
            decode(unknown.as_bytes()).unwrap_err().status,
            Status::Integrity
        );
    }

    #[test]
    fn the_folder_rule() {
        let c = |name: Option<&str>, surname: Option<&str>| CharacterInfo {
            name: name.map(String::from),
            surname: surname.map(String::from),
            ..Default::default()
        };
        assert!(matches_folder(
            &c(Some("Ellygie"), Some("Vargur")),
            "Ellygie-Vargur"
        ));
        assert!(matches_folder(
            &c(Some("Ellygie"), Some("Vargur")),
            "ellygie-vargur"
        ));
        assert!(matches_folder(&c(Some("Ellygie"), None), "Ellygie"));
        assert!(
            matches_folder(&c(Some("Ellygie"), Some("Vargur")), "Ellygie"),
            "legacy folder"
        );
        assert!(
            matches_folder(&c(None, None), "Anything"),
            "unreadable name: folder stands"
        );
        assert!(!matches_folder(&c(Some("Thrandor"), None), "Ellygie"));
        assert!(!matches_folder(
            &c(Some("Ellygie"), Some("Vargur")),
            "Ellygie-Other"
        ));
    }

    /// Every snapshot section, read from what the real addon writes: the
    /// fixture is generated by tools/addon-test from ForeverBuddy.lua (and
    /// CI's `--check` fails if it drifts), so a format change in the addon
    /// breaks this test instead of silently decoding as empty.
    #[test]
    fn a_full_snapshot_reads_every_section() {
        let file = decode(&fixture("snapshot.lua")).unwrap();
        let s = file.snapshot.unwrap();
        assert_eq!(
            (s.at, s.money, s.level),
            (1790968500, Some(25000), Some(12))
        );
        assert_eq!(
            (s.xp, s.xp_max, s.rested),
            (Some(1200), Some(8800), Some(674))
        );
        assert_eq!(s.rest_state.as_deref(), Some("Rested"));
        assert_eq!((s.ilvl_avg, s.ilvl_equipped), (Some(21.5), Some(20.25)));
        assert_eq!((s.played_total, s.played_level), (Some(24051), Some(4974)));
        assert_eq!(
            (s.zone.as_deref(), s.subzone.as_deref(), s.map),
            (Some("Elwynn Forest"), Some("Goldshire"), Some(1429))
        );
        let equipped = s.equipped.unwrap();
        assert_eq!((equipped[0].slot, equipped[0].item_id), (16, 25));
        let bags = s.bags.unwrap();
        assert_eq!(bags.len(), 2);
        assert_eq!(
            (
                bags[1].container,
                bags[1].slot,
                bags[1].item_id,
                bags[1].count
            ),
            (0, 2, 2589, 4)
        );
        let bank = s.bank.unwrap();
        assert_eq!(bank.at, 1790964600);
        assert_eq!(
            bank.items.len(),
            1,
            "main bank holds one stack, tab 6 is empty"
        );
        assert_eq!(
            (
                bank.items[0].container,
                bank.items[0].item_id,
                bank.items[0].count
            ),
            (-1, 14047, 20)
        );
        let mail = s.mail.unwrap();
        assert_eq!(mail.at, 1790964900);
        let letter = &mail.messages[0];
        assert_eq!(
            (
                letter.idx,
                letter.sender.as_deref(),
                letter.money,
                letter.days_left
            ),
            (1, Some("Coinpurse"), 500, Some(29.5))
        );
        assert_eq!(
            (
                mail.items[0].container,
                mail.items[0].item_id,
                mail.items[0].count
            ),
            (1, 2589, 10)
        );
        let professions = s.professions.unwrap();
        assert_eq!(professions.len(), 3);
        assert_eq!(
            (
                professions[0].name.as_str(),
                professions[0].skill,
                professions[0].line
            ),
            ("Herbalism", Some(60), Some(182))
        );
        let lockouts = s.lockouts.unwrap();
        assert_eq!(
            (lockouts[0].name.as_str(), lockouts[0].difficulty.as_str()),
            ("The Deadmines", "Normal")
        );
        assert_eq!(
            (lockouts[0].reset_at, lockouts[0].raid),
            (Some(1791136801), false)
        );
        assert_eq!(file.items.len(), 4);
        assert!(file
            .items
            .iter()
            .any(|i| i.item_id == 14047 && i.name.as_deref() == Some("Runecloth")));
        assert_eq!(file.character.guild.as_deref(), Some("Hearthguard"));
    }

    /// The fields the fixtures don't carry yet: a raid lockout and a
    /// profession specialization, in the addon's shape.
    #[test]
    fn raid_lockouts_and_profession_specs() {
        let src = br#"ForeverBuddyDB = {
  _meta = { schema = 1, counts = { sessions = 0, events = 0, items = 0, bag_items = 0 } },
  snapshot = {
    at = 1,
    professions = { { name = "Alchemy", skill = 300, max = 300, line = 171, spec = 2 } },
    lockouts = { { name = "Molten Core", difficulty = "Normal", reset_at = 9, raid = true } },
  },
  sessions = {},
}
"#;
        let s = decode(src).unwrap().snapshot.unwrap();
        let p = &s.professions.unwrap()[0];
        assert_eq!((p.line, p.spec), (Some(171), Some(2)));
        assert!(s.lockouts.unwrap()[0].raid);
    }

    /// Rejections never quote the file: the mail sender of a broken file
    /// doesn't appear in the error (spec §4).
    #[test]
    fn errors_name_fields_never_values() {
        let src = br#"ForeverBuddyDB = { _meta = { schema = 1, counts = { sessions = 5, events = 0, items = 0, bag_items = 0 } },
  snapshot = { at = 1, mail = { at = 1, items = { { sender = "SecretSender", subject = "Private" } } } }, sessions = {} }"#;
        let err = decode(src).unwrap_err();
        assert!(!err.error.contains("SecretSender") && !err.error.contains("Private"));
        let broken = br#"ForeverBuddyDB = { sender = "SecretSender" "#;
        let err = decode(broken).unwrap_err();
        assert!(!err.error.contains("SecretSender"), "{}", err.error);
    }

    #[test]
    fn item_ids_come_from_links() {
        assert_eq!(
            item_id("|cff9d9d9d|Hitem:2589::::::::12:::::|h[Linen Cloth]|h|r"),
            Some(2589)
        );
        assert_eq!(item_id("item:117"), Some(117));
        assert_eq!(item_id("no link"), None);
    }
}
