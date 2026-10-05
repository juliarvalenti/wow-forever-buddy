//! The Auction House (F5): prices from Auctionator's SavedVariables, kept
//! in the db as day-by-day history, and the questions the AH screen asks of
//! them. Reading follows ingest's rules: only settled, changed files, through
//! `safe_read`, read-only on the game folder; a file that can't be read is
//! retried when it changes.
//!
//! One market: Auctionator keys prices by its realm root (on Forever,
//! "Forever"). With more than one in the file, the screens use the one seen
//! most recently (`market`).

pub mod auctionator;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::NaiveDate;
use rusqlite::{params, OptionalExtension, Transaction};
use serde::Serialize;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::fsx::read::safe_read;
use auctionator::{PriceDb, Unreadable};

const FILE_NAME: &str = "Auctionator.lua";
/// Bigger than any price database Auctionator writes (a busy realm is a few
/// MB): a file over this isn't read, so a crafted one can't make the parser
/// or the CBOR decoder allocate without bound.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// How far back "usual" looks: the median and the sightings.
const WINDOW_DAYS: u64 = 30;

/// Emitted after new prices were read.
#[derive(Debug, Clone, Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct PricesUpdated {
    /// Items with a price now.
    pub items: u32,
}

/// Every account's `Auctionator.lua` under the flavor folder, as
/// `(account, relative path, absolute path)`.
pub fn files(flavor_dir: &Path) -> Vec<(String, String, PathBuf)> {
    let accounts = flavor_dir.join("WTF").join("Account");
    let mut out: Vec<_> = std::fs::read_dir(&accounts)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let account = e.file_name().to_string_lossy().into_owned();
            let file = e.path().join("SavedVariables").join(FILE_NAME);
            file.is_file().then(|| {
                let rel = format!("WTF/Account/{account}/SavedVariables/{FILE_NAME}");
                (account, rel, file)
            })
        })
        .collect();
    out.sort();
    out
}

/// `ingest_state` key, apart from the characters' files (whose keys start
/// with the flavor, and are listed as the Dashboard's ingest problems).
fn state_key(flavor: &str, rel: &str) -> String {
    format!("ah:{flavor}/{rel}")
}

fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64)
}

/// Reads every changed, settled `Auctionator.lua`. Returns how many items
/// now have a price if anything was read, `None` if nothing changed.
pub fn scan(db: &Db, flavor: &str, flavor_dir: &Path) -> AppResult<Option<u32>> {
    scan_within(db, flavor, flavor_dir, MAX_BYTES)
}

/// `scan`, with the size cap as a parameter (tests use a small one).
fn scan_within(db: &Db, flavor: &str, flavor_dir: &Path, max_bytes: u64) -> AppResult<Option<u32>> {
    let mut read_any = false;
    for (account, rel, abs) in files(flavor_dir) {
        let key = state_key(flavor, &rel);
        let Ok(meta) = std::fs::metadata(&abs) else {
            continue;
        };
        let (size, mtime) = (meta.len() as i64, mtime_ns(&meta));
        let settled = meta
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .is_some_and(|age| age >= crate::ingest::SETTLE);
        if !settled {
            continue;
        }
        let seen: Option<(i64, i64)> = db.with_conn(|c| {
            Ok(c.query_row(
                "SELECT size, mtime_ns FROM ingest_state WHERE path = ?1",
                [&key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
        })?;
        if seen == Some((size, mtime)) {
            continue;
        }
        let (status, error) = if meta.len() > max_bytes {
            let mb = meta.len() >> 20;
            (
                "skipped (too large)",
                Some(format!("Auctionator.lua is {mb} MB")),
            )
        } else {
            let Ok(bytes) = safe_read(&abs) else {
                continue;
            };
            // Never a value from the file in an error: positions, versions
            // and realm names only.
            match auctionator::decode(&bytes) {
                Ok(prices) => {
                    db.with_conn(|c| {
                        let tx = c.transaction()?;
                        store(
                            &tx,
                            flavor,
                            &account,
                            &prices,
                            chrono::Utc::now().timestamp(),
                        )?;
                        tx.commit()?;
                        Ok(())
                    })?;
                    read_any = true;
                    // Realms in a format we don't read are noted, the rest kept.
                    (
                        "ok",
                        (!prices.skipped.is_empty()).then(|| prices.skipped.join("; ")),
                    )
                }
                Err(Unreadable::Parse(e)) => {
                    ("skipped (parse)", Some(format!("Auctionator.lua: {e}")))
                }
                Err(Unreadable::NoPrices) => ("skipped (no prices)", None),
                Err(Unreadable::Format(why)) => ("skipped (format)", Some(why)),
            }
        };
        db.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO ingest_state (path, size, mtime_ns, ingested_at, status, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![key, size, mtime, chrono::Utc::now().to_rfc3339(), status, error],
            )?;
            Ok(())
        })?;
    }
    if !read_any {
        return Ok(None);
    }
    let items = db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT count(*) FROM ah_latest WHERE flavor = ?1 AND item_id IS NOT NULL",
            [flavor],
            |r| r.get::<_, i64>(0),
        )?)
    })?;
    Ok(Some(items as u32))
}

