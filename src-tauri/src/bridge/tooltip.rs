//! The tooltip index (bridge spec §5): for every item any character carries,
//! which alts hold it, where, and its last scan price, split across the two
//! `Tooltip` slots by item id. Ids and counts only: the one kind of text is
//! the account's own character names and class tokens, in the `alts` header.
//!
//! An item's entry is a flat list of numbers, the smallest shape the writer
//! emits: the price in copper (0 without one), then five numbers per alt
//! holding it: the alt's index in `alts`, and its count in bags, bank, mail
//! and equipped.
//!
//! Each alt also carries its level and `worn`: the base item level of what
//! it wears in each of the 19 inventory slots (0 for empty), so the addon
//! can say which alt a hovered item would upgrade (TIP2, INGAME §8 (b)).
//!
//! C1 (INGAME §12): each half also has `makes = { [itemID] = { altIndex,
//! "Tailoring", … } }` for the items an alt's recipes make (which alt, with
//! which profession), and each alt has `prof = { [name] =
//! { skill, at } }`, `at` being when its recipes were last read (for the
//! stale greying). An addon from before C1 ignores both.
//!
//! C2: each half also has `mats = { [itemID] = { reagentID, qty, … } }`,
//! the required reagents of the items in its `makes`. The holdings of each
//! reagent are already in `items`, so the addon adds them up across alts.

use std::collections::{BTreeMap, HashMap};

use crate::ah;
use crate::db::Db;
use crate::error::AppResult;
use crate::sv::{LuaTable, LuaValue};

use super::{header, render_capped, Slot};

/// The index's two halves.
pub const TOOLTIP_SLOTS: [Slot; 2] = [Slot::Tooltip1, Slot::Tooltip2];

/// Both slots, ready to write.
pub struct Built {
    pub slots: Vec<(Slot, Vec<u8>)>,
    /// The index didn't fit: each slot holds a header with `tooLarge` and no
    /// items, so the game says so instead of showing wrong counts.
    pub too_large: bool,
}

/// Which slot holds an item.
fn slot_for(item_id: i64) -> Slot {
    if item_id % 2 == 0 {
        Slot::Tooltip1
    } else {
        Slot::Tooltip2
    }
}

/// Where an item is, in the order of an entry's counts.
fn location_index(location: &str) -> Option<usize> {
    match location {
        "bag" => Some(0),
        "bank" => Some(1),
        "mail" => Some(2),
        "equipped" => Some(3),
        _ => None,
    }
}

struct Alt {
    id: i64,
    name: String,
    surname: String,
    class: String,
    seen: i64,
    bank: Option<i64>,
    mail: Option<i64>,
    level: Option<i64>,
}

/// Inventory slots 1-19, as the game numbers them.
const WORN_SLOTS: usize = 19;

fn key(k: &str) -> LuaValue {
    LuaValue::str(k)
}

fn table(array: Vec<LuaValue>, hash: Vec<(LuaValue, LuaValue)>) -> LuaValue {
    LuaValue::Table(Box::new(LuaTable { array, hash }))
}

