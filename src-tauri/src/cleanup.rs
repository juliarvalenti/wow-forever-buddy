//! Bag cleanup (B3, IMPLEMENTING §18, INGAME §14): items marked in the app to
//! sell, or to send to another of the account's characters. The addon shows
//! the marks in the bags (the Cleanup slot); selling and sending stay manual,
//! in game. A mark clears itself once its item has left the character
//! (ingest).
//!
//! B3b: the app also suggests marks, never applying them by itself: greys
//! to sell, gear another character could use (TIP2's upgrade rules) to send
//! to it, and gear this character has outgrown to sell. Each carries a
//! fixed reason code, never free text. A dismissed suggestion doesn't come
//! back for that item.

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::sv::{LuaTable, LuaValue};

/// What to do with an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Mark {
    Sell,
    /// To another character (its id).
    Send {
        to: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Recipient {
    pub id: u32,
    pub name: String,
    /// File token, lowercase, for the class colour.
    pub class: Option<String>,
}

/// Why a mark was made or suggested (B3b): a fixed code, never free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum Reason {
    Grey,
    Outgrown,
    /// `gain` item levels for the character it's sent to.
    Upgrade {
        gain: u32,
    },
}

impl Reason {
    fn code(self) -> &'static str {
        match self {
            Reason::Grey => "grey",
            Reason::Outgrown => "outgrown",
            Reason::Upgrade { .. } => "upgrade",
        }
    }

    fn gain(self) -> Option<u32> {
        match self {
            Reason::Upgrade { gain } => Some(gain),
            _ => None,
        }
    }

    fn from_row(code: Option<String>, gain: Option<u32>) -> Option<Reason> {
        match code.as_deref() {
            Some("grey") => Some(Reason::Grey),
            Some("outgrown") => Some(Reason::Outgrown),
            Some("upgrade") => Some(Reason::Upgrade {
                gain: gain.unwrap_or(0),
            }),
            _ => None,
        }
    }
}

/// One marked (or suggested) item of a character.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Marked {
    pub item_id: u32,
    pub name: String,
    pub quality: Option<u8>,
    pub icon: Option<u32>,
    /// How many the character holds (bags and bank).
    pub count: u32,
    /// The vendor's price for one, when the game gave it.
    pub sell_price: Option<f64>,
    /// `None` for sell.
    pub to: Option<Recipient>,
    pub reason: Option<Reason>,
    /// "app", or "agent:<client name>" for an approved proposal (a claim).
    pub producer: String,
}

/// The sheet's Bag cleanup panel.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Cleanup {
    pub marks: Vec<Marked>,
    /// What the app suggests marking (B3b), never applied by itself.
    pub suggestions: Vec<Marked>,
    pub delivery: crate::bridge::Delivery,
}

fn flavor_of(c: &rusqlite::Connection, character: u32) -> AppResult<String> {
    c.query_row(
        "SELECT flavor FROM characters WHERE id = ?1",
        [character],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("character {character}")))
}

fn touch(c: &rusqlite::Connection, flavor: &str) -> AppResult<()> {
    c.execute(
        "INSERT OR REPLACE INTO cleanup_changed (flavor, changed_at) VALUES (?1, ?2)",
        params![flavor, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// When `flavor`'s marks last changed (RFC 3339), empty if never.
pub fn changed_at(db: &Db, flavor: &str) -> AppResult<String> {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT changed_at FROM cleanup_changed WHERE flavor = ?1",
            [flavor],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_default())
    })
}

/// Marks one of `character`'s items. Only an item it holds (bags or bank);
/// a send only to another character of the same flavor, and never a
/// soulbound item.
pub fn mark(db: &Db, character: u32, item: u32, mark: Mark) -> AppResult<()> {
    db.with_conn(|c| put(c, character, item, mark, None, "app"))
}