/// Writes one file's prices: every day is kept (Auctionator forgets after
/// three weeks; the app doesn't), and an item's latest price only moves
/// forward in time, so an older copy of the file can't roll it back.
pub fn store(
    tx: &Transaction<'_>,
    flavor: &str,
    account: &str,
    db: &PriceDb,
    now: i64,
) -> AppResult<()> {
    let mut day_row = tx.prepare(
        "INSERT OR REPLACE INTO ah_prices (flavor, realm, item_key, item_id, day, low, high, available)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    let mut latest = tx.prepare(
        "INSERT INTO ah_latest (flavor, realm, item_key, item_id, price, day)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (flavor, realm, item_key) DO UPDATE SET
           price = excluded.price, day = excluded.day, item_id = excluded.item_id
         WHERE excluded.day >= ah_latest.day",
    )?;
    let read_day = chrono::DateTime::from_timestamp(now, 0)
        .unwrap_or_default()
        .date_naive();
    for realm in &db.realms {
        for item in &realm.items {
            for d in &item.days {
                day_row.execute(params![
                    flavor,
                    realm.realm,
                    item.key,
                    item.item_id,
                    d.day.to_string(),
                    d.low,
                    d.high,
                    d.available
                ])?;
            }
            // The last price seen, dated by its newest day.
            let (price, day) = match (item.last, item.days.last()) {
                (Some(m), Some(d)) => (m, d.day),
                (Some(m), None) => (m, read_day),
                (None, Some(d)) => (d.low, d.day),
                (None, None) => continue,
            };
            latest.execute(params![
                flavor,
                realm.realm,
                item.key,
                item.item_id,
                price,
                day.to_string()
            ])?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO ah_scans (flavor, account, replicate_at, browse_at, read_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            flavor,
            account,
            db.replicate_scan_at,
            db.browse_scan_at,
            now
        ],
    )?;
    Ok(())
}

// ---- What the screens ask ----

/// The market the screens show: the realm root with the newest price.
fn market(c: &rusqlite::Connection, flavor: &str) -> AppResult<Option<String>> {
    Ok(c.query_row(
        "SELECT realm FROM ah_latest WHERE flavor = ?1 GROUP BY realm
             ORDER BY max(day) DESC, count(*) DESC LIMIT 1",
        [flavor],
        |r| r.get(0),
    )
    .optional()?)
}

/// The scan bar: when Auctionator last scanned, and how many prices there are.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AhStatus {
    /// Whether an `Auctionator.lua` with prices has been read at all.
    pub has_prices: bool,
    /// Auctionator's realm root for the market shown ("Forever").
    pub market: Option<String>,
    /// Items with a price.
    pub items: u32,
    /// The newest of the last full or incremental scan (RFC 3339).
    pub last_scan_at: Option<String>,
    /// The newest day any price was seen (YYYY-MM-DD).
    pub newest_day: Option<String>,
}