/// Builds both slots for `flavor`, every character the app knows there (the
/// AddOns folder is shared by all the folder's WTF accounts).
pub fn build(db: &Db, flavor: &str, stamp: i64) -> AppResult<Built> {
    let (alts, rows, worn, hands) = db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT c.id, c.name, coalesce(c.surname, ''), upper(coalesce(c.class, '')),
                    c.last_seen,
                    (SELECT max(as_of) FROM char_items
                     WHERE character_id = c.id AND location = 'bank'),
                    (SELECT max(as_of) FROM char_items
                     WHERE character_id = c.id AND location = 'mail'),
                    c.level
             FROM characters c WHERE c.flavor = ?1
             ORDER BY c.last_seen DESC, c.id",
        )?;
        let alts = stmt
            .query_map([flavor], |r| {
                Ok(Alt {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    surname: r.get(2)?,
                    class: r.get(3)?,
                    seen: r.get(4)?,
                    bank: r.get(5)?,
                    mail: r.get(6)?,
                    level: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // What each alt wears: base item level by inventory slot.
        let mut stmt = c.prepare(
            "SELECT i.character_id, i.slot, max(coalesce(it.ilvl, 0)), max(i.item_id)
             FROM char_items i
             JOIN characters c ON c.id = i.character_id
             LEFT JOIN items it ON it.item_id = i.item_id
             WHERE c.flavor = ?1 AND i.location = 'equipped'
             GROUP BY i.character_id, i.slot",
        )?;
        let mut worn: HashMap<i64, [i64; WORN_SLOTS]> = HashMap::new();
        // TIP3 (a): what's in each hand, as item ids (main, off, ranged), so
        // the addon can tell a two-hander from a one-hander.
        let mut hands: HashMap<i64, [i64; 3]> = HashMap::new();
        for row in stmt.query_map([flavor], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })? {
            let (character, slot, ilvl, item) = row?;
            if let Some(at) = usize::try_from(slot)
                .ok()
                .filter(|s| (1..=WORN_SLOTS).contains(s))
            {
                worn.entry(character).or_insert([0; WORN_SLOTS])[at - 1] = ilvl.max(0);
            }
            if (16..=18).contains(&slot) && item > 0 {
                hands.entry(character).or_insert([0; 3])[(slot - 16) as usize] = item;
            }
        }
        let mut stmt = c.prepare(
            "SELECT i.item_id, i.character_id, i.location, sum(i.count)
             FROM char_items i JOIN characters c ON c.id = i.character_id
             WHERE c.flavor = ?1
             GROUP BY i.item_id, i.character_id, i.location",
        )?;
        let rows = stmt
            .query_map([flavor], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((alts, rows, worn, hands))
    })?;

    // item → alt index (1-based, in `alts` order) → counts by location.
    let index_of: BTreeMap<i64, i64> = alts
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i as i64 + 1))
        .collect();
    let mut items: BTreeMap<i64, BTreeMap<i64, [i64; 4]>> = BTreeMap::new();
    for (item_id, character_id, location, count) in rows {
        let (Some(&alt), Some(at)) = (index_of.get(&character_id), location_index(&location))
        else {
            continue;
        };
        items.entry(item_id).or_default().entry(alt).or_default()[at] += count;
    }

    let ids: Vec<u32> = items
        .keys()
        .filter_map(|&id| u32::try_from(id).ok())
        .collect();
    let prices: BTreeMap<i64, i64> = ah::prices(db, flavor, &ids)?
        .into_iter()
        .map(|(id, p)| (i64::from(id), p.round() as i64))
        .collect();
    let scan_at = ah::status(db, flavor)?
        .last_scan_at
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.timestamp());

    // C1: who can make what (item → alt and profession), and each alt's
    // professions with their skill and when their recipes were read.
    let (made, profs) = db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT r.item_id, r.character_id, r.profession
             FROM char_recipes r JOIN characters ch ON ch.id = r.character_id
             WHERE ch.flavor = ?1",
        )?;
        let made = stmt
            .query_map([flavor], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = c.prepare(
            "SELECT r.character_id, r.profession, p.skill, max(r.scanned_at)
             FROM char_recipes r JOIN characters ch ON ch.id = r.character_id
             LEFT JOIN professions p ON p.character_id = r.character_id AND p.name = r.profession
             WHERE ch.flavor = ?1
             GROUP BY r.character_id, r.profession",
        )?;
        let profs = stmt
            .query_map([flavor], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((made, profs))
    })?;
    // C2: what each craftable item takes, for the items someone can make.
    let reagents: Vec<(i64, i64, i64)> = db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT g.item_id, g.reagent_id, g.qty FROM recipe_reagents g
             WHERE g.item_id IN (SELECT r.item_id FROM char_recipes r
                                 JOIN characters ch ON ch.id = r.character_id
                                 WHERE ch.flavor = ?1)
             ORDER BY g.item_id, g.reagent_id",
        )?;
        let rows = stmt
            .query_map([flavor], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    // item → flat reagent id, quantity, …
    let mut takes: BTreeMap<i64, Vec<LuaValue>> = BTreeMap::new();
    for (item_id, reagent, qty) in reagents {
        takes
            .entry(item_id)
            .or_default()
            .extend([LuaValue::Int(reagent), LuaValue::Int(qty)]);
    }
    let mut mats: HashMap<Slot, Vec<(LuaValue, LuaValue)>> = HashMap::new();
    for (item_id, flat) in takes {
        mats.entry(slot_for(item_id))
            .or_default()
            .push((LuaValue::Int(item_id), table(flat, Vec::new())));
    }
    // item → (alt index, profession), one per alt.
    let mut makes: BTreeMap<i64, BTreeMap<i64, String>> = BTreeMap::new();
    for (item_id, character_id, profession) in made {
        if let Some(&alt) = index_of.get(&character_id) {
            makes
                .entry(item_id)
                .or_default()
                .entry(alt)
                .or_insert(profession);
        }
    }
    let mut prof_of: HashMap<i64, Vec<(LuaValue, LuaValue)>> = HashMap::new();
    for (character_id, name, skill, at) in profs {
        let mut p = vec![(key("at"), LuaValue::Int(at))];
        if let Some(skill) = skill {
            p.push((key("skill"), LuaValue::Int(skill)));
        }
        prof_of
            .entry(character_id)
            .or_default()
            .push((LuaValue::str(name), table(Vec::new(), p)));
    }

    let alts_value = table(
        alts.iter()
            .map(|a| {
                let mut hash = vec![
                    (key("name"), LuaValue::str(&a.name)),
                    (key("surname"), LuaValue::str(&a.surname)),
                    (key("class"), LuaValue::str(&a.class)),
                    (key("seen"), LuaValue::Int(a.seen)),
                ];
                if let Some(t) = a.bank {
                    hash.push((key("bank"), LuaValue::Int(t)));
                }
                if let Some(t) = a.mail {
                    hash.push((key("mail"), LuaValue::Int(t)));
                }
                if let Some(level) = a.level {
                    hash.push((key("level"), LuaValue::Int(level)));
                }
                let slots = worn.get(&a.id).copied().unwrap_or([0; WORN_SLOTS]);
                hash.push((
                    key("worn"),
                    table(
                        slots.iter().map(|&n| LuaValue::Int(n)).collect(),
                        Vec::new(),
                    ),
                ));
                if let Some(h) = hands.get(&a.id) {
                    hash.push((
                        key("hands"),
                        table(h.iter().map(|&n| LuaValue::Int(n)).collect(), Vec::new()),
                    ));
                }
                if let Some(p) = prof_of.get(&a.id) {
                    hash.push((key("prof"), table(Vec::new(), p.clone())));
                }
                table(Vec::new(), hash)
            })
            .collect(),
        Vec::new(),
    );

    let mut halves: HashMap<Slot, Vec<(LuaValue, LuaValue)>> = HashMap::new();
    for (item_id, holders) in &items {
        let mut entry = vec![LuaValue::Int(prices.get(item_id).copied().unwrap_or(0))];
        for (alt, counts) in holders {
            entry.push(LuaValue::Int(*alt));
            entry.extend(counts.iter().map(|&n| LuaValue::Int(n)));
        }
        halves
            .entry(slot_for(*item_id))
            .or_default()
            .push((LuaValue::Int(*item_id), table(entry, Vec::new())));
    }
    let mut makers: HashMap<Slot, Vec<(LuaValue, LuaValue)>> = HashMap::new();
    for (item_id, alts) in &makes {
        // Flat: alt index, profession name, …
        let flat = alts
            .iter()
            .flat_map(|(&a, p)| [LuaValue::Int(a), LuaValue::str(p)])
            .collect();
        makers
            .entry(slot_for(*item_id))
            .or_default()
            .push((LuaValue::Int(*item_id), table(flat, Vec::new())));
    }

    let head = |extra: Vec<(LuaValue, LuaValue)>| {
        let mut t = header(stamp);
        if let Some(at) = scan_at {
            t.hash.push((key("scanAt"), LuaValue::Int(at)));
        }
        t.hash.extend(extra);
        t
    };
    let mut slots = Vec::with_capacity(TOOLTIP_SLOTS.len());
    for slot in TOOLTIP_SLOTS {
        let mut fields = vec![
            (key("alts"), alts_value.clone()),
            (
                key("items"),
                table(Vec::new(), halves.remove(&slot).unwrap_or_default()),
            ),
        ];
        if let Some(m) = makers.remove(&slot) {
            fields.push((key("makes"), table(Vec::new(), m)));
        }
        if let Some(m) = mats.remove(&slot) {
            fields.push((key("mats"), table(Vec::new(), m)));
        }
        let body = head(fields);
        match render_capped(slot, body)? {
            Some(bytes) => slots.push((slot, bytes)),
            None => return too_large(&head),
        }
    }
    Ok(Built {
        slots,
        too_large: false,
    })
}