/// The one way a mark is written: the player's own, an accepted
/// suggestion, or an approved agent proposal (`producer` "agent:<client>").
pub fn put(
    c: &rusqlite::Connection,
    character: u32,
    item: u32,
    mark: Mark,
    reason: Option<Reason>,
    producer: &str,
) -> AppResult<()> {
    {
        let flavor = flavor_of(c, character)?;
        let (held, bound): (i64, i64) = c.query_row(
            "SELECT count(*), coalesce(max(bound), 0) FROM char_items
             WHERE character_id = ?1 AND item_id = ?2 AND location IN ('bag', 'bank')",
            params![character, item],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if held == 0 {
            return Err(AppError::NotFound(format!(
                "item {item} on character {character}"
            )));
        }
        let to = match mark {
            Mark::Sell => None,
            Mark::Send { to } => {
                if to == character {
                    return Err(AppError::InvalidSettings(
                        "mark: send to another character".into(),
                    ));
                }
                if bound != 0 {
                    return Err(AppError::InvalidSettings(
                        "mark: soulbound, so it can't be mailed".into(),
                    ));
                }
                if flavor_of(c, to)? != flavor {
                    return Err(AppError::NotFound(format!("character {to}")));
                }
                Some(to)
            }
        };
        c.execute(
            "INSERT OR REPLACE INTO cleanup_marks (character_id, item_id, action, to_character_id,
                                                   created_at, reason, gain, producer)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                character,
                item,
                if to.is_some() { "send" } else { "sell" },
                to,
                chrono::Utc::now().to_rfc3339(),
                reason.map(Reason::code),
                reason.and_then(Reason::gain),
                producer
            ],
        )?;
        touch(c, &flavor)
    }
}

pub fn clear(db: &Db, character: u32, item: u32) -> AppResult<()> {
    db.with_conn(|c| {
        let flavor = flavor_of(c, character)?;
        c.execute(
            "DELETE FROM cleanup_marks WHERE character_id = ?1 AND item_id = ?2",
            params![character, item],
        )?;
        touch(c, &flavor)
    })
}

/// The inventory slots an equip location fills, within TIP2's scope (INGAME
/// §8 (b)): rings and trinkets compare with the lower of their two slots.
/// Weapons, shields, off-hands, ranged, shirts and tabards are out of scope.
fn slots_for(equip: &str) -> &'static [i64] {
    match equip {
        "INVTYPE_HEAD" => &[1],
        "INVTYPE_NECK" => &[2],
        "INVTYPE_SHOULDER" => &[3],
        "INVTYPE_CHEST" | "INVTYPE_ROBE" => &[5],
        "INVTYPE_WAIST" => &[6],
        "INVTYPE_LEGS" => &[7],
        "INVTYPE_FEET" => &[8],
        "INVTYPE_WRIST" => &[9],
        "INVTYPE_HAND" => &[10],
        "INVTYPE_CLOAK" => &[15],
        "INVTYPE_FINGER" => &[11, 12],
        "INVTYPE_TRINKET" => &[13, 14],
        _ => &[],
    }
}

/// TIP2's armour rule: can a character of `class` at `level` wear it?
fn can_wear(
    class: &str,
    level: i64,
    class_id: Option<i64>,
    subclass: Option<i64>,
    required: i64,
) -> bool {
    if class_id != Some(4) {
        return true;
    }
    let forty = level >= 40 || required >= 40;
    match subclass {
        Some(0) | Some(1) => true,
        Some(2) => !matches!(class, "mage" | "priest" | "warlock"),
        Some(3) => {
            matches!(class, "warrior" | "paladin")
                || (matches!(class, "hunter" | "shaman") && forty)
        }
        Some(4) => matches!(class, "warrior" | "paladin") && forty,
        _ => false,
    }
}

/// The smallest gain that's worth suggesting (TIP2's +5).
const MIN_GAIN: i64 = 5;

struct Gear {
    item: u32,
    quality: Option<u8>,
    ilvl: Option<i64>,
    equip: Option<String>,
    class_id: Option<i64>,
    subclass: Option<i64>,
    required: i64,
    bound: bool,
    bind_known: bool,
}

