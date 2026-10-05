//! Reading Auctionator's account-wide SavedVariables
//! (`WTF/Account/<account>/SavedVariables/Auctionator.lua`), F5. Data only:
//! parsed with `sv`, never executed, and the game folder is only read.
//!
//! What Auctionator writes (its source, v321; Forever runs v339):
//! - `AUCTIONATOR_PRICE_DATABASE = { __dbversion = 8, [<realm root>] = … }`.
//!   On modern clients each realm's value is a string holding CBOR
//!   (`C_EncodingUtil.SerializeCBOR`), decoded to
//!   `{ version = 2, [<item key>] = { m, h, l, a } }`; an older file can
//!   hold the plain table. Both are read.
//! - Item keys: `"12345"` (an item), `"g:<item>:<ilvl>"` (gear at an item
//!   level), `"p:<species>"` (a battle pet, not an item).
//! - `m`: the last minimum price seen. Per day: `h[day]` the highest of
//!   that day's minimum prices, `l[day]` the lowest (only when it differs
//!   from `h`), `a[day]` the most listed. Copper. Days are string keys,
//!   counted from 1 Jan 2020 (`SCAN_DAY_0`, the player's local midnight).
//! - `AUCTIONATOR_SAVEDVARS.TimeOfLastReplicateScan` / `TimeOfLastBrowseScan`:
//!   Unix seconds of the last full and incremental scans, account-wide.
//!
//! Anything unexpected in one item is skipped, never fatal: a third-party
//! file we don't control shouldn't stop the rest from being read.

use chrono::NaiveDate;

use crate::sv::{self, LuaTable, LuaValue};

/// The day Auctionator counts from.
pub fn day_zero() -> NaiveDate {
    NaiveDate::from_ymd_opt(2020, 1, 1).expect("valid date")
}

