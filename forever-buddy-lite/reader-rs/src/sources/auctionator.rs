//! Auctionator.lua: the price database Auctionator keeps from its scans.
//!
//! `AUCTIONATOR_PRICE_DATABASE` holds one entry per realm. Each is a CBOR blob inside a Lua string
//! (database version 8, realm format version 2). Per item it keeps:
//! - `m`: the cheapest buyout in the latest scan;
//! - `h`: for each day, the cheapest price at the day's *highest* scan (the floor most of the day);
//! - `l`: for each day, the cheapest price at the day's *lowest* scan (catches a stray cheap listing);
//! - `a`: for each day, how many were up.
//!
//! Days count from 2020-01-01.

use super::Env;
use crate::{cbor, lua};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Auctionator's database layout this reader understands.
const DB_VERSION: f64 = 8.0;
const REALM_VERSION: f64 = 2.0;

#[derive(Debug, Default)]
pub struct PriceDb {
    pub files: Vec<PathBuf>,
    pub realms: Vec<RealmPrices>,
    /// When the last full scan finished (seconds since 1970): browse or replicate, whichever is newer.
    pub scan_at: Option<f64>,
}

#[derive(Debug, Default)]
pub struct RealmPrices {
    pub realm: String,
    /// Items in id order.
    pub items: Vec<PriceEntry>,
    index: HashMap<String, usize>,
}

/// One item's prices on one realm.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PriceEntry {
    /// The database key: the item id ("2589"), or another form for pets and such.
    pub key: String,
    /// Cheapest buyout in the latest scan, copper.
    pub now: Option<f64>,
    /// Whether it has a daily history at all (`h`).
    pub has_history: bool,
    /// Daily prices, oldest first.
    pub days: Vec<DayPrice>,
}

/// One day of an item's prices.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DayPrice {
    /// Days since 2020-01-01.
    pub day: i64,
    /// The cheapest price at the day's highest scan: what the floor mostly was.
    pub high: f64,
    /// The cheapest price at the day's lowest scan, when recorded.
    pub low: Option<f64>,
    /// How many were up, when recorded.
    pub listed: Option<f64>,
}

impl DayPrice {
    /// The day's cheapest: the low when recorded, else the high (which then was the only price).
    pub fn cheapest(&self) -> f64 {
        self.low.unwrap_or(self.high)
    }
}

impl PriceEntry {
    /// The item id, when the key is one.
    pub fn id(&self) -> Option<u32> {
        self.key.parse().ok()
    }
}

impl RealmPrices {
    pub fn get(&self, key: &str) -> Option<&PriceEntry> {
        self.index.get(key).map(|&i| &self.items[i])
    }
}

/// Reads every Auctionator file `env` points at. Realms that aren't in the expected format are skipped.
pub fn load(env: &Env) -> Result<PriceDb, String> {
    let mut db = PriceDb { files: env.auctionator_files(), ..PriceDb::default() };
    for file in db.files.clone() {
        let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let globals = lua::parse_saved_variables(&bytes).map_err(|e| e.to_string())?;
        let global = |name: &str| globals.iter().find(|(n, _)| n == name).map(|(_, v)| v);
        let Some(lua::Value::Table(prices)) = global("AUCTIONATOR_PRICE_DATABASE") else { continue };
        if number(prices.get("__dbversion")) != Some(DB_VERSION) {
            continue;
        }
        if let Some(lua::Value::Table(saved)) = global("AUCTIONATOR_SAVEDVARS") {
            let latest = [db.scan_at, number(saved.get("TimeOfLastBrowseScan")), number(saved.get("TimeOfLastReplicateScan"))]
                .into_iter()
                .flatten()
                .fold(0.0, f64::max);
            if latest > 0.0 {
                db.scan_at = Some(latest);
            }
        }
        for (key, value) in &prices.entries {
            let lua::Key::Str(name) = key else { continue };
            let name = String::from_utf8_lossy(name).into_owned();
            if name.starts_with("__") {
                continue;
            }
            if let Some(realm) = realm_from_lua(&name, value) {
                // The same realm in a later file replaces the earlier one.
                match db.realms.iter_mut().find(|r| r.realm == name) {
                    Some(old) => *old = realm,
                    None => db.realms.push(realm),
                }
            }
        }
    }
    Ok(db)
}

fn number(v: Option<&lua::Value>) -> Option<f64> {
    match v {
        Some(lua::Value::Number(n)) => Some(*n),
        _ => None,
    }
}

/// A realm's entry: normally a CBOR blob in a string; a plain table works too.
fn realm_from_lua(name: &str, value: &lua::Value) -> Option<RealmPrices> {
    let data: Value = match value {
        lua::Value::Str(bytes) => cbor::decode(bytes).ok()?.to_lua_json(),
        lua::Value::Table(t) => t.to_json(),
        _ => return None,
    };
    if data.get("version").and_then(Value::as_f64) != Some(REALM_VERSION) {
        return None;
    }
    Some(realm_from_json(name, &data))
}

/// Builds a realm's typed prices from its decoded data.
pub fn realm_from_json(name: &str, data: &Value) -> RealmPrices {
    let mut realm = RealmPrices { realm: name.to_string(), ..RealmPrices::default() };
    for (key, entry) in crate::json::entries(Some(data)) {
        if key == "version" || !entry.is_object() {
            continue;
        }
        realm.index.insert(key.clone(), realm.items.len());
        realm.items.push(entry_from_json(key, entry));
    }
    realm
}

fn entry_from_json(key: String, e: &Value) -> PriceEntry {
    let per_day = |field: &str, day: &str| e.get(field).and_then(|m| m.get(day)).and_then(Value::as_f64);
    let history = e.get("h").filter(|h| crate::json::truthy(h));
    let mut days: Vec<DayPrice> = crate::json::entries(history)
        .into_iter()
        .filter_map(|(day_key, high)| {
            Some(DayPrice { day: day_key.parse().ok()?, high: high.as_f64()?, low: per_day("l", &day_key), listed: per_day("a", &day_key) })
        })
        .collect();
    days.sort_by_key(|d| d.day);
    PriceEntry { key, now: e.get("m").and_then(Value::as_f64), has_history: history.is_some(), days }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typed_entries() {
        let realm = realm_from_json(
            "R",
            &json!({"version": 2, "25": {"m": 369, "h": {"2510": 569, "2509": 1069}, "l": {"2509": 569}, "a": {"2509": 7}}, "26": {"m": 5}}),
        );
        let e = realm.get("25").unwrap();
        assert_eq!(e.id(), Some(25));
        assert_eq!(e.now, Some(369.0));
        assert_eq!(e.days.iter().map(|d| d.day).collect::<Vec<_>>(), [2509, 2510]); // oldest first
        assert_eq!(e.days[0], DayPrice { day: 2509, high: 1069.0, low: Some(569.0), listed: Some(7.0) });
        assert_eq!(e.days[1].cheapest(), 569.0); // no low that day: the high
        assert!(!realm.get("26").unwrap().has_history);
        assert!(realm.get("version").is_none());
    }
}
