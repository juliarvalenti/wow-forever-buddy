//! Shopping lists and alt errands (B2, IMPLEMENTING §15, INGAME §10): what
//! the player is gathering across characters, how much of it their
//! characters already hold, and which alt can send what to whom.
//!
//! A list may be for one character ("for Sela"). Then whatever that
//! character is still short of, and another alt holds, becomes an errand:
//! "Coinpurse can send 20". The app works these out, and the Lists slot
//! carries them as they stand; in game, the addon adds only what the
//! current character holds right now. Nothing here or there ever buys,
//! attaches or sends.
//!
//! Lists come from the app or, once P2c lands, from an approved proposal
//! (`producer` "agent:<client name>", a claim). Every string is checked for
//! length here and shown as plain text in game.

use std::collections::{BTreeMap, HashMap};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::ah;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::sv::{LuaTable, LuaValue};

const MAX_NAME: usize = 60;
const MAX_ITEM_NAME: usize = 100;
const MAX_LISTS: i64 = 30;
const MAX_ITEMS: i64 = 100;
const MAX_NEED: u32 = 9999;

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Who {
    pub id: u32,
    pub name: String,
    /// File token, lowercase (`warrior`), for the class colour; empty if
    /// unknown.
    pub class: String,
}

/// What one character holds of an item, as of its last logout.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct ListHolding {
    pub character: Who,
    pub bags: u32,
    pub bank: u32,
    pub mail: u32,
    /// When the addon last saw these (RFC 3339, UTC): the oldest of the
    /// places counted.
    pub as_of: String,
}

/// An alt that can send some of what the list's character still needs.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Errand {
    pub from: Who,
    pub count: u32,
    /// How many of `count` are in its bags, the rest in its bank.
    pub in_bags: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct ListItem {
    pub id: u32,
    /// `None` for an item typed as free text that no character has seen.
    pub item_id: Option<u32>,
    pub name: String,
    pub quality: Option<u8>,
    pub icon_file_id: Option<u32>,
    pub need: u32,
    /// All characters together, bags, bank and mail.
    pub have: u32,
    /// Most first.
    pub holders: Vec<ListHolding>,
    /// The last scan's lowest buyout, in copper.
    pub price: Option<f64>,
    pub errands: Vec<Errand>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct List {
    pub id: u32,
    pub name: String,
    pub for_character: Option<Who>,
    /// "app" or "agent:<client name>", as a claim.
    pub producer: String,
    pub created_at: String,
    pub items: Vec<ListItem>,
}

/// What to add: an item your characters have seen, or free text.
#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum NewItem {
    Id(u32),
    Name(String),
}

fn invalid(why: &str) -> AppError {
    AppError::InvalidSettings(format!("list: {why}"))
}

fn fits(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max && !s.contains(['\n', '\r'])
}

/// Notes that `flavor`'s lists changed now.
fn touch(c: &rusqlite::Connection, flavor: &str) -> AppResult<()> {
    c.execute(
        "INSERT OR REPLACE INTO lists_changed (flavor, changed_at) VALUES (?1, ?2)",
        params![flavor, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// When `flavor`'s lists last changed (RFC 3339), if ever.
pub fn changed_at(db: &Db, flavor: &str) -> AppResult<Option<String>> {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT changed_at FROM lists_changed WHERE flavor = ?1",
            [flavor],
            |r| r.get(0),
        )
        .optional()?)
    })
}

/// The flavor of the list holding `item`.
fn item_flavor(c: &rusqlite::Connection, item: u32) -> AppResult<String> {
    c.query_row(
        "SELECT l.flavor FROM list_items li JOIN lists l ON l.id = li.list_id WHERE li.id = ?1",
        [item],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("list item {item}")))
}

fn check_for(c: &rusqlite::Connection, flavor: &str, character: Option<u32>) -> AppResult<()> {
    if let Some(id) = character {
        let known: bool = c.query_row(
            "SELECT EXISTS (SELECT 1 FROM characters WHERE id = ?1 AND flavor = ?2)",
            params![id, flavor],
            |r| r.get(0),
        )?;
        if !known {
            return Err(AppError::NotFound(format!("character {id}")));
        }
    }
    Ok(())
}

