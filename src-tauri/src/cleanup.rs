//! Bag cleanup (B3, IMPLEMENTING §18, INGAME §14): items marked in the app to
//! sell, or to send to another of the account's characters. The addon shows
//! the marks in the bags (the Cleanup slot); selling and sending stay manual,
//! in game. A mark clears itself once its item has left the character
//! (ingest), and "Mark all greys" is a one-off, not a rule.

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

/// One marked item of a character.
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
}

/// The sheet's Bag cleanup panel.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Cleanup {
    pub marks: Vec<Marked>,
    /// Poor-quality items in the bags that aren't marked yet (for "Mark all
    /// greys (6)").
    pub greys: u32,
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
    db.with_conn(|c| {
        let flavor = flavor_of(c, character)?;
        let (held, bound): (i64, i64) = c.query_row(
            "SELECT count(*), coalesce(max(bound), 0) FROM char_items
             WHERE character_id = ?1 AND item_id = ?2 AND location IN ('bag', 'bank')",
            params![character, item],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if held == 0 {
            return Err(AppError::NotFound(format!("item {item} on character {character}")));
        }
        let to = match mark {
            Mark::Sell => None,
            Mark::Send { to } => {
                if to == character {
                    return Err(AppError::InvalidSettings("mark: send to another character".into()));
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
            "INSERT OR REPLACE INTO cleanup_marks (character_id, item_id, action, to_character_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                character,
                item,
                if to.is_some() { "send" } else { "sell" },
                to,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        touch(c, &flavor)
    })
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

/// "Mark all greys": every poor-quality item in the bags, not already
/// marked, to sell. A one-off; new greys aren't marked later.
pub fn mark_greys(db: &Db, character: u32) -> AppResult<u32> {
    db.with_conn(|c| {
        let flavor = flavor_of(c, character)?;
        let n = c.execute(
            "INSERT OR IGNORE INTO cleanup_marks (character_id, item_id, action, created_at)
             SELECT DISTINCT i.character_id, i.item_id, 'sell', ?2
             FROM char_items i JOIN items it ON it.item_id = i.item_id
             WHERE i.character_id = ?1 AND i.location = 'bag' AND it.quality = 0",
            params![character, chrono::Utc::now().to_rfc3339()],
        )?;
        touch(c, &flavor)?;
        Ok(n as u32)
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
                    it.sell_price, ch.id, ch.name, lower(ch.class)
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
                    sell_price: r.get::<_, Option<i64>>(5)?.filter(|&p| p > 0).map(|p| p as f64),
                    to: match to {
                        Some(id) => Some(Recipient {
                            id,
                            name: r.get(7)?,
                            class: r.get(8)?,
                        }),
                        None => None,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let greys: u32 = c.query_row(
            "SELECT count(DISTINCT i.item_id) FROM char_items i JOIN items it ON it.item_id = i.item_id
             WHERE i.character_id = ?1 AND i.location = 'bag' AND it.quality = 0
               AND i.item_id NOT IN (SELECT item_id FROM cleanup_marks WHERE character_id = ?1)",
            [character],
            |r| r.get(0),
        )?;
        Ok(Cleanup {
            marks,
            greys,
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
/// the marking character's index: a flat list of item id, "sell" or
/// "send", and the recipient's index (0 for sell).
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
            "SELECT m.character_id, m.item_id, m.action, m.to_character_id
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
            ))
        })? {
            let (who, item, action, to) = row?;
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

    /// Thrandor (1) carries 3 Broken Fangs (grey), Truestrike Shoulders and a
    /// soulbound Hearthstone; Sela (2) is another character; Evil (3) is on
    /// another flavor.
    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            for (id, flavor, name) in [(1, FLAVOR, "Thrandor"), (2, FLAVOR, "Sela"), (3, "_retail_", "Evil")] {
                c.execute(
                    "INSERT INTO characters (id, flavor, account, group_dir, char_dir, name, class,
                                             first_seen, last_seen)
                     VALUES (?1, ?2, 'A', '70', ?3, ?3, 'PRIEST', 1, 1)",
                    params![id, flavor, name],
                )?;
            }
            for (id, name, quality, sell) in [(3299, "Broken Fang", 0, 6), (16000, "Truestrike Shoulders", 3, 5000), (6948, "Hearthstone", 1, 0)] {
                c.execute(
                    "INSERT INTO items (item_id, name, quality, sell_price, seen_at) VALUES (?1, ?2, ?3, ?4, 1)",
                    params![id, name, quality, sell],
                )?;
            }
            for (slot, item, count, bound) in [(1, 3299, 3, 0), (2, 16000, 1, 0), (3, 6948, 1, 1)] {
                c.execute(
                    "INSERT INTO char_items (character_id, location, container, slot, item_id, link, count, as_of, bound)
                     VALUES (1, 'bag', 0, ?1, ?2, '', ?3, 1, ?4)",
                    params![slot, item, count, bound],
                )?;
            }
            Ok(())
        })
        .unwrap();
        db
    }

    #[test]
    fn marks_and_greys() {
        let db = db();
        assert_eq!(view(&db, 1).unwrap().greys, 1);
        assert_eq!(mark_greys(&db, 1).unwrap(), 1);
        mark(&db, 1, 16000, Mark::Send { to: 2 }).unwrap();
        let v = view(&db, 1).unwrap();
        assert_eq!(v.greys, 0, "marked now");
        assert_eq!(v.marks.len(), 2);
        let sell = v.marks.iter().find(|m| m.item_id == 3299).unwrap();
        assert_eq!(
            (sell.count, sell.sell_price, sell.to.clone()),
            (3, Some(6.0), None)
        );
        let send = v.marks.iter().find(|m| m.item_id == 16000).unwrap();
        assert_eq!(send.to.as_ref().unwrap().name, "Sela");
        clear(&db, 1, 3299).unwrap();
        assert_eq!(view(&db, 1).unwrap().marks.len(), 1);
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
        mark(&db, 1, 16000, Mark::Send { to: 2 }).unwrap();
        let mut t = crate::bridge::header(1);
        t.hash.extend(slot_entries(&db, FLAVOR).unwrap());
        let slot = crate::bridge::Slot::Cleanup;
        let LuaValue::Table(t) =
            crate::bridge::check(slot, &crate::bridge::render(slot, t).unwrap()).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(t.get("alts").unwrap().as_table().unwrap().array.len(), 2);
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
                LuaValue::Int(3299),
                LuaValue::str("sell"),
                LuaValue::Int(0),
                LuaValue::Int(16000),
                LuaValue::str("send"),
                LuaValue::Int(2),
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