pub fn status(db: &Db, flavor: &str) -> AppResult<AhStatus> {
    db.with_conn(|c| {
        let Some(m) = market(c, flavor)? else {
            return Ok(AhStatus {
                has_prices: false,
                market: None,
                items: 0,
                last_scan_at: None,
                newest_day: None,
            });
        };
        let (items, newest): (i64, Option<String>) = c.query_row(
            "SELECT count(*), max(day) FROM ah_latest
             WHERE flavor = ?1 AND realm = ?2 AND item_id IS NOT NULL",
            params![flavor, m],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let scan: Option<i64> = c.query_row(
            "SELECT max(max(coalesce(replicate_at, 0), coalesce(browse_at, 0)))
             FROM ah_scans WHERE flavor = ?1",
            [flavor],
            |r| r.get(0),
        )?;
        Ok(AhStatus {
            has_prices: true,
            market: Some(m),
            items: items as u32,
            last_scan_at: scan
                .filter(|s| *s > 0)
                .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                .map(|d| d.to_rfc3339()),
            newest_day: newest,
        })
    })
}

/// One item with its price, for lists (search, watchlist, worth selling).
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AhItem {
    pub item_id: u32,
    /// From the items our addon has seen; `None` for an item no character
    /// has carried (the UI says "Item 12345").
    pub name: Option<String>,
    pub quality: Option<u8>,
    /// The last lowest buyout (copper).
    pub price: f64,
    /// The day it was last seen (YYYY-MM-DD).
    pub last_seen: String,
    /// Days seen in the last 30.
    pub sightings: u32,
    /// The median of the daily lows over the last 30 days.
    pub median: Option<f64>,
    /// Those daily lows, oldest first, for a sparkline.
    pub recent: Vec<f64>,
    /// The median of how many were listed on those days ("typical listing").
    pub listed: Option<f64>,
}

fn window_start(today: NaiveDate) -> String {
    today
        .checked_sub_days(chrono::Days::new(WINDOW_DAYS))
        .unwrap_or(today)
        .to_string()
}

fn median(mut v: Vec<i64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2] as f64
    } else {
        (v[n / 2 - 1] + v[n / 2]) as f64 / 2.0
    })
}

/// The item's plain key: the whole-item price (gear is also kept per item level).
fn plain(item_id: u32) -> String {
    item_id.to_string()
}