pub fn create_list(
    db: &Db,
    flavor: &str,
    name: &str,
    for_character: Option<u32>,
    producer: &str,
) -> AppResult<u32> {
    if !fits(name, MAX_NAME) {
        return Err(invalid("a name of 1 to 60 characters"));
    }
    db.with_conn(|c| {
        check_for(c, flavor, for_character)?;
        let count: i64 = c.query_row(
            "SELECT count(*) FROM lists WHERE flavor = ?1",
            [flavor],
            |r| r.get(0),
        )?;
        if count >= MAX_LISTS {
            return Err(invalid("at most 30 lists"));
        }
        c.execute(
            "INSERT INTO lists (flavor, name, for_character_id, producer, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                flavor,
                name.trim(),
                for_character,
                producer,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        let id = c.last_insert_rowid() as u32;
        touch(c, flavor)?;
        Ok(id)
    })
}

fn list_flavor(c: &rusqlite::Connection, list_id: u32) -> AppResult<String> {
    c.query_row("SELECT flavor FROM lists WHERE id = ?1", [list_id], |r| {
        r.get(0)
    })
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("list {list_id}")))
}

/// Renames a list or changes who it's for.
pub fn update_list(db: &Db, list_id: u32, name: &str, for_character: Option<u32>) -> AppResult<()> {
    if !fits(name, MAX_NAME) {
        return Err(invalid("a name of 1 to 60 characters"));
    }
    db.with_conn(|c| {
        let flavor = list_flavor(c, list_id)?;
        check_for(c, &flavor, for_character)?;
        c.execute(
            "UPDATE lists SET name = ?2, for_character_id = ?3 WHERE id = ?1",
            params![list_id, name.trim(), for_character],
        )?;
        touch(c, &flavor)
    })
}

pub fn delete_list(db: &Db, list_id: u32) -> AppResult<()> {
    db.with_conn(|c| {
        let flavor = list_flavor(c, list_id)?;
        c.execute("DELETE FROM lists WHERE id = ?1", [list_id])?;
        touch(c, &flavor)
    })
}