/// Every slot as a header saying the index didn't fit (spec §5): refuse and
/// report, never truncate.
fn too_large(head: &dyn Fn(Vec<(LuaValue, LuaValue)>) -> LuaTable) -> AppResult<Built> {
    let slots = TOOLTIP_SLOTS
        .iter()
        .map(|&slot| {
            let body = head(vec![(key("tooLarge"), LuaValue::Bool(true))]);
            Ok((slot, super::render(slot, body)?))
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Built {
        slots,
        too_large: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::check;
    use rusqlite::params;

    const FLAVOR: &str = "_classic_beta_";

    /// `(item, location, count)`.
    type Held<'a> = Vec<(i64, &'a str, i64)>;
    /// `(name, class, last_seen, items)`.
    type Char<'a> = (&'a str, &'a str, i64, Held<'a>);

    fn account(chars: &[Char<'_>]) -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            let tx = c.transaction()?;
            {
                let mut item = tx.prepare(
                    "INSERT INTO char_items (character_id, location, container, slot, item_id,
                                             link, count, as_of)
                     VALUES (?1, ?2, 0, ?3, ?4, '', ?5, ?6)",
                )?;
                for (name, class, seen, items) in chars {
                    tx.execute(
                        "INSERT INTO characters (flavor, account, group_dir, char_dir, name,
                                                 class, first_seen, last_seen)
                         VALUES (?1, 'ACCOUNT1', '70', ?2, ?2, ?3, ?4, ?4)",
                        params![FLAVOR, name, class, seen],
                    )?;
                    let id = tx.last_insert_rowid();
                    for (slot, (item_id, location, count)) in items.iter().enumerate() {
                        let as_of = seen - if *location == "bank" { 3600 } else { 0 };
                        item.execute(params![id, location, slot as i64, item_id, count, as_of])?;
                    }
                }
            }
            tx.commit()?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn parsed(built: &Built, slot: Slot) -> LuaTable {
        let (_, bytes) = built.slots.iter().find(|(s, _)| *s == slot).unwrap();
        match check(slot, bytes).unwrap() {
            LuaValue::Table(t) => *t,
            _ => unreachable!(),
        }
    }

    fn ints(v: &LuaValue) -> Vec<i64> {
        v.as_table()
            .unwrap()
            .array
            .iter()
            .map(|x| match x {
                LuaValue::Int(i) => *i,
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn items_split_by_id_with_counts_per_alt_and_place() {
        let db = account(&[
            (
                "Thrandor",
                "paladin",
                2_000,
                vec![(14047, "bag", 12), (14047, "bag", 8), (2589, "equipped", 1)],
            ),
            (
                "Coinpurse",
                "WARRIOR",
                3_000,
                vec![(14047, "bank", 340), (12360, "mail", 24)],
            ),
        ]);
        let built = build(&db, FLAVOR, 42).unwrap();
        assert!(!built.too_large);

        let even = parsed(&built, Slot::Tooltip1);
        assert_eq!(even.get("stamp"), Some(&LuaValue::Int(42)));
        // Last played first; class tokens upper case.
        let alts = even.get("alts").unwrap().as_table().unwrap();
        let first = alts.get_index(1).unwrap().as_table().unwrap();
        assert_eq!(
            first.get("name").and_then(|v| v.as_bytes()),
            Some(&b"Coinpurse"[..])
        );
        assert_eq!(
            first.get("bank"),
            Some(&LuaValue::Int(3_000 - 3_600)),
            "last bank visit"
        );
        let second = alts.get_index(2).unwrap().as_table().unwrap();
        assert_eq!(
            second.get("class").and_then(|v| v.as_bytes()),
            Some(&b"PALADIN"[..])
        );
        assert_eq!(second.get("bank"), None, "never seen at the bank");

        let even_items = even.get("items").unwrap().as_table().unwrap();
        assert_eq!(even_items.hash.len(), 1, "12360 only");
        // No price yet: 0, then Coinpurse (alt 1) with 24 in the mail.
        assert_eq!(
            ints(even_items.get_index(12360).unwrap()),
            [0, 1, 0, 0, 24, 0]
        );

        let odd = parsed(&built, Slot::Tooltip2);
        let odd_items = odd.get("items").unwrap().as_table().unwrap();
        // Runecloth: Coinpurse 340 in the bank, Thrandor 20 in bags.
        assert_eq!(
            ints(odd_items.get_index(14047).unwrap()),
            [0, 1, 0, 340, 0, 0, 2, 20, 0, 0, 0]
        );
        assert_eq!(ints(odd_items.get_index(2589).unwrap()), [0, 2, 0, 0, 0, 1]);
        // The only text is the alts header: no item names anywhere.
        let (_, bytes) = &built.slots[1];
        assert!(!String::from_utf8_lossy(bytes).contains("Runecloth"));
    }

    #[test]
    fn each_alt_carries_its_level_and_worn_item_levels_by_slot() {
        let db = account(&[
            ("Kaelor", "ROGUE", 3_000, vec![]),
            ("Sela", "PRIEST", 2_000, vec![]),
        ]);
        db.with_conn(|c| {
            c.execute_batch(
                "INSERT INTO items (item_id, name, ilvl, seen_at) VALUES
                    (100, 'Cap', 50, 0), (101, 'Ring', 44, 0), (102, 'Unknown ilvl', NULL, 0);
                 UPDATE characters SET level = 52 WHERE name = 'Kaelor';
                 INSERT INTO char_items (character_id, location, container, slot, item_id, link, count, as_of)
                 SELECT id, 'equipped', 0, 1, 100, '', 1, 0 FROM characters WHERE name = 'Kaelor';
                 INSERT INTO char_items (character_id, location, container, slot, item_id, link, count, as_of)
                 SELECT id, 'equipped', 0, 12, 101, '', 1, 0 FROM characters WHERE name = 'Kaelor';
                 INSERT INTO char_items (character_id, location, container, slot, item_id, link, count, as_of)
                 SELECT id, 'equipped', 0, 19, 102, '', 1, 0 FROM characters WHERE name = 'Kaelor';
                 INSERT INTO char_items (character_id, location, container, slot, item_id, link, count, as_of)
                 SELECT id, 'equipped', 0, 16, 103, '', 1, 0 FROM characters WHERE name = 'Kaelor';",
            )?;
            Ok(())
        })
        .unwrap();
        let built = build(&db, FLAVOR, 42).unwrap();
        let t = parsed(&built, Slot::Tooltip1);
        let alts = t.get("alts").unwrap().as_table().unwrap();

        let kaelor = alts.get_index(1).unwrap().as_table().unwrap();
        assert_eq!(kaelor.get("level"), Some(&LuaValue::Int(52)));
        let mut want = [0i64; 19];
        want[0] = 50; // head
        want[11] = 44; // second finger
                       // An item with no known ilvl reads as 0, like an empty slot.
        assert_eq!(ints(kaelor.get("worn").unwrap()), want);
        // TIP3 (a): the main hand's item id, nothing in the other two.
        assert_eq!(ints(kaelor.get("hands").unwrap()), [103, 0, 0]);

        // Nothing worn and no level yet: all zeros, no level key.
        let sela = alts.get_index(2).unwrap().as_table().unwrap();
        assert_eq!(sela.get("level"), None);
        assert_eq!(ints(sela.get("worn").unwrap()), [0; 19]);
        assert_eq!(sela.get("hands"), None);
    }

    /// Synthetic accounts to check spec §5's size table against what the
    /// writer really emits: `alts` characters with `per_alt` distinct items
    /// each, spread over bags, bank and mail, from a pool of `pool` ids.
    fn sized(alts: usize, per_alt: usize, pool: i64) -> Built {
        build(&sized_db(alts, per_alt, pool), FLAVOR, 1_790_000_000).unwrap()
    }

    fn sized_db(alts: usize, per_alt: usize, pool: i64) -> Db {
        let places = ["bag", "bank", "mail"];
        let chars: Vec<(String, Held)> = (0..alts)
            .map(|a| {
                let items = (0..per_alt)
                    .map(|i| {
                        let id = 1000 + (a as i64 * 37 + i as i64 * 7) % pool;
                        (id, places[i % 3], 1 + (i as i64 % 200))
                    })
                    .collect();
                (format!("Alt{a}"), items)
            })
            .collect();
        let rows: Vec<Char> = chars
            .iter()
            .enumerate()
            .map(|(i, (n, items))| (n.as_str(), "MAGE", 1_790_000_000 + i as i64, items.clone()))
            .collect();
        account(&rows)
    }

    /// `crafters` characters each knowing `recipes` recipes (out of a pool
    /// of craftable items), every item taking `reagents` reagents.
    fn with_recipes(db: &Db, crafters: usize, recipes: usize, pool: i64, reagents: i64) {
        db.with_conn(|c| {
            let ids: Vec<i64> = c
                .prepare("SELECT id FROM characters ORDER BY id")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for (a, id) in ids.iter().take(crafters).enumerate() {
                for i in 0..recipes {
                    let item = 50_000 + (a as i64 * 131 + i as i64 * 3) % pool;
                    c.execute(
                        "INSERT OR IGNORE INTO char_recipes (character_id, profession, item_id, scanned_at)
                         VALUES (?1, 'Tailoring', ?2, 1)",
                        rusqlite::params![id, item],
                    )?;
                    for r in 0..reagents {
                        c.execute(
                            "INSERT OR IGNORE INTO recipe_reagents (item_id, reagent_id, qty, seen_at)
                             VALUES (?1, ?2, ?3, 1)",
                            rusqlite::params![item, 1000 + (item * 7 + r * 13) % 3_000, 1 + r],
                        )?;
                    }
                }
            }
            Ok(())
        })
        .unwrap();
    }

    /// C2 adds `mats` to the index: a crafting-heavy account still fits,
    /// and an absurd one says it's too large rather than dropping a half.
    #[test]
    fn materials_fit_or_say_too_large() {
        let db = sized_db(20, 250, 3_000);
        with_recipes(&db, 10, 1_000, 4_000, 6);
        let built = build(&db, FLAVOR, 1_790_000_000).unwrap();
        assert!(!built.too_large);
        for (slot, bytes) in &built.slots {
            eprintln!("crafters: {} is {} KB", slot.name(), bytes.len() / 1024);
            assert!(bytes.len() < crate::bridge::MAX_SLOT_BYTES);
        }
        let db = sized_db(50, 400, 6_000);
        with_recipes(&db, 50, 1_000, 60_000, 8);
        let built = build(&db, FLAVOR, 1_790_000_000).unwrap();
        assert!(built.too_large);
        assert_eq!(built.slots.len(), 2, "both halves, each saying so");
    }

    #[test]
    fn sizes_match_the_spec_estimate() {
        for (what, alts, per_alt, pool, limit) in [
            ("large", 20, 250, 3_000, 512 * 1024),
            ("extreme", 50, 400, 6_000, crate::bridge::MAX_SLOT_BYTES),
        ] {
            let built = sized(alts, per_alt, pool);
            assert!(!built.too_large, "{what}");
            for (slot, bytes) in &built.slots {
                eprintln!("{what}: {} is {} KB", slot.name(), bytes.len() / 1024);
                assert!(bytes.len() < limit, "{what}: {}", slot.name());
            }
        }
    }

    #[test]
    fn too_large_says_so_instead_of_cutting_short() {
        let built = sized(100, 1_500, 20_000);
        assert!(built.too_large);
        for slot in TOOLTIP_SLOTS {
            let t = parsed(&built, slot);
            assert_eq!(t.get("tooLarge"), Some(&LuaValue::Bool(true)));
            assert_eq!(t.get("items"), None);
        }
    }

    /// C1: the harness's recipes scenario (Thrandor's Tailoring makes 2568
    /// and 2572) goes through ingest into `makes` and the alt's `prof`.
    #[test]
    fn who_can_make_what() {
        use crate::ingest::{ingest_bytes, target_for};
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/addon/recipes.lua"),
        )
        .unwrap();
        let db = Db::open_in_memory().unwrap();
        let t = target_for(
            FLAVOR,
            "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
        )
        .unwrap();
        ingest_bytes(&db, &t, &bytes).unwrap();
        let built = build(&db, FLAVOR, 1).unwrap();
        let even = parsed(&built, Slot::Tooltip1);
        let makes = even.get("makes").unwrap().as_table().unwrap();
        let maker = makes.get_index(2568).unwrap().as_table().unwrap();
        assert_eq!(maker.array.len(), 2);
        assert_eq!(maker.array[0], LuaValue::Int(1));
        assert_eq!(maker.array[1].as_bytes(), Some(&b"Tailoring"[..]));
        assert!(makes.get_index(2572).is_some());
        assert_eq!(makes.get_index(2575), None, "not learned");
        // C2: 2568 takes 1 Coarse Thread and 2 Linen Cloth (by reagent id);
        // 2572 lists none.
        let mats = even.get("mats").unwrap().as_table().unwrap();
        let takes = mats.get_index(2568).unwrap().as_table().unwrap();
        assert_eq!(takes.array, [2320, 1, 2589, 2].map(LuaValue::Int));
        assert_eq!(mats.get_index(2572), None);
        assert_eq!(
            parsed(&built, Slot::Tooltip2).get("makes"),
            None,
            "no odd ids"
        );
        let alts = even.get("alts").unwrap().as_table().unwrap();
        let prof = alts.get_index(1).unwrap().as_table().unwrap();
        let tailoring = prof
            .get("prof")
            .unwrap()
            .as_table()
            .unwrap()
            .get("Tailoring")
            .unwrap()
            .as_table()
            .unwrap();
        assert_eq!(tailoring.get("skill"), Some(&LuaValue::Int(34)));
        assert!(matches!(tailoring.get("at"), Some(LuaValue::Int(_))));
    }
}