fn item_row(
    c: &rusqlite::Connection,
    flavor: &str,
    realm: &str,
    item_id: u32,
    today: NaiveDate,
) -> AppResult<Option<AhItem>> {
    let row: Option<(i64, String, Option<String>, Option<i64>)> = c
        .query_row(
            "SELECT l.price, l.day, i.name, i.quality FROM ah_latest l
             LEFT JOIN items i ON i.item_id = l.item_id
             WHERE l.flavor = ?1 AND l.realm = ?2 AND l.item_key = ?3",
            params![flavor, realm, plain(item_id)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((price, last_seen, name, quality)) = row else {
        return Ok(None);
    };
    let mut stmt = c.prepare(
        "SELECT low, available FROM ah_prices
         WHERE flavor = ?1 AND realm = ?2 AND item_key = ?3 AND day >= ?4 ORDER BY day",
    )?;
    let days = stmt
        .query_map(
            params![flavor, realm, plain(item_id), window_start(today)],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let lows: Vec<i64> = days.iter().map(|(l, _)| *l).collect();
    Ok(Some(AhItem {
        item_id,
        name,
        quality: quality.and_then(|q| u8::try_from(q).ok()),
        price: price as f64,
        last_seen,
        sightings: lows.len() as u32,
        recent: lows.iter().map(|l| *l as f64).collect(),
        median: median(lows),
        listed: median(days.iter().filter_map(|(_, a)| *a).collect()),
    }))
}

/// Items whose name (as our addon saw it) contains `query`, or the item with
/// that id; priced ones only, best known first.
pub fn search(db: &Db, flavor: &str, query: &str, today: NaiveDate) -> AppResult<Vec<AhItem>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    db.with_conn(|c| {
        let Some(realm) = market(c, flavor)? else {
            return Ok(Vec::new());
        };
        let id: Option<i64> = q.parse().ok();
        let mut stmt = c.prepare(
            "SELECT l.item_id FROM ah_latest l LEFT JOIN items i ON i.item_id = l.item_id
             WHERE l.flavor = ?1 AND l.realm = ?2 AND l.item_key = CAST(l.item_id AS TEXT)
               AND (instr(lower(i.name), lower(?3)) > 0 OR l.item_id = ?4)
             ORDER BY i.name IS NULL, length(i.name), i.name LIMIT 20",
        )?;
        let ids = stmt
            .query_map(params![flavor, realm, q, id], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(item) = item_row(c, flavor, &realm, id as u32, today)? {
                out.push(item);
            }
        }
        Ok(out)
    })
}

/// One day on the price chart.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AhPoint {
    pub day: String,
    pub low: f64,
    pub high: f64,
    pub available: Option<f64>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AhHistory {
    pub item: AhItem,
    /// Oldest first; only days a scan saw it.
    pub points: Vec<AhPoint>,
}

/// One item's history over the last `days` (all of it when `None`).
pub fn history(
    db: &Db,
    flavor: &str,
    item_id: u32,
    days: Option<u32>,
    today: NaiveDate,
) -> AppResult<AhHistory> {
    db.with_conn(|c| {
        let realm =
            market(c, flavor)?.ok_or_else(|| AppError::NotFound("no auction prices yet".into()))?;
        let item = item_row(c, flavor, &realm, item_id, today)?
            .ok_or_else(|| AppError::NotFound(format!("no price for item {item_id}")))?;
        let from = days
            .and_then(|d| today.checked_sub_days(chrono::Days::new(u64::from(d))))
            .map(|d| d.to_string())
            .unwrap_or_default();
        let mut stmt = c.prepare(
            "SELECT day, low, high, available FROM ah_prices
             WHERE flavor = ?1 AND realm = ?2 AND item_key = ?3 AND day >= ?4 ORDER BY day",
        )?;
        let points = stmt
            .query_map(params![flavor, realm, plain(item_id), from], |r| {
                Ok(AhPoint {
                    day: r.get(0)?,
                    low: r.get::<_, i64>(1)? as f64,
                    high: r.get::<_, i64>(2)? as f64,
                    available: r.get::<_, Option<i64>>(3)?.map(|a| a as f64),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AhHistory { item, points })
    })
}

/// The watchlist, in the order items were added.
pub fn watchlist(db: &Db, flavor: &str, today: NaiveDate) -> AppResult<Vec<AhItem>> {
    db.with_conn(|c| {
        let Some(realm) = market(c, flavor)? else {
            return Ok(Vec::new());
        };
        // rowid: insertion order, which `added_at` (seconds) can't break ties in.
        let mut stmt =
            c.prepare("SELECT item_id FROM ah_watch WHERE flavor = ?1 ORDER BY added_at, rowid")?;
        let ids = stmt
            .query_map([flavor], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(item) = item_row(c, flavor, &realm, id as u32, today)? {
                out.push(item);
            }
        }
        Ok(out)
    })
}

/// Adds or removes an item from the watchlist.
pub fn set_watched(db: &Db, flavor: &str, item_id: u32, watched: bool) -> AppResult<()> {
    db.with_conn(|c| {
        if watched {
            c.execute(
                "INSERT OR IGNORE INTO ah_watch (flavor, item_id, added_at) VALUES (?1, ?2, ?3)",
                params![flavor, item_id, chrono::Utc::now().timestamp()],
            )?;
        } else {
            c.execute(
                "DELETE FROM ah_watch WHERE flavor = ?1 AND item_id = ?2",
                params![flavor, item_id],
            )?;
        }
        Ok(())
    })
}

/// How sure a price is, from how often and how lately it was seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Sure,
    Fair,
    Rough,
}

/// Where some of an item is, for "Coinpurse · bank".
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Holding {
    pub character_id: u32,
    pub character: String,
    pub class: Option<String>,
    /// `bag`, `bank` or `mail`.
    pub location: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Sellable {
    pub item: AhItem,
    pub count: u32,
    /// Most first.
    pub holdings: Vec<Holding>,
    /// count × price (copper).
    pub value: f64,
    pub confidence: Confidence,
    /// Why a price is rough: "few" (under 5 sightings) or "stale" (not seen
    /// for over a week).
    pub caution: Option<String>,
}

fn confidence(item: &AhItem, today: NaiveDate) -> (Confidence, Option<String>) {
    let age = NaiveDate::parse_from_str(&item.last_seen, "%Y-%m-%d")
        .map(|d| (today - d).num_days())
        .unwrap_or(i64::MAX);
    if item.sightings < 5 {
        (Confidence::Rough, Some("few".into()))
    } else if age > 7 {
        (Confidence::Rough, Some("stale".into()))
    } else if item.sightings >= 8 && age <= 3 {
        (Confidence::Sure, None)
    } else {
        (Confidence::Fair, None)
    }
}

/// What the flavor's characters carry (bags, bank, mail; not gear) that's
/// worth at least `min_value` copper at the last lowest buyout, most first.
/// Soulbound things never reach the AH, so they have no price and drop out.
pub fn worth_selling(
    db: &Db,
    flavor: &str,
    min_value: f64,
    today: NaiveDate,
) -> AppResult<Vec<Sellable>> {
    db.with_conn(|c| {
        let Some(realm) = market(c, flavor)? else {
            return Ok(Vec::new());
        };
        let mut stmt = c.prepare(
            "SELECT i.item_id, c.id, c.name, c.class, i.location, sum(i.count)
             FROM char_items i JOIN characters c ON c.id = i.character_id
             JOIN ah_latest l ON l.flavor = c.flavor AND l.realm = ?2 AND l.item_key = CAST(i.item_id AS TEXT)
             WHERE c.flavor = ?1 AND i.location != 'equipped'
             GROUP BY i.item_id, c.id, i.location",
        )?;
        let rows = stmt
            .query_map(params![flavor, realm], |r| {
                Ok((
                    r.get::<_, i64>(0)? as u32,
                    Holding {
                        character_id: r.get::<_, i64>(1)? as u32,
                        character: r.get(2)?,
                        class: r.get::<_, Option<String>>(3)?.map(|c| c.to_lowercase()),
                        location: r.get(4)?,
                        count: r.get::<_, i64>(5)? as u32,
                    },
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut by_item: std::collections::BTreeMap<u32, Vec<Holding>> = Default::default();
        for (id, h) in rows {
            by_item.entry(id).or_default().push(h);
        }
        let mut out = Vec::new();
        for (id, mut holdings) in by_item {
            let Some(item) = item_row(c, flavor, &realm, id, today)? else {
                continue;
            };
            let count: u32 = holdings.iter().map(|h| h.count).sum();
            let value = item.price * f64::from(count);
            if value < min_value {
                continue;
            }
            holdings.sort_by(|a, b| b.count.cmp(&a.count).then(a.character.cmp(&b.character)));
            let (confidence, caution) = confidence(&item, today);
            out.push(Sellable { item, count, holdings, value, confidence, caution });
        }
        out.sort_by(|a, b| b.value.total_cmp(&a.value));
        Ok(out)
    })
}

/// One of the most valuable holdings, for the Ledger's net worth panel.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Holdings {
    pub item: AhItem,
    pub count: u32,
    pub value: f64,
}

/// What the goods the alts carry are worth (F5c): bags, bank and mail, not
/// gear, at the last lowest buyout. Only priced items count; the rest is
/// counted apart, so a net worth never pretends to be complete.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct GoodsWorth {
    /// Copper, across every character.
    pub value: f64,
    /// Items carried, and how many of them have a price.
    pub items: u32,
    pub priced: u32,
    /// Per character, `(character id, copper)`, for "Worth carried".
    pub by_character: Vec<(u32, f64)>,
    /// The most valuable holdings, most first (up to five).
    pub top: Vec<Holdings>,
    /// The newest scan the prices come from (RFC 3339).
    pub as_of: Option<String>,
}

pub fn goods_worth(db: &Db, flavor: &str, today: NaiveDate) -> AppResult<GoodsWorth> {
    db.with_conn(|c| {
        let realm = market(c, flavor)?;
        let mut stmt = c.prepare(
            "SELECT i.item_id, c.id, sum(i.count), l.price
             FROM char_items i JOIN characters c ON c.id = i.character_id
             LEFT JOIN ah_latest l ON l.flavor = c.flavor AND l.realm = ?2
                                  AND l.item_key = CAST(i.item_id AS TEXT)
             WHERE c.flavor = ?1 AND i.location != 'equipped'
             GROUP BY i.item_id, c.id",
        )?;
        let rows = stmt
            .query_map(params![flavor, realm], |r| {
                Ok((
                    r.get::<_, i64>(0)? as u32,
                    r.get::<_, i64>(1)? as u32,
                    r.get::<_, i64>(2)? as u32,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut worth = GoodsWorth {
            value: 0.0,
            items: 0,
            priced: 0,
            by_character: Vec::new(),
            top: Vec::new(),
            as_of: None,
        };
        let mut per_char: std::collections::BTreeMap<u32, f64> = Default::default();
        let mut per_item: std::collections::BTreeMap<u32, (u32, f64)> = Default::default();
        for (item, character, count, price) in rows {
            worth.items += count;
            let Some(price) = price else { continue };
            let value = price as f64 * f64::from(count);
            worth.priced += count;
            worth.value += value;
            *per_char.entry(character).or_default() += value;
            let e = per_item.entry(item).or_default();
            e.0 += count;
            e.1 += value;
        }
        worth.by_character = per_char.into_iter().collect();
        let mut top: Vec<(u32, u32, f64)> = per_item.into_iter().map(|(id, (n, v))| (id, n, v)).collect();
        top.sort_by(|a, b| b.2.total_cmp(&a.2));
        if let Some(realm) = &realm {
            for (id, count, value) in top.into_iter().take(5) {
                if let Some(item) = item_row(c, flavor, realm, id, today)? {
                    worth.top.push(Holdings { item, count, value });
                }
            }
        }
        let scan: Option<i64> = c.query_row(
            "SELECT max(max(coalesce(replicate_at, 0), coalesce(browse_at, 0))) FROM ah_scans WHERE flavor = ?1",
            [flavor],
            |r| r.get(0),
        )?;
        worth.as_of = scan
            .filter(|s| *s > 0)
            .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
            .map(|d| d.to_rfc3339());
        Ok(worth)
    })
}

/// The last lowest buyout of each of `item_ids` that has one (the session
/// recap's "≈ worth" cells), as `(item id, copper)`.
pub fn prices(db: &Db, flavor: &str, item_ids: &[u32]) -> AppResult<Vec<(u32, f64)>> {
    db.with_conn(|c| {
        let Some(realm) = market(c, flavor)? else {
            return Ok(Vec::new());
        };
        let mut stmt = c.prepare(
            "SELECT price FROM ah_latest WHERE flavor = ?1 AND realm = ?2 AND item_key = ?3",
        )?;
        let mut out = Vec::new();
        for id in item_ids {
            if let Some(p) = stmt
                .query_row(params![flavor, realm, plain(*id)], |r| r.get::<_, i64>(0))
                .optional()?
            {
                out.push((*id, p as f64));
            }
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::auctionator::tests::{cbor_realm, file, Day};
    use super::*;

    const FLAVOR: &str = "_classic_beta_";

    fn today() -> NaiveDate {
        auctionator::day_zero() + chrono::Days::new(2469)
    }

    fn load(db: &Db, bytes: &[u8]) {
        let prices = auctionator::decode(bytes).unwrap();
        db.with_conn(|c| {
            let tx = c.transaction()?;
            store(&tx, FLAVOR, "ACCOUNT1", &prices, 1_791_200_000)?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    }

    /// Arcanite seen 9 days up to today, Black Lotus twice, Runecloth
    /// 6 times but last 10 days ago.
    fn market_file() -> Vec<u8> {
        let arcanite: Vec<Day> = (2461..=2469)
            .map(|d| {
                (
                    d,
                    400_000 + (2469 - d as i64) * 1000,
                    Some(380_000 + (2469 - d as i64) * 1000),
                    80,
                )
            })
            .collect();
        let runecloth: Vec<Day> = (2454..=2459).map(|d| (d, 1200, None, 900)).collect();
        let cbor = cbor_realm(&[
            ("12360", 364_000, &arcanite),
            (
                "13468",
                820_000,
                &[(2467, 830_000, None, 2), (2469, 820_000, None, 1)],
            ),
            ("14047", 1100, &runecloth),
        ]);
        file(&[("Forever", cbor)], Some(1_791_200_000))
    }

    #[test]
    fn stores_and_reports() {
        let db = Db::open_in_memory().unwrap();
        load(&db, &market_file());

        let s = status(&db, FLAVOR).unwrap();
        assert!(s.has_prices);
        assert_eq!((s.market.as_deref(), s.items), (Some("Forever"), 3));
        assert_eq!(s.newest_day.as_deref(), Some("2026-10-05"));
        assert!(s.last_scan_at.is_some());

        let h = history(&db, FLAVOR, 12360, Some(30), today()).unwrap();
        assert_eq!(h.points.len(), 9);
        assert_eq!(h.item.price, 364_000.0, "the last minimum, not a day's");
        assert_eq!(h.item.sightings, 9);
        assert_eq!(h.item.median, Some(384_000.0));
        assert_eq!(h.item.recent.len(), 9);
        assert_eq!(h.item.recent.last(), Some(&380_000.0), "oldest first");
        assert_eq!(h.item.listed, Some(80.0));
        assert_eq!(h.points.last().unwrap().low, 380_000.0);
        assert!(matches!(
            history(&db, FLAVOR, 1, None, today()),
            Err(AppError::NotFound(_))
        ));
        assert!(!status(&db, "_classic_").unwrap().has_prices);
    }

    #[test]
    fn history_outlives_auctionators_pruning() {
        let db = Db::open_in_memory().unwrap();
        load(&db, &market_file());
        // A later file: Auctionator dropped the old days and saw one new one.
        let later = file(
            &[(
                "Forever",
                cbor_realm(&[("12360", 350_000, &[(2470, 350_000, None, 70)])]),
            )],
            Some(1_791_300_000),
        );
        load(&db, &later);
        let h = history(&db, FLAVOR, 12360, None, today() + chrono::Days::new(1)).unwrap();
        assert_eq!(h.points.len(), 10, "old days kept, the new one added");
        assert_eq!(h.item.price, 350_000.0);
        // And an older copy of the file can't roll the latest price back.
        load(&db, &market_file());
        let h = history(&db, FLAVOR, 12360, None, today() + chrono::Days::new(1)).unwrap();
        assert_eq!(h.item.price, 350_000.0);
    }

    #[test]
    fn search_and_watchlist() {
        let db = Db::open_in_memory().unwrap();
        load(&db, &market_file());
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO items (item_id, name, quality, seen_at) VALUES (12360, 'Arcanite Bar', 2, 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let found = search(&db, FLAVOR, "arcan", today()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name.as_deref(), Some("Arcanite Bar"));
        let by_id = search(&db, FLAVOR, "13468", today()).unwrap();
        assert_eq!((by_id[0].item_id, by_id[0].name.clone()), (13468, None));
        assert!(search(&db, FLAVOR, "  ", today()).unwrap().is_empty());

        set_watched(&db, FLAVOR, 13468, true).unwrap();
        set_watched(&db, FLAVOR, 12360, true).unwrap();
        set_watched(&db, FLAVOR, 12360, true).unwrap();
        let w: Vec<u32> = watchlist(&db, FLAVOR, today())
            .unwrap()
            .iter()
            .map(|i| i.item_id)
            .collect();
        assert_eq!(w, [13468, 12360], "order added, no duplicates");
        set_watched(&db, FLAVOR, 13468, false).unwrap();
        assert_eq!(watchlist(&db, FLAVOR, today()).unwrap().len(), 1);
    }

    /// The file in the game folder: read once settled, not again until it
    /// changes, and never listed among the characters' ingest problems.
    #[test]
    fn scan_reads_each_accounts_file_once() {
        let tmp = tempfile::tempdir().unwrap();
        let sv = tmp.path().join("WTF/Account/ACCOUNT1/SavedVariables");
        std::fs::create_dir_all(&sv).unwrap();
        let path = sv.join(FILE_NAME);
        let age = |p: &Path| {
            let old = SystemTime::now() - std::time::Duration::from_secs(60);
            std::fs::File::options()
                .write(true)
                .open(p)
                .unwrap()
                .set_modified(old)
                .unwrap();
        };
        std::fs::write(&path, market_file()).unwrap();
        // Still being written: left for the next scan.
        let db = Db::open_in_memory().unwrap();
        assert_eq!(scan(&db, FLAVOR, tmp.path()).unwrap(), None);

        age(&path);
        assert_eq!(scan(&db, FLAVOR, tmp.path()).unwrap(), Some(3));
        assert_eq!(scan(&db, FLAVOR, tmp.path()).unwrap(), None, "unchanged");

        // A torn file is recorded and retried when it changes, quietly.
        std::fs::write(&path, b"AUCTIONATOR_PRICE_DATABASE = { [\"Fo").unwrap();
        age(&path);
        assert_eq!(scan(&db, FLAVOR, tmp.path()).unwrap(), None);
        assert!(crate::ingest::problems(&db, FLAVOR).unwrap().is_empty());
        assert!(
            status(&db, FLAVOR).unwrap().has_prices,
            "earlier prices stay"
        );

        // Over the size cap: not read at all, and recorded as such.
        std::fs::write(&path, market_file()).unwrap();
        age(&path);
        let fresh = Db::open_in_memory().unwrap();
        assert_eq!(scan_within(&fresh, FLAVOR, tmp.path(), 100).unwrap(), None);
        let state: String = fresh
            .with_conn(|c| Ok(c.query_row("SELECT status FROM ingest_state", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(state, "skipped (too large)");
        assert!(!status(&fresh, FLAVOR).unwrap().has_prices);
    }

    #[test]
    fn worth_selling_across_alts() {
        let db = alts_with_goods();
        let s = worth_selling(&db, FLAVOR, 500_000.0, today()).unwrap();
        let ids: Vec<u32> = s.iter().map(|x| x.item.item_id).collect();
        assert_eq!(
            ids,
            [12360, 13468],
            "most valuable first, Runecloth under the bar"
        );
        assert_eq!((s[0].count, s[0].value), (24, 24.0 * 364_000.0));
        assert_eq!(s[0].holdings[0].location, "bank");
        assert_eq!(s[0].confidence, Confidence::Sure);
        assert_eq!(
            (s[1].confidence, s[1].caution.as_deref()),
            (Confidence::Rough, Some("few"))
        );
        assert_eq!(s[1].holdings[0].class.as_deref(), Some("warrior"));
    }

    /// F5c: net worth's goods, only what has a price.
    #[test]
    fn goods_worth_counts_priced_items_only() {
        let db = alts_with_goods();
        let w = goods_worth(&db, FLAVOR, today()).unwrap();
        // Arcanite 24 × 36g40s + Lotus 82g + Runecloth 40 × 11s; the
        // unpriced Hearthstone is counted but not valued; gear never is.
        assert_eq!(w.value, 24.0 * 364_000.0 + 820_000.0 + 40.0 * 1100.0);
        assert_eq!((w.items, w.priced), (24 + 1 + 40 + 1, 24 + 1 + 40));
        assert_eq!(
            w.by_character,
            [(1, 24.0 * 364_000.0), (2, 820_000.0 + 44_000.0)]
        );
        assert_eq!(w.top[0].item.item_id, 12360);
        assert!(w.as_of.is_some());
        assert_eq!(
            prices(&db, FLAVOR, &[12360, 6948]).unwrap(),
            [(12360, 364_000.0)]
        );
        assert!(goods_worth(&db, "_classic_", today())
            .unwrap()
            .top
            .is_empty());
    }

    /// Coinpurse and Velyra with some goods, against `market_file`'s prices.
    fn alts_with_goods() -> Db {
        let db = Db::open_in_memory().unwrap();
        load(&db, &market_file());
        db.with_conn(|c| {
            for (id, name) in [(1, "Coinpurse"), (2, "Velyra")] {
                c.execute(
                    "INSERT INTO characters (id, flavor, account, group_dir, char_dir, name, class, first_seen, last_seen)
                     VALUES (?1, ?2, 'A', '70', ?3, ?3, 'WARRIOR', 0, 0)",
                    params![id, FLAVOR, name],
                )?;
            }
            let item = |who: i64, loc: &str, slot: i64, id: i64, n: i64| {
                c.execute(
                    "INSERT INTO char_items (character_id, location, container, slot, item_id, link, count, as_of)
                     VALUES (?1, ?2, 0, ?3, ?4, '', ?5, 0)",
                    params![who, loc, slot, id, n],
                )
            };
            item(1, "bank", 1, 12360, 20)?; // Arcanite ×24 across two places
            item(1, "bag", 2, 12360, 4)?;
            item(2, "bank", 1, 13468, 1)?; // a Black Lotus
            item(2, "bag", 1, 14047, 40)?; // Runecloth: 44s, under the bar
            item(2, "bag", 2, 6948, 1)?; // a Hearthstone: never on the AH
            item(2, "equipped", 1, 12360, 1)?; // gear never counts
            Ok(())
        })
        .unwrap();
        db
    }
}