/// Adds an item, or sets its need if it's already on the list. Free text
/// that names exactly one item a character has seen becomes that item.
pub fn add_item(db: &Db, list_id: u32, item: &NewItem, need: u32) -> AppResult<u32> {
    if !(1..=MAX_NEED).contains(&need) {
        return Err(invalid("a need of 1 to 9999"));
    }
    db.with_conn(|c| {
        let tx = c.transaction()?;
        let flavor = list_flavor(&tx, list_id)?;
        let (item_id, name) = match item {
            NewItem::Id(id) => (Some(*id), None),
            NewItem::Name(n) => {
                if !fits(n, MAX_ITEM_NAME) {
                    return Err(invalid("an item name of 1 to 100 characters"));
                }
                let n = n.trim();
                let mut stmt =
                    tx.prepare("SELECT item_id FROM items WHERE name = ?1 COLLATE NOCASE LIMIT 2")?;
                let ids = stmt
                    .query_map([n], |r| r.get::<_, u32>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                match ids[..] {
                    [id] => (Some(id), None),
                    _ => (None, Some(n.to_string())),
                }
            }
        };
        let existing: Option<u32> = tx
            .query_row(
                "SELECT id FROM list_items WHERE list_id = ?1
                 AND (item_id = ?2 OR (item_id IS NULL AND name = ?3 COLLATE NOCASE))",
                params![list_id, item_id, name],
                |r| r.get(0),
            )
            .optional()?;
        let id = match existing {
            Some(id) => {
                tx.execute(
                    "UPDATE list_items SET need = ?2 WHERE id = ?1",
                    params![id, need],
                )?;
                id
            }
            None => {
                let (count, next): (i64, i64) = tx.query_row(
                    "SELECT count(*), coalesce(max(position), 0) + 1 FROM list_items
                     WHERE list_id = ?1",
                    [list_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                if count >= MAX_ITEMS {
                    return Err(invalid("at most 100 items a list"));
                }
                tx.execute(
                    "INSERT INTO list_items (list_id, item_id, name, need, position)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![list_id, item_id, name, need, next],
                )?;
                tx.last_insert_rowid() as u32
            }
        };
        touch(&tx, &flavor)?;
        tx.commit()?;
        Ok(id)
    })
}

pub fn set_need(db: &Db, item: u32, need: u32) -> AppResult<()> {
    if !(1..=MAX_NEED).contains(&need) {
        return Err(invalid("a need of 1 to 9999"));
    }
    db.with_conn(|c| {
        let flavor = item_flavor(c, item)?;
        c.execute(
            "UPDATE list_items SET need = ?2 WHERE id = ?1",
            params![item, need],
        )?;
        touch(c, &flavor)
    })
}

pub fn remove_item(db: &Db, item: u32) -> AppResult<()> {
    db.with_conn(|c| {
        let flavor = item_flavor(c, item)?;
        c.execute("DELETE FROM list_items WHERE id = ?1", [item])?;
        touch(c, &flavor)
    })
}

/// An item some character has seen, for "+ Add an item…".
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct SeenItem {
    pub item_id: u32,
    pub name: String,
    pub quality: Option<u8>,
    pub icon_file_id: Option<u32>,
}

/// Up to 20 seen items whose name contains `query`, names starting with it
/// first.
pub fn search_seen(db: &Db, query: &str) -> AppResult<Vec<SeenItem>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let escaped = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT item_id, name, quality, icon_file_id FROM items
             WHERE name LIKE '%' || ?1 || '%' ESCAPE '\\'
             ORDER BY name NOT LIKE ?1 || '%' ESCAPE '\\', name LIMIT 20",
        )?;
        let rows = stmt
            .query_map([escaped], |r| {
                Ok(SeenItem {
                    item_id: r.get(0)?,
                    name: r.get(1)?,
                    quality: r.get(2)?,
                    icon_file_id: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn rfc3339(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

struct Row {
    id: u32,
    list_id: u32,
    item_id: Option<u32>,
    name: String,
    quality: Option<u8>,
    icon: Option<u32>,
    need: u32,
}

/// Every list of `flavor`, with holdings, prices and errands worked out.
pub fn lists(db: &Db, flavor: &str) -> AppResult<Vec<List>> {
    let (mut out, rows, chars, held) = db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT l.id, l.name, l.producer, l.created_at,
                    ch.id, ch.name, lower(coalesce(ch.class, ''))
             FROM lists l LEFT JOIN characters ch ON ch.id = l.for_character_id
             WHERE l.flavor = ?1 ORDER BY l.id",
        )?;
        let lists = stmt
            .query_map([flavor], |r| {
                let who: Option<u32> = r.get(4)?;
                Ok(List {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    producer: r.get(2)?,
                    created_at: r.get(3)?,
                    for_character: match who {
                        Some(id) => Some(Who {
                            id,
                            name: r.get(5)?,
                            class: r.get(6)?,
                        }),
                        None => None,
                    },
                    items: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = c.prepare(
            "SELECT li.id, li.list_id, li.item_id,
                    coalesce(it.name, li.name, 'Item ' || li.item_id),
                    it.quality, it.icon_file_id, li.need
             FROM list_items li JOIN lists l ON l.id = li.list_id
             LEFT JOIN items it ON it.item_id = li.item_id
             WHERE l.flavor = ?1 ORDER BY li.list_id, li.position",
        )?;
        let rows = stmt
            .query_map([flavor], |r| {
                Ok(Row {
                    id: r.get(0)?,
                    list_id: r.get(1)?,
                    item_id: r.get(2)?,
                    name: r.get(3)?,
                    quality: r.get(4)?,
                    icon: r.get(5)?,
                    need: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = c.prepare(
            "SELECT id, name, lower(coalesce(class, '')) FROM characters WHERE flavor = ?1",
        )?;
        let chars: HashMap<u32, Who> = stmt
            .query_map([flavor], |r| {
                Ok(Who {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    class: r.get(2)?,
                })
            })?
            .map(|w| w.map(|w| (w.id, w)))
            .collect::<Result<_, _>>()?;
        // What every character holds of the listed items, by place.
        let mut stmt = c.prepare(
            "SELECT i.item_id, i.character_id, i.location, sum(i.count), min(i.as_of)
             FROM char_items i JOIN characters ch ON ch.id = i.character_id
             WHERE ch.flavor = ?1 AND i.location IN ('bag', 'bank', 'mail')
               AND i.item_id IN (SELECT li.item_id FROM list_items li
                                 JOIN lists l ON l.id = li.list_id WHERE l.flavor = ?1)
             GROUP BY i.item_id, i.character_id, i.location",
        )?;
        let mut held: BTreeMap<u32, BTreeMap<u32, ([u32; 3], i64)>> = BTreeMap::new();
        for row in stmt.query_map([flavor], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })? {
            let (item, character, location, count, as_of) = row?;
            let at = match location.as_str() {
                "bag" => 0,
                "bank" => 1,
                _ => 2,
            };
            let e = held
                .entry(item)
                .or_default()
                .entry(character)
                .or_insert(([0; 3], as_of));
            e.0[at] += u32::try_from(count).unwrap_or(0);
            e.1 = e.1.min(as_of);
        }
        Ok((lists, rows, chars, held))
    })?;

    let ids: Vec<u32> = rows.iter().filter_map(|r| r.item_id).collect();
    let prices: HashMap<u32, f64> = ah::prices(db, flavor, &ids)?.into_iter().collect();

    let by_list: HashMap<u32, usize> = out.iter().enumerate().map(|(i, l)| (l.id, i)).collect();
    for row in rows {
        let Some(&at) = by_list.get(&row.list_id) else {
            continue;
        };
        let target = out[at].for_character.as_ref().map(|w| w.id);
        let mut holders: Vec<ListHolding> = row
            .item_id
            .and_then(|id| held.get(&id))
            .into_iter()
            .flatten()
            .filter_map(|(ch, (counts, as_of))| {
                Some(ListHolding {
                    character: chars.get(ch)?.clone(),
                    bags: counts[0],
                    bank: counts[1],
                    mail: counts[2],
                    as_of: rfc3339(*as_of),
                })
            })
            .collect();
        let total = |h: &ListHolding| h.bags + h.bank + h.mail;
        holders.sort_by(|a, b| {
            total(b)
                .cmp(&total(a))
                .then(a.character.name.cmp(&b.character.name))
        });
        let have: u32 = holders.iter().map(total).sum();
        let errands = match target {
            Some(to) => errands(&holders, to, row.need),
            None => Vec::new(),
        };
        out[at].items.push(ListItem {
            id: row.id,
            item_id: row.item_id,
            name: row.name,
            quality: row.quality,
            icon_file_id: row.icon,
            need: row.need,
            have,
            holders,
            price: row.item_id.and_then(|id| prices.get(&id).copied()),
            errands,
        });
    }
    Ok(out)
}

/// What the other alts can send `to` toward `need`: those with the most in
/// their bags first, bags before bank. Mail isn't counted as sendable: it
/// has to be taken out first, and then it's in the bags.
fn errands(holders: &[ListHolding], to: u32, need: u32) -> Vec<Errand> {
    let own: u32 = holders
        .iter()
        .filter(|h| h.character.id == to)
        .map(|h| h.bags + h.bank + h.mail)
        .sum();
    let mut short = need.saturating_sub(own);
    let mut from: Vec<&ListHolding> = holders
        .iter()
        .filter(|h| h.character.id != to && h.bags + h.bank > 0)
        .collect();
    from.sort_by(|a, b| {
        b.bags
            .cmp(&a.bags)
            .then(b.bank.cmp(&a.bank))
            .then(a.character.name.cmp(&b.character.name))
    });
    let mut out = Vec::new();
    for h in from {
        if short == 0 {
            break;
        }
        let count = short.min(h.bags + h.bank);
        out.push(Errand {
            from: h.character.clone(),
            count,
            in_bags: count.min(h.bags),
        });
        short -= count;
    }
    out
}

fn key(k: &str) -> LuaValue {
    LuaValue::str(k)
}

fn table(array: Vec<LuaValue>, hash: Vec<(LuaValue, LuaValue)>) -> LuaValue {
    LuaValue::Table(Box::new(LuaTable { array, hash }))
}

fn int(n: impl Into<i64>) -> LuaValue {
    LuaValue::Int(n.into())
}

/// The Lists slot's body: `alts` (name, surname, class, seen), then `lists`,
/// each with its items' need, holdings by alt index and errands. Alts are
/// referred to by index so a name is written once.
pub fn slot_entries(db: &Db, flavor: &str) -> AppResult<Vec<(LuaValue, LuaValue)>> {
    let lists = lists(db, flavor)?;
    let alts: Vec<(u32, String, String, String, i64)> = db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT id, name, coalesce(surname, ''), upper(coalesce(class, '')), last_seen
             FROM characters WHERE flavor = ?1 ORDER BY last_seen DESC, id",
        )?;
        let rows = stmt
            .query_map([flavor], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    let index: HashMap<u32, i64> = alts
        .iter()
        .enumerate()
        .map(|(i, a)| (a.0, i as i64 + 1))
        .collect();
    let alt = |id: u32| index.get(&id).copied().map(LuaValue::Int);

    let alts_value = table(
        alts.iter()
            .map(|(_, name, surname, class, seen)| {
                table(
                    Vec::new(),
                    vec![
                        (key("name"), LuaValue::str(name)),
                        (key("surname"), LuaValue::str(surname)),
                        (key("class"), LuaValue::str(class)),
                        (key("seen"), LuaValue::Int(*seen)),
                    ],
                )
            })
            .collect(),
        Vec::new(),
    );

    let lists_value = table(
        lists
            .iter()
            .map(|l| {
                let items = l
                    .items
                    .iter()
                    .map(|i| {
                        let mut h = vec![
                            (key("name"), LuaValue::str(&i.name)),
                            (key("need"), int(i.need)),
                        ];
                        if let Some(id) = i.item_id {
                            h.push((key("id"), int(id)));
                        }
                        if let Some(p) = i.price {
                            h.push((key("price"), LuaValue::Int(p.round() as i64)));
                        }
                        // Flat: alt index, bags, bank, mail, as of (Unix).
                        let mut held = Vec::new();
                        for hd in &i.holders {
                            if let Some(a) = alt(hd.character.id) {
                                held.push(a);
                                held.extend([int(hd.bags), int(hd.bank), int(hd.mail)]);
                                held.push(LuaValue::Int(
                                    chrono::DateTime::parse_from_rfc3339(&hd.as_of)
                                        .map(|d| d.timestamp())
                                        .unwrap_or(0),
                                ));
                            }
                        }
                        h.push((key("held"), table(held, Vec::new())));
                        // Flat: from alt index, count, in bags.
                        let mut errands = Vec::new();
                        for e in &i.errands {
                            if let Some(a) = alt(e.from.id) {
                                errands.extend([a, int(e.count), int(e.in_bags)]);
                            }
                        }
                        if !errands.is_empty() {
                            h.push((key("errands"), table(errands, Vec::new())));
                        }
                        table(Vec::new(), h)
                    })
                    .collect();
                let mut h = vec![
                    (key("id"), int(l.id)),
                    (key("name"), LuaValue::str(&l.name)),
                    (key("items"), table(items, Vec::new())),
                ];
                if let Some(a) = l.for_character.as_ref().and_then(|w| alt(w.id)) {
                    h.push((key("for"), a));
                }
                table(Vec::new(), h)
            })
            .collect(),
        Vec::new(),
    );
    Ok(vec![(key("alts"), alts_value), (key("lists"), lists_value)])
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAVOR: &str = "_classic_beta_";
    const THORIUM: u32 = 12359;

    /// Sela (1) gathers for Blacksmithing; Coinpurse (2) holds 34 Thorium in
    /// bags and 300 in the bank; Kaelor (3) has 4 in the bank.
    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            for (id, name, class) in [(1, "Sela", "PALADIN"), (2, "Coinpurse", "ROGUE"), (3, "Kaelor", "WARRIOR")] {
                c.execute(
                    "INSERT INTO characters (id, flavor, account, group_dir, char_dir, name, class,
                                             first_seen, last_seen)
                     VALUES (?1, ?2, 'ACCOUNT1', '70', ?3, ?3, ?4, 1, ?1)",
                    params![id, FLAVOR, name, class],
                )?;
            }
            c.execute(
                "INSERT INTO items (item_id, name, quality, seen_at) VALUES (?1, 'Thorium Bar', 1, 1)",
                [THORIUM],
            )?;
            for (ch, loc, count, as_of) in [(2, "bag", 20, 100), (2, "bag", 14, 100), (2, "bank", 300, 90), (3, "bank", 4, 50)] {
                c.execute(
                    "INSERT INTO char_items (character_id, location, container, slot, item_id, link,
                                             count, as_of)
                     VALUES (?1, ?2, 0, 1, ?3, '', ?4, ?5)",
                    params![ch, loc, THORIUM, count, as_of],
                )?;
            }
            Ok(())
        })
        .unwrap();
        db
    }

    #[test]
    fn holdings_and_errands() {
        let db = db();
        let list = create_list(&db, FLAVOR, "Blacksmithing", Some(1), "app").unwrap();
        add_item(&db, list, &NewItem::Id(THORIUM), 40).unwrap();
        let l = &lists(&db, FLAVOR).unwrap()[0];
        assert_eq!(l.for_character.as_ref().unwrap().name, "Sela");
        let item = &l.items[0];
        assert_eq!(item.name, "Thorium Bar");
        assert_eq!(item.have, 338);
        assert_eq!(item.holders[0].character.name, "Coinpurse");
        assert_eq!((item.holders[0].bags, item.holders[0].bank), (34, 300));
        // Sela holds none: Coinpurse can send 40, 34 of them from its bags.
        assert_eq!(item.errands.len(), 1);
        assert_eq!(item.errands[0].from.name, "Coinpurse");
        assert_eq!((item.errands[0].count, item.errands[0].in_bags), (40, 34));
    }

    #[test]
    fn no_errands_without_a_character_or_once_held() {
        let db = db();
        let list = create_list(&db, FLAVOR, "Raid night", None, "app").unwrap();
        add_item(&db, list, &NewItem::Id(THORIUM), 40).unwrap();
        assert!(lists(&db, FLAVOR).unwrap()[0].items[0].errands.is_empty());
        // For Kaelor, who has 4: needing 4 means nothing to send.
        update_list(&db, list, "Raid night", Some(3)).unwrap();
        add_item(&db, list, &NewItem::Id(THORIUM), 4).unwrap();
        let l = &lists(&db, FLAVOR).unwrap()[0];
        assert_eq!(l.items.len(), 1, "adding again sets the need");
        assert_eq!(l.items[0].need, 4);
        assert!(l.items[0].errands.is_empty());
    }

    #[test]
    fn free_text_resolves_when_it_names_one_seen_item() {
        let db = db();
        let list = create_list(&db, FLAVOR, "Mats", None, "app").unwrap();
        add_item(&db, list, &NewItem::Name("thorium bar".into()), 5).unwrap();
        add_item(&db, list, &NewItem::Name("Mooncloth".into()), 3).unwrap();
        let items = &lists(&db, FLAVOR).unwrap()[0].items;
        assert_eq!(items[0].item_id, Some(THORIUM));
        assert_eq!(
            (items[1].item_id, items[1].name.as_str()),
            (None, "Mooncloth")
        );
        assert_eq!(items[1].have, 0);
    }

    #[test]
    fn lists_are_checked() {
        let db = db();
        assert!(
            create_list(&db, FLAVOR, "", None, "app").is_err(),
            "empty name"
        );
        assert!(
            create_list(&db, FLAVOR, &"x".repeat(61), None, "app").is_err(),
            "long name"
        );
        assert!(
            create_list(&db, FLAVOR, "a\nb", None, "app").is_err(),
            "two lines"
        );
        assert!(
            create_list(&db, FLAVOR, "Mats", Some(9), "app").is_err(),
            "unknown character"
        );
        assert!(
            create_list(&db, "_retail_", "Mats", Some(1), "app").is_err(),
            "other flavor's character"
        );
        let list = create_list(&db, FLAVOR, "Mats", None, "app").unwrap();
        assert!(
            add_item(&db, list, &NewItem::Id(THORIUM), 0).is_err(),
            "need 0"
        );
        assert!(
            add_item(&db, list, &NewItem::Id(THORIUM), 10_000).is_err(),
            "need too big"
        );
        assert!(
            add_item(&db, 99, &NewItem::Id(THORIUM), 1).is_err(),
            "unknown list"
        );
        for i in 0..100 {
            add_item(&db, list, &NewItem::Name(format!("Thing {i}")), 1).unwrap();
        }
        assert!(
            add_item(&db, list, &NewItem::Name("one more".into()), 1).is_err(),
            "full list"
        );
        delete_list(&db, list).unwrap();
        assert!(lists(&db, FLAVOR).unwrap().is_empty());
    }

    /// The slot is data only, names written once, and hostile text is just
    /// a string (the addon escapes it at display).
    #[test]
    fn the_slot() {
        let db = db();
        let list = create_list(&db, FLAVOR, "]] --[[ |cffff0000x", Some(1), "agent:x").unwrap();
        add_item(&db, list, &NewItem::Id(THORIUM), 40).unwrap();
        let mut t = crate::bridge::header(1);
        t.hash.extend(slot_entries(&db, FLAVOR).unwrap());
        let slot = crate::bridge::Slot::Lists;
        let t = match crate::bridge::check(slot, &crate::bridge::render(slot, t).unwrap()).unwrap()
        {
            LuaValue::Table(t) => *t,
            _ => unreachable!(),
        };
        let alts = t.get("alts").unwrap().as_table().unwrap();
        assert_eq!(alts.array.len(), 3);
        let list = t
            .get("lists")
            .unwrap()
            .as_table()
            .unwrap()
            .get_index(1)
            .unwrap()
            .as_table()
            .unwrap();
        assert_eq!(
            list.get("name").and_then(|v| v.as_bytes()),
            Some(&b"]] --[[ |cffff0000x"[..])
        );
        let sela = list.get("for").unwrap().clone();
        let LuaValue::Int(sela) = sela else { panic!() };
        let who = alts.get_index(sela).unwrap().as_table().unwrap();
        assert_eq!(
            who.get("name").and_then(|v| v.as_bytes()),
            Some(&b"Sela"[..])
        );
        let item = list
            .get("items")
            .unwrap()
            .as_table()
            .unwrap()
            .get_index(1)
            .unwrap()
            .as_table()
            .unwrap();
        assert_eq!(item.get("need"), Some(&LuaValue::Int(40)));
        let errands = item.get("errands").unwrap().as_table().unwrap();
        assert_eq!(errands.array.len(), 3);
        assert_eq!(errands.array[1], LuaValue::Int(40));
        assert_eq!(errands.array[2], LuaValue::Int(34));
        let held = item.get("held").unwrap().as_table().unwrap();
        assert_eq!(held.array.len(), 10, "two holders, five numbers each");
    }

    /// The harness's lists scenario: the addon's receipt for the Lists slot
    /// comes back at logout, and "Sent to the game" reads it.
    #[test]
    fn the_receipt_comes_back() {
        use crate::bridge::{delivery, Delivery, Slot};
        use crate::ingest::{ingest_bytes, target_for};
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/addon/lists.lua"),
        )
        .unwrap();
        let db = Db::open_in_memory().unwrap();
        let t = target_for(
            FLAVOR,
            "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
        )
        .unwrap();
        ingest_bytes(&db, &t, &bytes).unwrap();
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO bridge_slots (flavor, slot, stamp, written_at, bytes, status)
                 VALUES (?1, 'Lists', 1790960000, '2026-10-01T00:00:00+00:00', 10, 'written')",
                [FLAVOR],
            )?;
            Ok(())
        })
        .unwrap();
        let changed = "2026-09-30T00:00:00+00:00";
        assert!(matches!(
            delivery(&db, FLAVOR, Slot::Lists, None, changed, true).unwrap(),
            Delivery::Synced { .. }
        ));
    }
}