#[derive(Debug, Clone, PartialEq)]
pub struct DayPrice {
    pub day: NaiveDate,
    /// The lowest minimum price seen that day (copper).
    pub low: i64,
    /// The highest minimum price seen that day.
    pub high: i64,
    pub available: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemPrices {
    pub key: String,
    /// `None` for battle pets (`p:` keys).
    pub item_id: Option<u32>,
    /// The last minimum price seen.
    pub last: Option<i64>,
    /// Oldest first.
    pub days: Vec<DayPrice>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RealmPrices {
    pub realm: String,
    pub items: Vec<ItemPrices>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PriceDb {
    pub realms: Vec<RealmPrices>,
    pub replicate_scan_at: Option<i64>,
    pub browse_scan_at: Option<i64>,
}

/// Why a file couldn't be read at all (a single bad item is skipped instead).
#[derive(Debug, Clone, PartialEq)]
pub enum Unreadable {
    /// Not valid Lua data (possibly caught mid-write: retry later).
    Parse(String),
    /// No `AUCTIONATOR_PRICE_DATABASE`, or not a table.
    NoPrices,
}

/// The item id in an item key: `"12345"` or `"g:12345:180"`; pets have none.
pub fn item_id(key: &str) -> Option<u32> {
    let mut parts = key.split(':');
    match (parts.next(), parts.next()) {
        (Some(id), None) => id.parse().ok(),
        (Some("g" | "gr"), Some(id)) => id.parse().ok(),
        _ => None,
    }
}

/// A value from either source, read the same way.
enum Node<'a> {
    Lua(&'a LuaValue),
    Cbor(&'a ciborium::Value),
}

impl<'a> Node<'a> {
    fn int(&self) -> Option<i64> {
        match self {
            Node::Lua(v) => v.as_f64().filter(|f| f.is_finite()).map(|f| f as i64),
            Node::Cbor(v) => match v {
                ciborium::Value::Integer(i) => i64::try_from(*i).ok(),
                ciborium::Value::Float(f) if f.is_finite() => Some(*f as i64),
                _ => None,
            },
        }
    }

    /// Key/value pairs, keys as text (numbers written as Lua would).
    fn entries(&self) -> Vec<(String, Node<'a>)> {
        match self {
            Node::Lua(LuaValue::Table(t)) => t
                .hash
                .iter()
                .filter_map(|(k, v)| lua_key(k).map(|k| (k, Node::Lua(v))))
                .chain(
                    t.array
                        .iter()
                        .enumerate()
                        .map(|(i, v)| ((i + 1).to_string(), Node::Lua(v))),
                )
                .collect(),
            Node::Cbor(ciborium::Value::Map(m)) => m
                .iter()
                .filter_map(|(k, v)| cbor_key(k).map(|k| (k, Node::Cbor(v))))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn get(&self, name: &str) -> Option<Node<'a>> {
        self.entries()
            .into_iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
}

fn lua_key(k: &LuaValue) -> Option<String> {
    match k {
        LuaValue::Str(_) => k.to_string_lossy(),
        LuaValue::Int(i) => Some(i.to_string()),
        LuaValue::Num(n) if n.fract() == 0.0 => Some((*n as i64).to_string()),
        _ => None,
    }
}

fn cbor_key(k: &ciborium::Value) -> Option<String> {
    match k {
        ciborium::Value::Text(s) => Some(s.clone()),
        ciborium::Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        ciborium::Value::Integer(i) => i64::try_from(*i).ok().map(|i| i.to_string()),
        _ => None,
    }
}

/// `{ ["2469"] = 125 }` → (date, price) pairs.
fn day_map(node: Option<Node<'_>>) -> Vec<(NaiveDate, i64)> {
    let Some(node) = node else { return Vec::new() };
    node.entries()
        .into_iter()
        .filter_map(|(k, v)| {
            let day: u64 = k.parse().ok()?;
            let date = day_zero().checked_add_days(chrono::Days::new(day))?;
            Some((date, v.int()?))
        })
        .collect()
}

fn item(key: String, node: &Node<'_>) -> Option<ItemPrices> {
    let high = day_map(node.get("h"));
    let low = day_map(node.get("l"));
    let available = day_map(node.get("a"));
    let mut days: Vec<DayPrice> = high
        .into_iter()
        .filter(|(_, h)| *h > 0)
        .map(|(day, h)| DayPrice {
            day,
            low: low
                .iter()
                .find(|(d, _)| *d == day)
                .map_or(h, |(_, l)| *l)
                .min(h),
            high: h,
            available: available.iter().find(|(d, _)| *d == day).map(|(_, a)| *a),
        })
        .collect();
    days.sort_by_key(|d| d.day);
    let last = node.get("m").and_then(|m| m.int()).filter(|m| *m > 0);
    if days.is_empty() && last.is_none() {
        return None;
    }
    Some(ItemPrices {
        item_id: item_id(&key),
        key,
        last,
        days,
    })
}

fn realm(name: String, node: &Node<'_>) -> RealmPrices {
    let mut items: Vec<ItemPrices> = node
        .entries()
        .into_iter()
        .filter(|(k, _)| k != "version")
        .filter_map(|(k, v)| item(k, &v))
        .collect();
    items.sort_by(|a, b| a.key.cmp(&b.key));
    RealmPrices { realm: name, items }
}

fn table<'g>(globals: &'g [(String, LuaValue)], name: &str) -> Option<&'g LuaTable> {
    globals
        .iter()
        .rev()
        .find(|(n, _)| n == name)
        .and_then(|(_, v)| v.as_table())
}

/// Decodes a whole `Auctionator.lua`.
pub fn decode(bytes: &[u8]) -> Result<PriceDb, Unreadable> {
    let globals = sv::parse(bytes).map_err(|e| Unreadable::Parse(e.to_string()))?;
    let db = table(&globals, "AUCTIONATOR_PRICE_DATABASE").ok_or(Unreadable::NoPrices)?;
    let mut realms = Vec::new();
    for (k, v) in &db.hash {
        let Some(name) = lua_key(k).filter(|n| !n.starts_with("__")) else {
            continue;
        };
        match v {
            LuaValue::Table(_) => realms.push(realm(name, &Node::Lua(v))),
            LuaValue::Str(bytes) => {
                // A realm whose CBOR doesn't decode is skipped, not fatal.
                if let Ok(value) = ciborium::from_reader::<ciborium::Value, _>(&bytes[..]) {
                    realms.push(realm(name, &Node::Cbor(&value)));
                }
            }
            _ => {}
        }
    }
    realms.sort_by(|a, b| a.realm.cmp(&b.realm));
    let saved = table(&globals, "AUCTIONATOR_SAVEDVARS");
    let at = |name: &str| {
        saved
            .and_then(|t| t.get(name))
            .and_then(LuaValue::as_f64)
            .filter(|t| *t > 0.0)
            .map(|t| t as i64)
    };
    Ok(PriceDb {
        realms,
        replicate_scan_at: at("TimeOfLastReplicateScan"),
        browse_scan_at: at("TimeOfLastBrowseScan"),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ciborium::Value as C;

    fn day(d: u64) -> NaiveDate {
        day_zero().checked_add_days(chrono::Days::new(d)).unwrap()
    }

    fn t(k: &str) -> C {
        C::Text(k.into())
    }

    /// One day in a fixture: (Auctionator day, high, low if different, listed).
    pub(crate) type Day = (u64, i64, Option<i64>, i64);

    /// A realm's prices as Auctionator's CBOR, for fixtures: (key, m, days).
    pub(crate) fn cbor_realm(items: &[(&str, i64, &[Day])]) -> Vec<u8> {
        let mut map = vec![(t("version"), C::Integer(2.into()))];
        for (key, m, days) in items {
            let mut h = Vec::new();
            let mut l = Vec::new();
            let mut a = Vec::new();
            for (d, high, low, avail) in days.iter() {
                h.push((t(&d.to_string()), C::Integer((*high).into())));
                if let Some(low) = low {
                    l.push((t(&d.to_string()), C::Integer((*low).into())));
                }
                a.push((t(&d.to_string()), C::Integer((*avail).into())));
            }
            map.push((
                t(key),
                C::Map(vec![
                    (t("m"), C::Integer((*m).into())),
                    (t("h"), C::Map(h)),
                    (t("l"), C::Map(l)),
                    (t("a"), C::Map(a)),
                ]),
            ));
        }
        let mut out = Vec::new();
        ciborium::into_writer(&C::Map(map), &mut out).unwrap();
        out
    }

    /// A whole file the way WoW writes it, with each realm's CBOR as a Lua
    /// string (the serializer escapes the binary bytes).
    pub(crate) fn file(realms: &[(&str, Vec<u8>)], replicate: Option<i64>) -> Vec<u8> {
        let mut db = LuaTable::default();
        db.hash
            .push((LuaValue::str("__dbversion"), LuaValue::Int(8)));
        for (name, cbor) in realms {
            db.hash.push((LuaValue::str(name), LuaValue::str(cbor)));
        }
        let mut saved = LuaTable::default();
        if let Some(at) = replicate {
            saved
                .hash
                .push((LuaValue::str("TimeOfLastReplicateScan"), LuaValue::Int(at)));
        }
        sv::write_globals(&[
            (
                "AUCTIONATOR_SAVEDVARS".into(),
                LuaValue::Table(Box::new(saved)),
            ),
            (
                "AUCTIONATOR_PRICE_DATABASE".into(),
                LuaValue::Table(Box::new(db)),
            ),
        ])
    }

    #[test]
    fn reads_cbor_realms() {
        let cbor = cbor_realm(&[
            (
                "2589",
                120,
                &[(2468, 140, None, 812), (2469, 125, Some(120), 640)],
            ),
            ("g:19019:180", 99_999_999, &[(2465, 99_999_999, None, 1)]),
            ("p:39", 5000, &[(2469, 5000, None, 3)]),
        ]);
        let db = decode(&file(&[("Forever", cbor)], Some(1_791_200_000))).unwrap();
        assert_eq!(db.replicate_scan_at, Some(1_791_200_000));
        assert_eq!(db.browse_scan_at, None);
        let r = &db.realms[0];
        assert_eq!(r.realm, "Forever");
        assert_eq!(r.items.len(), 3, "version skipped");
        let linen = r.items.iter().find(|i| i.key == "2589").unwrap();
        assert_eq!((linen.item_id, linen.last), (Some(2589), Some(120)));
        assert_eq!(
            linen.days,
            [
                DayPrice {
                    day: day(2468),
                    low: 140,
                    high: 140,
                    available: Some(812)
                },
                DayPrice {
                    day: day(2469),
                    low: 120,
                    high: 125,
                    available: Some(640)
                },
            ]
        );
        assert_eq!(day(2469), NaiveDate::from_ymd_opt(2026, 10, 5).unwrap());
        let gear = r.items.iter().find(|i| i.key.starts_with("g:")).unwrap();
        assert_eq!(gear.item_id, Some(19019));
        assert_eq!(
            r.items.iter().find(|i| i.key == "p:39").unwrap().item_id,
            None
        );
    }

    #[test]
    fn reads_an_older_plain_table_file() {
        let src = br#"
AUCTIONATOR_PRICE_DATABASE = {
	["__dbversion"] = 8,
	["Forever"] = {
		["version"] = 2,
		["2589"] = { ["m"] = 120, ["h"] = { ["2469"] = 125 }, ["l"] = { ["2469"] = 120 }, ["a"] = {} },
		["7413"] = { ["m"] = 0, ["h"] = {}, ["l"] = {}, ["a"] = {} },
	},
}
"#;
        let db = decode(src).unwrap();
        let items = &db.realms[0].items;
        assert_eq!(items.len(), 1, "an item with no price at all is left out");
        assert_eq!(items[0].days[0].low, 120);
        assert_eq!(items[0].days[0].available, None);
    }

    #[test]
    fn bad_parts_are_skipped_not_fatal() {
        let good = cbor_realm(&[("2589", 120, &[(2469, 125, None, 1)])]);
        let db = decode(&file(
            &[("Broken", b"\xff\x00not cbor".to_vec()), ("Forever", good)],
            None,
        ))
        .unwrap();
        assert_eq!(db.realms.len(), 1);
        assert_eq!(db.realms[0].realm, "Forever");

        assert_eq!(decode(b"SOMETHING_ELSE = {}\n"), Err(Unreadable::NoPrices));
        assert!(matches!(
            decode(b"AUCTIONATOR_PRICE_DATABASE = { [\"Fo"),
            Err(Unreadable::Parse(_))
        ));
    }

    #[test]
    fn item_ids_from_keys() {
        assert_eq!(item_id("2589"), Some(2589));
        assert_eq!(item_id("g:19019:180"), Some(19019));
        assert_eq!(item_id("gr:19019:5"), Some(19019));
        assert_eq!(item_id("p:39"), None);
        assert_eq!(item_id("version"), None);
    }
}