/// What the app would suggest for each of `character`'s bag items that
/// isn't marked or dismissed: the mark and its reason.
fn suggest(c: &rusqlite::Connection, character: u32) -> AppResult<Vec<(u32, Mark, Reason)>> {
    let flavor = flavor_of(c, character)?;
    // Every character of the flavor: class, level, and its worn item levels.
    let mut stmt = c.prepare(
        "SELECT id, lower(coalesce(class, '')), coalesce(level, 0) FROM characters WHERE flavor = ?1",
    )?;
    let chars = stmt
        .query_map([&flavor], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = c.prepare(
        "SELECT i.character_id, i.slot, max(coalesce(it.ilvl, 0))
         FROM char_items i JOIN characters ch ON ch.id = i.character_id
         LEFT JOIN items it ON it.item_id = i.item_id
         WHERE ch.flavor = ?1 AND i.location = 'equipped'
         GROUP BY i.character_id, i.slot",
    )?;
    let worn: HashMap<(u32, i64), i64> = stmt
        .query_map([&flavor], |r| {
            Ok((
                (r.get::<_, u32>(0)?, r.get::<_, i64>(1)?),
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    // The lower of the slots it would fill (0 for an empty one).
    let wears = |who: u32, slots: &[i64]| {
        slots
            .iter()
            .map(|s| worn.get(&(who, *s)).copied().unwrap_or(0))
            .min()
    };

    let mut stmt = c.prepare(
        "SELECT i.item_id, it.quality, it.ilvl, it.equip_loc, it.class_id, it.subclass_id,
                coalesce(it.min_level, 0), max(i.bound), min(i.bind_known)
         FROM char_items i LEFT JOIN items it ON it.item_id = i.item_id
         WHERE i.character_id = ?1 AND i.location = 'bag'
           AND i.item_id NOT IN (SELECT item_id FROM cleanup_marks WHERE character_id = ?1)
           AND i.item_id NOT IN (SELECT item_id FROM cleanup_dismissed WHERE character_id = ?1)
         GROUP BY i.item_id
         ORDER BY i.item_id",
    )?;
    let gear = stmt
        .query_map([character], |r| {
            Ok(Gear {
                item: r.get(0)?,
                quality: r.get(1)?,
                ilvl: r.get(2)?,
                equip: r.get(3)?,
                class_id: r.get(4)?,
                subclass: r.get(5)?,
                required: r.get(6)?,
                bound: r.get::<_, i64>(7)? != 0,
                bind_known: r.get::<_, i64>(8)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let me = chars.iter().find(|ch| ch.0 == character);
    let mut out = Vec::new();
    for g in gear {
        if g.quality == Some(0) {
            out.push((g.item, Mark::Sell, Reason::Grey));
            continue;
        }
        let (Some(ilvl), Some(equip)) = (g.ilvl, g.equip.as_deref()) else {
            continue;
        };
        let slots = slots_for(equip);
        if slots.is_empty() {
            continue;
        }
        // The other character it would help most: the biggest gain, ties
        // to the higher level.
        let best = chars
            .iter()
            .filter(|(id, class, level)| {
                *id != character && can_wear(class, *level, g.class_id, g.subclass, g.required)
            })
            .filter_map(|(id, _, level)| {
                let gain = ilvl - wears(*id, slots)?;
                (gain >= MIN_GAIN).then_some((gain, *level, *id))
            })
            .max();
        match best {
            Some((gain, _, to)) if g.bind_known && !g.bound => {
                out.push((
                    g.item,
                    Mark::Send { to },
                    Reason::Upgrade { gain: gain as u32 },
                ));
            }
            // Someone could use it, but it may be mailable: no guess.
            Some(_) if !g.bind_known => {}
            _ => {
                // Below what this character wears there, and no one else
                // can have it: outgrown.
                let below = me.is_some_and(|(_, class, level)| {
                    can_wear(class, *level, g.class_id, g.subclass, g.required)
                        && wears(character, slots).is_some_and(|w| ilvl < w)
                });
                if below {
                    out.push((g.item, Mark::Sell, Reason::Outgrown));
                }
            }
        }
    }
    Ok(out)
}

/// Marks a suggestion (one item, or all of them with `None`), with its
/// reason. Returns how many were marked.
pub fn accept(db: &Db, character: u32, item: Option<u32>) -> AppResult<u32> {
    db.with_conn(|c| {
        let mut n = 0;
        for (id, mark, reason) in suggest(c, character)? {
            if item.is_none_or(|i| i == id) {
                put(c, character, id, mark, Some(reason), "app")?;
                n += 1;
            }
        }
        if item.is_some() && n == 0 {
            return Err(AppError::NotFound(format!(
                "suggestion for item {}",
                item.unwrap_or(0)
            )));
        }
        Ok(n)
    })
}

/// Dismisses a suggestion: it doesn't come back for that item.
pub fn dismiss(db: &Db, character: u32, item: u32) -> AppResult<()> {
    db.with_conn(|c| {
        flavor_of(c, character)?;
        c.execute(
            "INSERT OR IGNORE INTO cleanup_dismissed (character_id, item_id) VALUES (?1, ?2)",
            params![character, item],
        )?;
        Ok(())
    })
}

/// A suggestion as the panel shows it: the item, and who it would go to.
fn row(
    c: &rusqlite::Connection,
    character: u32,
    item: u32,
    mark: Mark,
    reason: Reason,
) -> AppResult<Marked> {
    let (name, quality, icon, sell, count): (String, Option<u8>, Option<u32>, Option<i64>, u32) = c
        .query_row(
            "SELECT coalesce(it.name, 'Item ' || ?2), it.quality, it.icon_file_id, it.sell_price,
                (SELECT coalesce(sum(count), 0) FROM char_items
                 WHERE character_id = ?1 AND item_id = ?2 AND location IN ('bag', 'bank'))
         FROM (SELECT 1) LEFT JOIN items it ON it.item_id = ?2",
            params![character, item],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
    let to = match mark {
        Mark::Sell => None,
        Mark::Send { to } => Some(c.query_row(
            "SELECT id, name, lower(class) FROM characters WHERE id = ?1",
            [to],
            |r| {
                Ok(Recipient {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    class: r.get(2)?,
                })
            },
        )?),
    };
    Ok(Marked {
        item_id: item,
        name,
        quality,
        icon,
        count,
        sell_price: sell.filter(|&p| p > 0).map(|p| p as f64),
        to,
        reason: Some(reason),
        producer: "app".into(),
    })
}

/// The panel's contents (without `delivery`, which the command fills in).
pub fn view(db: &Db, character: u32) -> AppResult<Cleanup> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT m.item_id, coalesce(it.name, 'Item ' || m.item_id), it.quality, it.icon_file_id,
                    (SELECT coalesce(sum(count), 0) FROM char_items
                     WHERE character_id = m.character_id AND item_id = m.item_id
                       AND location IN ('bag', 'bank')),
                    it.sell_price, ch.id, ch.name, lower(ch.class), m.reason, m.gain, m.producer
             FROM cleanup_marks m
             LEFT JOIN items it ON it.item_id = m.item_id
             LEFT JOIN characters ch ON ch.id = m.to_character_id
             WHERE m.character_id = ?1
             ORDER BY m.action DESC, coalesce(it.name, ''), m.item_id",
        )?;
        let marks = stmt
            .query_map([character], |r| {
                let to: Option<u32> = r.get(6)?;
                Ok(Marked {
                    item_id: r.get(0)?,
                    name: r.get(1)?,
                    quality: r.get(2)?,
                    icon: r.get(3)?,
                    count: r.get(4)?,
                    sell_price: r
                        .get::<_, Option<i64>>(5)?
                        .filter(|&p| p > 0)
                        .map(|p| p as f64),
                    to: match to {
                        Some(id) => Some(Recipient {
                            id,
                            name: r.get(7)?,
                            class: r.get(8)?,
                        }),
                        None => None,
                    },
                    reason: Reason::from_row(r.get(9)?, r.get(10)?),
                    producer: r.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut suggestions = Vec::new();
        for (item, mark, reason) in suggest(c, character)? {
            suggestions.push(row(c, character, item, mark, reason)?);
        }
        Ok(Cleanup {
            marks,
            suggestions,
            delivery: crate::bridge::Delivery::Waiting,
        })
    })
}

fn key(k: &str) -> LuaValue {
    LuaValue::str(k)
}

fn table(array: Vec<LuaValue>, hash: Vec<(LuaValue, LuaValue)>) -> LuaValue {
    LuaValue::Table(Box::new(LuaTable { array, hash }))
}

/// The Cleanup slot's body: `alts` (name, surname, class), then `marks`, by
/// the marking character's index: a flat list, five per mark, of item id,
/// "sell" or "send", the recipient's index (0 for sell), the reason code
/// ("grey", "outgrown", "upgrade", or "" for none) and the gain (0 unless
/// an upgrade).
pub fn slot_entries(db: &Db, flavor: &str) -> AppResult<Vec<(LuaValue, LuaValue)>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT id, name, coalesce(surname, ''), upper(coalesce(class, ''))
             FROM characters WHERE flavor = ?1 ORDER BY id",
        )?;
        let alts = stmt
            .query_map([flavor], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let index: HashMap<i64, i64> = alts
            .iter()
            .enumerate()
            .map(|(i, a)| (a.0, i as i64 + 1))
            .collect();
        let mut stmt = c.prepare(
            "SELECT m.character_id, m.item_id, m.action, m.to_character_id,
                    coalesce(m.reason, ''), coalesce(m.gain, 0)
             FROM cleanup_marks m JOIN characters ch ON ch.id = m.character_id
             WHERE ch.flavor = ?1 ORDER BY m.character_id, m.item_id",
        )?;
        let mut by: Vec<(i64, Vec<LuaValue>)> = Vec::new();
        for row in stmt.query_map([flavor], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })? {
            let (who, item, action, to, reason, gain) = row?;
            let (Some(&owner), to) = (index.get(&who), to.and_then(|t| index.get(&t)).copied())
            else {
                continue;
            };
            if action == "send" && to.is_none() {
                continue;
            }
            let entry = [
                LuaValue::Int(item),
                LuaValue::str(action),
                LuaValue::Int(to.unwrap_or(0)),
                LuaValue::str(reason),
                LuaValue::Int(gain),
            ];
            match by.last_mut() {
                Some((o, flat)) if *o == owner => flat.extend(entry),
                _ => by.push((owner, entry.into())),
            }
        }
        let alts_value = table(
            alts.iter()
                .map(|(_, name, surname, class)| {
                    table(
                        Vec::new(),
                        vec![
                            (key("name"), LuaValue::str(name)),
                            (key("surname"), LuaValue::str(surname)),
                            (key("class"), LuaValue::str(class)),
                        ],
                    )
                })
                .collect(),
            Vec::new(),
        );
        let marks_value = table(
            Vec::new(),
            by.into_iter()
                .map(|(owner, flat)| (LuaValue::Int(owner), table(flat, Vec::new())))
                .collect(),
        );
        Ok(vec![(key("alts"), alts_value), (key("marks"), marks_value)])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAVOR: &str = "_classic_beta_";

    /// Thrandor (1, warrior 60) carries 3 Broken Fangs (grey), Truestrike
    /// Shoulders (leather 63), an old plate helm (50), a ring whose bind
    /// state the game never reported, and a soulbound Hearthstone. He wears
    /// 66 in head and shoulders. Sela (2, priest 40) wears nothing; Kaelor
    /// (4, rogue 30) wears 20 in shoulders. Evil (3) is on another flavor.
    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            for (id, flavor, name, class, level) in [
                (1, FLAVOR, "Thrandor", "WARRIOR", 60),
                (2, FLAVOR, "Sela", "PRIEST", 40),
                (3, "_retail_", "Evil", "PRIEST", 60),
                (4, FLAVOR, "Kaelor", "ROGUE", 30),
            ] {
                c.execute(
                    "INSERT INTO characters (id, flavor, account, group_dir, char_dir, name, class,
                                             level, first_seen, last_seen)
                     VALUES (?1, ?2, 'A', '70', ?3, ?3, ?4, ?5, 1, 1)",
                    params![id, flavor, name, class, level],
                )?;
            }
            // id, name, quality, sell, ilvl, equip, class, subclass, required
            for (id, name, quality, sell, ilvl, equip, class, sub, req) in [
                (3299, "Broken Fang", 0, 6, None, None, None, None, None),
                (16000, "Truestrike Shoulders", 3, 5000, Some(63), Some("INVTYPE_SHOULDER"), Some(4), Some(2), Some(58)),
                (2001, "Old Plate Helm", 2, 900, Some(50), Some("INVTYPE_HEAD"), Some(4), Some(4), Some(40)),
                (2002, "Mystery Ring", 2, 300, Some(30), Some("INVTYPE_FINGER"), Some(4), Some(0), Some(25)),
                (6948, "Hearthstone", 1, 0, None, None, None, None, None),
                (3001, "Worn Helm", 4, 0, Some(66), Some("INVTYPE_HEAD"), Some(4), Some(4), Some(60)),
                (3002, "Worn Pauldrons", 4, 0, Some(66), Some("INVTYPE_SHOULDER"), Some(4), Some(4), Some(60)),
                (3003, "Rag Shoulders", 1, 0, Some(20), Some("INVTYPE_SHOULDER"), Some(4), Some(2), Some(15)),
            ] {
                c.execute(
                    "INSERT INTO items (item_id, name, quality, sell_price, ilvl, equip_loc, class_id,
                                        subclass_id, min_level, seen_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1)",
                    params![id, name, quality, sell, ilvl, equip, class, sub, req],
                )?;
            }
            for (slot, item, count, bound, known) in [
                (1, 3299, 3, 0, 1),
                (2, 16000, 1, 0, 1),
                (3, 6948, 1, 1, 1),
                (4, 2001, 1, 1, 1),
                (5, 2002, 1, 0, 0),
            ] {
                c.execute(
                    "INSERT INTO char_items (character_id, location, container, slot, item_id, link, count,
                                             as_of, bound, bind_known)
                     VALUES (1, 'bag', 0, ?1, ?2, '', ?3, 1, ?4, ?5)",
                    params![slot, item, count, bound, known],
                )?;
            }
            for (who, slot, item) in [(1, 1, 3001), (1, 3, 3002), (4, 3, 3003)] {
                c.execute(
                    "INSERT INTO char_items (character_id, location, container, slot, item_id, link, count, as_of)
                     VALUES (?1, 'equipped', 0, ?2, ?3, '', 1, 1)",
                    params![who, slot, item],
                )?;
            }
            Ok(())
        })
        .unwrap();
        db
    }

    #[test]
    fn suggestions() {
        let db = db();
        let got: Vec<(u32, Option<u32>, Option<Reason>)> = view(&db, 1)
            .unwrap()
            .suggestions
            .iter()
            .map(|s| (s.item_id, s.to.as_ref().map(|t| t.id), s.reason))
            .collect();
        assert_eq!(
            got,
            [
                // Plate no one else can wear, below his 66 helm.
                (2001, None, Some(Reason::Outgrown)),
                (3299, None, Some(Reason::Grey)),
                // Leather: not for Sela (a priest); +43 over Kaelor's 20.
                (16000, Some(4), Some(Reason::Upgrade { gain: 43 })),
            ],
            "the ring's bind state is unknown, so no send; the Hearthstone isn't gear"
        );

        dismiss(&db, 1, 2001).unwrap();
        assert_eq!(accept(&db, 1, Some(3299)).unwrap(), 1);
        assert_eq!(accept(&db, 1, None).unwrap(), 1, "the rest: the shoulders");
        let v = view(&db, 1).unwrap();
        assert!(v.suggestions.is_empty(), "dismissed, or marked now");
        let send = v.marks.iter().find(|m| m.item_id == 16000).unwrap();
        assert_eq!(send.to.as_ref().unwrap().name, "Kaelor");
        assert_eq!(send.reason, Some(Reason::Upgrade { gain: 43 }));
        let grey = v.marks.iter().find(|m| m.item_id == 3299).unwrap();
        assert_eq!(
            (grey.count, grey.sell_price, grey.reason),
            (3, Some(6.0), Some(Reason::Grey))
        );
        assert_eq!(grey.producer, "app");

        // A cleared mark comes back as a suggestion; a dismissed one doesn't.
        clear(&db, 1, 3299).unwrap();
        let back: Vec<u32> = view(&db, 1)
            .unwrap()
            .suggestions
            .iter()
            .map(|s| s.item_id)
            .collect();
        assert_eq!(back, [3299]);
        assert!(accept(&db, 1, Some(2001)).is_err(), "dismissed");
        assert!(!changed_at(&db, FLAVOR).unwrap().is_empty());
    }

    #[test]
    fn marks_are_checked() {
        let db = db();
        assert!(mark(&db, 1, 12345, Mark::Sell).is_err(), "not held");
        assert!(
            mark(&db, 1, 16000, Mark::Send { to: 1 }).is_err(),
            "to itself"
        );
        assert!(
            mark(&db, 1, 16000, Mark::Send { to: 3 }).is_err(),
            "another flavor"
        );
        assert!(
            mark(&db, 1, 6948, Mark::Send { to: 2 }).is_err(),
            "soulbound"
        );
        assert!(
            mark(&db, 1, 6948, Mark::Sell).is_ok(),
            "a bound item can still be sold"
        );
        assert!(mark(&db, 9, 6948, Mark::Sell).is_err(), "unknown character");
    }

    /// The slot: alts once, marks by the owner's index, recipients by index.
    #[test]
    fn the_slot() {
        let db = db();
        mark(&db, 1, 3299, Mark::Sell).unwrap();
        accept(&db, 1, Some(16000)).unwrap();
        let mut t = crate::bridge::header(1);
        t.hash.extend(slot_entries(&db, FLAVOR).unwrap());
        let slot = crate::bridge::Slot::Cleanup;
        let LuaValue::Table(t) =
            crate::bridge::check(slot, &crate::bridge::render(slot, t).unwrap()).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(t.get("alts").unwrap().as_table().unwrap().array.len(), 3);
        let mine = t
            .get("marks")
            .unwrap()
            .as_table()
            .unwrap()
            .get_index(1)
            .unwrap()
            .as_table()
            .unwrap();
        assert_eq!(
            mine.array,
            [
                // The player's own mark: no reason.
                LuaValue::Int(3299),
                LuaValue::str("sell"),
                LuaValue::Int(0),
                LuaValue::str(""),
                LuaValue::Int(0),
                // An accepted suggestion: to Kaelor (index 3), +43.
                LuaValue::Int(16000),
                LuaValue::str("send"),
                LuaValue::Int(3),
                LuaValue::str("upgrade"),
                LuaValue::Int(43),
            ]
        );
    }

    /// A mark goes once its item has left the character (ingest).
    #[test]
    fn marks_clear_when_the_item_is_gone() {
        use crate::ingest::{ingest_bytes, target_for};
        let fixture = |name: &str| {
            std::fs::read(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/addon")
                    .join(name),
            )
            .unwrap()
        };
        let db = Db::open_in_memory().unwrap();
        let t = target_for(
            FLAVOR,
            "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
        )
        .unwrap();
        ingest_bytes(&db, &t, &fixture("first_login.lua")).unwrap();
        // The first login's bags hold Linen Cloth (2589); mark it, plus an
        // item it doesn't hold any more.
        mark(&db, 1, 2589, Mark::Sell).unwrap();
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO cleanup_marks (character_id, item_id, action, created_at) VALUES (1, 4242, 'sell', 'x')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        ingest_bytes(&db, &t, &fixture("second_login.lua")).unwrap();
        let left: Vec<u32> = view(&db, 1)
            .unwrap()
            .marks
            .iter()
            .map(|m| m.item_id)
            .collect();
        assert_eq!(
            left,
            [2589],
            "the gone item's mark cleared, the held one kept"
        );
    }
}
