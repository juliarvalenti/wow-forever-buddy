//! The Characters screen and the character sheet (v0.2 V7): what ingest put
//! in the db, shaped for the UI. Read-only, except the user's bank-alt tag (F3).
//!
//! Money and durations are f64 and times RFC 3339 strings, because specta
//! won't send i64 to TypeScript. Item names come from the `items` table when
//! the addon saw the item, else from the link text. Everything here is game
//! text: the UI renders it as text, never as HTML.

use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use crate::db::Db;
use crate::error::{AppError, AppResult};

/// One card on the Characters screen.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CharacterCard {
    pub id: u32,
    /// The folder identity (WTF/Account/<account>/<group>/<folder>), to
    /// match the WTF roster's characters the addon hasn't seen yet.
    pub account: String,
    pub group_dir: String,
    pub folder: String,
    pub name: String,
    pub surname: Option<String>,
    /// File token, lowercase (`warrior`), for the class colour.
    pub class: Option<String>,
    pub race: Option<String>,
    pub level: Option<u32>,
    pub realm: Option<String>,
    pub guild: Option<String>,
    pub zone: Option<String>,
    pub subzone: Option<String>,
    /// The last logout we have (RFC 3339, UTC).
    pub last_seen: String,
    /// Copper.
    pub money: f64,
    pub xp: Option<f64>,
    pub xp_max: Option<f64>,
    pub rested: Option<f64>,
    pub ilvl: Option<f64>,
    /// Seconds.
    pub played: Option<f64>,
    /// Free slots across all bags, if the bags have been seen.
    pub bag_free: Option<u32>,
    pub bag_size: Option<u32>,
    pub mail: u32,
    pub bank_items: u32,
    /// Marked by the user as a bank alt (F3): a "Bank" tag on the card.
    pub bank_alt: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CharactersOverview {
    /// Copper, across every character.
    pub gold: f64,
    /// Items carried, across bags, bank and mail.
    pub items: u32,
    pub characters: Vec<CharacterCard>,
}

/// One item in a slot, ready to show.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ItemRow {
    pub container: i32,
    pub slot: u32,
    pub item_id: u32,
    pub name: String,
    /// 0 poor … 5 legendary, when known.
    pub quality: Option<u8>,
    pub ilvl: Option<u32>,
    pub count: u32,
    /// When this character last looted one (RFC 3339), if the journal has
    /// it, and the zone it was in. Never a source (IMPLEMENTING.md §7).
    pub looted_at: Option<String>,
    pub looted_in: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct BagView {
    pub container: i32,
    pub name: Option<String>,
    pub size: Option<u32>,
    pub free: Option<u32>,
    pub items: Vec<ItemRow>,
}

/// A location the addon only sees when it's opened (bank, mailbox).
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Visited {
    /// When it was last seen (RFC 3339), `None` if never.
    pub as_of: Option<String>,
    pub bags: Vec<BagView>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct MailRow {
    /// Other players' text from your own mailbox: shown here, never exported.
    pub sender: Option<String>,
    pub subject: Option<String>,
    pub money: f64,
    pub cod: f64,
    pub days_left: Option<f64>,
    pub items: Vec<ItemRow>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct MailView {
    pub as_of: Option<String>,
    pub messages: Vec<MailRow>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ProfessionRow {
    pub name: String,
    pub skill: Option<u32>,
    pub max: Option<u32>,
}

/// A raid or dungeon save that hasn't reset yet (F3).
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Lockout {
    /// The instance, as the game names it ("Molten Core").
    pub name: String,
    /// "Normal", "Heroic", … as the game gives it; may be empty.
    pub difficulty: String,
    pub raid: bool,
    /// When the save resets (RFC 3339), if the game said.
    pub reset_at: Option<String>,
}

/// A save on any character, for the Dashboard's "Lockouts this week".
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AltLockout {
    pub character_id: u32,
    pub character: String,
    /// File token, lowercase, for the class colour.
    pub class: Option<String>,
    pub lockout: Lockout,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct GoldPoint {
    pub at: String,
    pub money: f64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CharacterSheet {
    pub card: CharacterCard,
    pub equipped: Vec<ItemRow>,
    pub bags: Vec<BagView>,
    pub bank: Visited,
    pub mail: MailView,
    pub professions: Vec<ProfessionRow>,
    /// Saves that haven't reset, soonest reset first.
    pub lockouts: Vec<Lockout>,
    /// The login the saves were read at (RFC 3339); `None` if they never
    /// have been, as opposed to read and none found.
    pub lockouts_as_of: Option<String>,
    /// The last 30 days of gold, oldest first.
    pub gold_30d: Vec<GoldPoint>,
}

fn iso(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .unwrap_or_default()
        .to_rfc3339()
}

fn opt_u32(v: Option<i64>) -> Option<u32> {
    v.and_then(|v| u32::try_from(v).ok())
}

/// The bracketed name in an item link (`|h[Linen Cloth]|h`).
fn link_name(link: &str) -> Option<String> {
    let start = link.find("|h[")? + 3;
    let end = start + link[start..].find("]|h")?;
    Some(link[start..end].to_string())
}

/// Quality from the link's colour code, for items without static info.
fn link_quality(link: &str) -> Option<u8> {
    let hex = link.strip_prefix("|cff")?.get(..6)?.to_ascii_lowercase();
    Some(match hex.as_str() {
        "9d9d9d" => 0,
        "ffffff" => 1,
        "1eff00" => 2,
        "0070dd" => 3,
        "a335ee" => 4,
        "ff8000" => 5,
        _ => return None,
    })
}

const CARD_SQL: &str = "
    SELECT c.id, c.name, c.surname, c.class, c.race, c.level, c.realm, c.guild, c.last_seen,
           s.zone, s.subzone, s.money, s.xp, s.xp_max, s.rested, s.ilvl_equipped, s.played_total,
           (SELECT sum(free) FROM char_bags b WHERE b.character_id = c.id AND b.location = 'bag'),
           (SELECT sum(size) FROM char_bags b WHERE b.character_id = c.id AND b.location = 'bag'),
           (SELECT count(*) FROM char_mail m WHERE m.character_id = c.id),
           (SELECT coalesce(sum(count), 0) FROM char_items i
             WHERE i.character_id = c.id AND i.location = 'bank'),
           c.account, c.group_dir, c.char_dir, c.bank_alt
    FROM characters c
    LEFT JOIN char_snapshots s
      ON s.character_id = c.id
     AND s.at = (SELECT max(at) FROM char_snapshots WHERE character_id = c.id)";

fn card(r: &Row<'_>) -> rusqlite::Result<CharacterCard> {
    let f = |v: Option<i64>| v.map(|v| v as f64);
    Ok(CharacterCard {
        id: r.get::<_, i64>(0)? as u32,
        account: r.get(21)?,
        group_dir: r.get(22)?,
        folder: r.get(23)?,
        name: r.get(1)?,
        surname: r.get(2)?,
        class: r.get::<_, Option<String>>(3)?.map(|c| c.to_lowercase()),
        race: r.get(4)?,
        level: opt_u32(r.get(5)?),
        realm: r.get(6)?,
        guild: r.get(7)?,
        last_seen: iso(r.get(8)?),
        zone: r.get(9)?,
        subzone: r.get(10)?,
        money: r.get::<_, Option<i64>>(11)?.unwrap_or(0) as f64,
        xp: f(r.get(12)?),
        xp_max: f(r.get(13)?),
        rested: f(r.get(14)?),
        ilvl: r.get(15)?,
        played: f(r.get(16)?),
        bag_free: opt_u32(r.get(17)?),
        bag_size: opt_u32(r.get(18)?),
        mail: r.get::<_, i64>(19)? as u32,
        bank_items: r.get::<_, i64>(20)? as u32,
        bank_alt: r.get::<_, i64>(24)? != 0,
    })
}

/// Saves still in force at `now`: a reset time in the future, or no reset
/// time but seen within the last week (a weekly save can't outlast that).
const LOCKOUT_LIVE: &str = "(l.reset_at > ?2 OR (l.reset_at IS NULL AND l.as_of > ?2 - 7 * 86400))";

fn lockout(r: &Row<'_>, at: usize) -> rusqlite::Result<Lockout> {
    Ok(Lockout {
        name: r.get(at)?,
        difficulty: r.get(at + 1)?,
        raid: r.get::<_, i64>(at + 2)? != 0,
        reset_at: r.get::<_, Option<i64>>(at + 3)?.map(iso),
    })
}

/// Every live save across `flavor`'s characters, soonest reset first.
pub fn lockouts(db: &Db, flavor: &str, now: i64) -> AppResult<Vec<AltLockout>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT c.id, c.name, c.class, l.name, l.difficulty, l.raid, l.reset_at
             FROM lockouts l JOIN characters c ON c.id = l.character_id
             WHERE c.flavor = ?1 AND {LOCKOUT_LIVE}
             ORDER BY l.reset_at IS NULL, l.reset_at, l.name, c.name"
        ))?;
        let rows = stmt
            .query_map(params![flavor, now], |r| {
                Ok(AltLockout {
                    character_id: r.get::<_, i64>(0)? as u32,
                    character: r.get(1)?,
                    class: r.get::<_, Option<String>>(2)?.map(|c| c.to_lowercase()),
                    lockout: lockout(r, 3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Marks or unmarks a character as a bank alt (F3). Only this column: the
/// addon's data is untouched, and ingest never writes it.
pub fn set_bank_alt(db: &Db, id: u32, bank_alt: bool) -> AppResult<()> {
    db.with_conn(|c| {
        let n = c.execute(
            "UPDATE characters SET bank_alt = ?2 WHERE id = ?1",
            params![i64::from(id), i64::from(bank_alt)],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("character {id}")));
        }
        Ok(())
    })
}

/// Every character of `flavor`, most recently seen first (the UI re-sorts).
pub fn overview(db: &Db, flavor: &str) -> AppResult<CharactersOverview> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(&format!(
            "{CARD_SQL} WHERE c.flavor = ?1 ORDER BY c.last_seen DESC"
        ))?;
        let characters = stmt
            .query_map([flavor], card)?
            .collect::<Result<Vec<_>, _>>()?;
        let items: i64 = c.query_row(
            "SELECT coalesce(sum(i.count), 0) FROM char_items i
             JOIN characters c ON c.id = i.character_id
             WHERE c.flavor = ?1 AND i.location != 'equipped'",
            [flavor],
            |r| r.get(0),
        )?;
        Ok(CharactersOverview {
            gold: characters.iter().map(|c| c.money).sum(),
            items: items as u32,
            characters,
        })
    })
}

fn items_at(c: &rusqlite::Connection, id: i64, location: &str) -> AppResult<Vec<ItemRow>> {
    // `loot`: the latest time this character picked each item up (a `gain`
    // with no `how`: not bought, not from mail), and the zone it was in.
    let mut stmt = c.prepare(
        "WITH loot AS (
           SELECT json_extract(e.data, '$.item') AS item_id, e.at,
                  (SELECT json_extract(z.data, '$.zone') FROM adventure_events z
                    WHERE z.adventure_id = e.adventure_id AND z.kind = 'zone' AND z.at <= e.at
                    ORDER BY z.at DESC, z.seq DESC LIMIT 1) AS zone,
                  row_number() OVER (PARTITION BY json_extract(e.data, '$.item')
                                     ORDER BY e.at DESC, e.seq DESC) AS n
           FROM adventure_events e JOIN adventures a ON a.id = e.adventure_id
           WHERE a.character_id = ?1 AND e.kind = 'gain'
             AND json_extract(e.data, '$.how') IS NULL
         )
         SELECT i.container, i.slot, i.item_id, i.link, i.count, it.name, it.quality, it.ilvl,
                l.at, l.zone
         FROM char_items i
         LEFT JOIN items it ON it.item_id = i.item_id
         LEFT JOIN loot l ON l.item_id = i.item_id AND l.n = 1
         WHERE i.character_id = ?1 AND i.location = ?2
         ORDER BY i.container, i.slot",
    )?;
    let rows = stmt
        .query_map(params![id, location], |r| {
            let link: String = r.get(3)?;
            let name: Option<String> = r.get(5)?;
            let quality: Option<i64> = r.get(6)?;
            Ok(ItemRow {
                container: r.get::<_, i64>(0)? as i32,
                slot: r.get::<_, i64>(1)? as u32,
                item_id: r.get::<_, i64>(2)? as u32,
                name: name
                    .or_else(|| link_name(&link))
                    .unwrap_or_else(|| "Unknown item".into()),
                quality: quality
                    .and_then(|q| u8::try_from(q).ok())
                    .or_else(|| link_quality(&link)),
                ilvl: opt_u32(r.get(7)?),
                count: r.get::<_, i64>(4)? as u32,
                looted_at: r.get::<_, Option<i64>>(8)?.map(iso),
                looted_in: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Bags (or bank tabs) with their items grouped in.
fn bags_at(c: &rusqlite::Connection, id: i64, location: &str) -> AppResult<Vec<BagView>> {
    let items = items_at(c, id, location)?;
    let mut stmt = c.prepare(
        "SELECT container, name, size, free FROM char_bags
         WHERE character_id = ?1 AND location = ?2 ORDER BY container",
    )?;
    let mut bags: Vec<BagView> = stmt
        .query_map(params![id, location], |r| {
            Ok(BagView {
                container: r.get::<_, i64>(0)? as i32,
                name: r.get(1)?,
                size: opt_u32(r.get(2)?),
                free: opt_u32(r.get(3)?),
                items: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for item in items {
        match bags.iter_mut().find(|b| b.container == item.container) {
            Some(bag) => bag.items.push(item),
            // Items whose bag wasn't described still show, in their own group.
            None => bags.push(BagView {
                container: item.container,
                name: None,
                size: None,
                free: None,
                items: vec![item],
            }),
        }
    }
    bags.sort_by_key(|b| b.container);
    Ok(bags)
}

fn as_of(c: &rusqlite::Connection, id: i64, location: &str) -> AppResult<Option<String>> {
    let at: Option<i64> = c.query_row(
        "SELECT max(as_of) FROM char_items WHERE character_id = ?1 AND location = ?2",
        params![id, location],
        |r| r.get(0),
    )?;
    Ok(at.map(iso))
}

/// Everything on one character's sheet.
pub fn sheet(db: &Db, id: u32) -> AppResult<CharacterSheet> {
    let id = i64::from(id);
    db.with_conn(|c| {
        let card = c
            .query_row(&format!("{CARD_SQL} WHERE c.id = ?1"), [id], card)
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("character {id}")))?;
        let bank_bags = bags_at(c, id, "bank")?;
        let mail_items = items_at(c, id, "mail")?;
        let mut stmt = c.prepare(
            "SELECT idx, sender, subject, money, cod, days_left FROM char_mail
             WHERE character_id = ?1 ORDER BY idx",
        )?;
        let messages = stmt
            .query_map([id], |r| {
                let idx: i64 = r.get(0)?;
                Ok(MailRow {
                    sender: r.get(1)?,
                    subject: r.get(2)?,
                    money: r.get::<_, i64>(3)? as f64,
                    cod: r.get::<_, i64>(4)? as f64,
                    days_left: r.get(5)?,
                    items: mail_items
                        .iter()
                        .filter(|i| i64::from(i.container) == idx)
                        .cloned()
                        .collect(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = c.prepare(
            "SELECT name, skill, max FROM professions WHERE character_id = ?1
             ORDER BY coalesce(skill, 0) DESC, name",
        )?;
        let professions = stmt
            .query_map([id], |r| {
                Ok(ProfessionRow {
                    name: r.get(0)?,
                    skill: opt_u32(r.get(1)?),
                    max: opt_u32(r.get(2)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let now = chrono::Utc::now().timestamp();
        let mut stmt = c.prepare(&format!(
            "SELECT l.name, l.difficulty, l.raid, l.reset_at FROM lockouts l
             WHERE l.character_id = ?1 AND {LOCKOUT_LIVE}
             ORDER BY l.reset_at IS NULL, l.reset_at, l.name"
        ))?;
        let lockouts = stmt
            .query_map(params![id, now], |r| lockout(r, 0))?
            .collect::<Result<Vec<_>, _>>()?;
        // The addon reads saves at every login and puts them in the
        // snapshot, so the newest snapshot dates them (IMPLEMENTING.md §8).
        let lockouts_as_of: Option<i64> = c.query_row(
            "SELECT max(at) FROM char_snapshots WHERE character_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        let since = now - 30 * 86_400;
        let mut stmt = c.prepare(
            "SELECT at, money FROM gold_points WHERE character_id = ?1 AND at >= ?2 ORDER BY at",
        )?;
        let gold_30d = stmt
            .query_map(params![id, since], |r| {
                Ok(GoldPoint {
                    at: iso(r.get(0)?),
                    money: r.get::<_, i64>(1)? as f64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CharacterSheet {
            equipped: items_at(c, id, "equipped")?,
            bags: bags_at(c, id, "bag")?,
            bank: Visited {
                as_of: as_of(c, id, "bank")?,
                bags: bank_bags,
            },
            mail: MailView {
                as_of: as_of(c, id, "mail")?,
                messages,
            },
            professions,
            lockouts,
            lockouts_as_of: lockouts_as_of.map(iso),
            gold_30d,
            card,
        })
    })
}

/// One item stack total on one character, in one place.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SearchHit {
    pub character_id: u32,
    /// "Velyra Duskmane".
    pub character: String,
    pub class: Option<String>,
    /// `bag`, `bank` or `mail`.
    pub location: String,
    pub item_id: u32,
    pub name: String,
    pub quality: Option<u8>,
    pub ilvl: Option<u32>,
    /// Summed over every stack of it in that place.
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SearchResults {
    /// Most first, at most `SEARCH_MAX`.
    pub hits: Vec<SearchHit>,
    /// Items matched, across every hit (shown or not).
    pub total: u32,
    /// Characters with a match, to light their cards.
    pub characters: Vec<u32>,
    /// More hits than `hits` holds.
    pub more: bool,
}

/// The most rows the results table shows.
pub const SEARCH_MAX: usize = 200;

/// What a search box query asks for: words that must all be in the item's
/// name (any case), and item level filters like `ilvl>60` (the mock's
/// placeholder): `>`, `>=`, `<`, `<=` or `=`.
#[derive(Debug, Default, PartialEq)]
struct Query {
    words: Vec<String>,
    ilvl: Vec<(Cmp, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cmp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
}

impl Cmp {
    fn holds(self, a: u32, b: u32) -> bool {
        match self {
            Cmp::Gt => a > b,
            Cmp::Ge => a >= b,
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
            Cmp::Eq => a == b,
        }
    }
}

impl Query {
    fn parse(text: &str) -> Query {
        // Longest operators first, so ">=" isn't read as ">" then "=5".
        const OPS: [(&str, Cmp); 5] = [
            (">=", Cmp::Ge),
            ("<=", Cmp::Le),
            (">", Cmp::Gt),
            ("<", Cmp::Lt),
            ("=", Cmp::Eq),
        ];
        let mut q = Query::default();
        for token in text.split_whitespace() {
            let lower = token.to_lowercase();
            let filter = lower.strip_prefix("ilvl").and_then(|rest| {
                let (cmp, n) = OPS
                    .iter()
                    .find_map(|&(op, cmp)| rest.strip_prefix(op).map(|n| (cmp, n)))?;
                Some((cmp, n.parse::<u32>().ok()?))
            });
            match filter {
                Some(f) => q.ilvl.push(f),
                None => q.words.push(lower),
            }
        }
        q
    }

    fn is_empty(&self) -> bool {
        self.words.is_empty() && self.ilvl.is_empty()
    }

    /// An item level filter needs a known item level: an item the addon has
    /// no info for doesn't match `ilvl>60`.
    fn matches(&self, name: &str, ilvl: Option<u32>) -> bool {
        let name = name.to_lowercase();
        self.words.iter().all(|w| name.contains(w.as_str()))
            && self
                .ilvl
                .iter()
                .all(|&(cmp, n)| ilvl.is_some_and(|i| cmp.holds(i, n)))
    }
}

/// Every satchel, bank and mailbox of `flavor`'s characters, searched for
/// `text` (see `Query`). Bank and mail are as of each character's last
/// visit, like the sheet. An empty query finds nothing.
pub fn search(db: &Db, flavor: &str, text: &str) -> AppResult<SearchResults> {
    let query = Query::parse(text);
    let mut hits: Vec<SearchHit> = Vec::new();
    if !query.is_empty() {
        db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT c.id, c.name, c.surname, c.class, i.location, i.item_id, max(i.link),
                        sum(i.count), it.name, it.quality, it.ilvl
                 FROM char_items i
                 JOIN characters c ON c.id = i.character_id
                 LEFT JOIN items it ON it.item_id = i.item_id
                 WHERE c.flavor = ?1 AND i.location IN ('bag', 'bank', 'mail')
                 GROUP BY c.id, i.location, i.item_id",
            )?;
            let rows = stmt.query_map([flavor], |r| {
                let link: String = r.get(6)?;
                let surname: Option<String> = r.get(2)?;
                let first: String = r.get(1)?;
                let quality: Option<i64> = r.get(9)?;
                Ok(SearchHit {
                    character_id: r.get::<_, i64>(0)? as u32,
                    character: match surname {
                        Some(s) => format!("{first} {s}"),
                        None => first,
                    },
                    class: r.get::<_, Option<String>>(3)?.map(|c| c.to_lowercase()),
                    location: r.get(4)?,
                    item_id: r.get::<_, i64>(5)? as u32,
                    name: r
                        .get::<_, Option<String>>(8)?
                        .or_else(|| link_name(&link))
                        .unwrap_or_else(|| "Unknown item".into()),
                    quality: quality
                        .and_then(|q| u8::try_from(q).ok())
                        .or_else(|| link_quality(&link)),
                    ilvl: opt_u32(r.get(10)?),
                    count: u32::try_from(r.get::<_, i64>(7)?).unwrap_or(u32::MAX),
                })
            })?;
            for hit in rows {
                let hit = hit?;
                if query.matches(&hit.name, hit.ilvl) {
                    hits.push(hit);
                }
            }
            Ok(())
        })?;
    }
    hits.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.character.cmp(&b.character))
    });
    let total = hits.iter().fold(0u32, |n, h| n.saturating_add(h.count));
    let mut characters: Vec<u32> = hits.iter().map(|h| h.character_id).collect();
    characters.sort_unstable();
    characters.dedup();
    let more = hits.len() > SEARCH_MAX;
    hits.truncate(SEARCH_MAX);
    Ok(SearchResults {
        hits,
        total,
        characters,
        more,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{self, apply::Target};

    fn ingest_text(db: &Db, char_dir: &str, src: &str) {
        let t = Target {
            flavor: "_classic_beta_".into(),
            account: "ACCOUNT1".into(),
            group_dir: "70".into(),
            char_dir: char_dir.into(),
        };
        let out = ingest::ingest_bytes(db, &t, src.as_bytes()).unwrap();
        assert!(matches!(out, ingest::Outcome::Applied(_)), "{out:?}");
    }

    fn full(at: i64, money: i64) -> String {
        format!(
            r#"ForeverBuddyDB = {{
  _meta = {{ schema = 1, written = {at}, counts = {{ sessions = 0, events = 0, items = 1, bag_items = 2 }} }},
  character = {{ name = "Ellygie", surname = "Vargur", class = "MAGE", race = "Gnome", level = 30 }},
  snapshot = {{
    at = {at}, money = {money}, level = 30, ilvl = {{ equipped = 30.25 }}, played = {{ total = 3600 }},
    zone = {{ zone = "Stranglethorn Vale", subzone = "Booty Bay" }},
    equipped = {{ [1] = "|cff0070dd|Hitem:7413::::|h[Rare Helm]|h|r" }},
    bags = {{ [0] = {{ name = "Backpack", size = 16, free = 14,
                      items = {{ {{ link = "|cffffffff|Hitem:2589:|h[Linen Cloth]|h|r", count = 20 }},
                                {{ link = "|cffffffff|Hitem:117:|h[Tough Jerky]|h|r", count = 5 }} }} }} }},
    bank = {{ at = {bank}, bags = {{ [1] = {{ name = "Tab 1", items = {{ {{ link = "|Hitem:2592:|h[Wool]|h", count = 7 }} }} }} }} }},
    mail = {{ at = {at}, items = {{ {{ sender = "Someone", subject = "Hi", money = 50,
                                     items = {{ {{ link = "|Hitem:858:|h[Potion]|h", count = 2 }} }} }} }} }},
    professions = {{ {{ name = "Tailoring", skill = 150, max = 225 }} }},
    lockouts = {{ {{ name = "Molten Core", difficulty = "Normal", reset_at = {mc}, raid = true }},
                 {{ name = "The Deadmines", difficulty = "Normal", reset_at = {dm} }} }},
  }},
  items = {{ [2589] = {{ name = "Linen Cloth", quality = 1, ilvl = 5 }} }},
  sessions = {{}},
}}
"#,
            bank = at - 100,
            // Molten Core resets in three days; the Deadmines already has.
            mc = at + 3 * 86_400,
            dm = at - 10,
        )
    }

    #[test]
    fn overview_and_sheet_show_what_was_ingested() {
        let db = Db::open_in_memory().unwrap();
        let now = chrono::Utc::now().timestamp();
        ingest_text(&db, "Ellygie-Vargur", &full(now - 600, 123456));

        let o = overview(&db, "_classic_beta_").unwrap();
        assert_eq!(o.characters.len(), 1);
        assert_eq!(o.gold, 123456.0);
        assert_eq!(o.items, 20 + 5 + 7 + 2, "bags, bank and mail; not gear");
        let c = &o.characters[0];
        assert_eq!(
            (c.name.as_str(), c.surname.as_deref()),
            ("Ellygie", Some("Vargur"))
        );
        assert_eq!(c.class.as_deref(), Some("mage"));
        assert_eq!((c.bag_free, c.bag_size), (Some(14), Some(16)));
        assert_eq!((c.mail, c.bank_items), (1, 7));
        assert_eq!(c.subzone.as_deref(), Some("Booty Bay"));

        let s = sheet(&db, c.id).unwrap();
        assert_eq!(s.equipped[0].name, "Rare Helm", "name from the link");
        assert_eq!(
            s.equipped[0].quality,
            Some(3),
            "quality from the link colour"
        );
        assert_eq!(s.bags[0].name.as_deref(), Some("Backpack"));
        assert_eq!(s.bags[0].items[0].name, "Linen Cloth");
        assert_eq!(
            s.bags[0].items[0].ilvl,
            Some(5),
            "static info wins when known"
        );
        assert!(s.bank.as_of.is_some());
        assert_eq!(s.bank.bags[0].items[0].count, 7);
        assert_eq!(s.mail.messages[0].sender.as_deref(), Some("Someone"));
        assert_eq!(s.mail.messages[0].items[0].item_id, 858);
        assert_eq!(s.professions[0].name, "Tailoring");
        assert_eq!(s.gold_30d.len(), 1);
        let names: Vec<_> = s.lockouts.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Molten Core"], "the reset Deadmines save is gone");
        assert_eq!(s.lockouts_as_of.as_deref(), Some(iso(now - 600).as_str()));
        assert!(s.lockouts[0].raid && s.lockouts[0].reset_at.is_some());
        assert!(!c.bank_alt);
    }

    /// F3: the Dashboard's saves across alts, live ones only, soonest first.
    #[test]
    fn lockouts_across_alts() {
        let db = Db::open_in_memory().unwrap();
        let now = chrono::Utc::now().timestamp();
        ingest_text(&db, "Ellygie-Vargur", &full(now - 600, 1));
        let brannic = full(now - 86_400, 1).replace(
            r#"name = "Ellygie", surname = "Vargur""#,
            r#"name = "Brannic""#,
        );
        ingest_text(&db, "Brannic", &brannic);

        let all = lockouts(&db, "_classic_beta_", now).unwrap();
        assert_eq!(all.len(), 2, "one live Molten Core save each: {all:?}");
        assert!(all.iter().all(|a| a.lockout.name == "Molten Core"));
        assert!(
            all[0].lockout.reset_at <= all[1].lockout.reset_at,
            "soonest reset first"
        );
        assert_eq!(all[0].class.as_deref(), Some("mage"));
        assert!(lockouts(&db, "_classic_", now).unwrap().is_empty());
        // A week later both have reset.
        assert!(lockouts(&db, "_classic_beta_", now + 7 * 86_400)
            .unwrap()
            .is_empty());
    }

    /// F3: the bank-alt tag is the user's: it survives the next ingest.
    #[test]
    fn the_bank_alt_tag_sticks() {
        let db = Db::open_in_memory().unwrap();
        let now = chrono::Utc::now().timestamp();
        ingest_text(&db, "Ellygie-Vargur", &full(now - 600, 1));
        let id = overview(&db, "_classic_beta_").unwrap().characters[0].id;

        set_bank_alt(&db, id, true).unwrap();
        ingest_text(&db, "Ellygie-Vargur", &full(now - 60, 2));
        let c = &overview(&db, "_classic_beta_").unwrap().characters[0];
        assert!(c.bank_alt && c.money == 2.0, "tag kept, data updated");

        set_bank_alt(&db, id, false).unwrap();
        assert!(!overview(&db, "_classic_beta_").unwrap().characters[0].bank_alt);
        assert!(matches!(
            set_bank_alt(&db, 999, true),
            Err(AppError::NotFound(_))
        ));
    }

    /// The real addon's output (generated by tools/addon-test), so the
    /// screen's queries break if what the addon writes changes.
    #[test]
    fn the_snapshot_fixture_fills_the_card_and_sheet() {
        let db = Db::open_in_memory().unwrap();
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/addon/snapshot.lua"),
        )
        .unwrap();
        ingest_text(&db, "Thrandor-Vargur", &src);

        let o = overview(&db, "_classic_beta_").unwrap();
        assert_eq!(o.gold, 25000.0);
        assert_eq!(o.items, 1 + 4 + 20 + 10, "bags, bank and mail; not gear");
        let c = &o.characters[0];
        assert_eq!(
            (c.level, c.bag_free, c.bag_size),
            (Some(12), Some(14), Some(16))
        );
        assert_eq!((c.mail, c.bank_items), (1, 20));
        assert_eq!(c.subzone.as_deref(), Some("Goldshire"));

        let s = sheet(&db, c.id).unwrap();
        assert_eq!(s.equipped[0].name, "Worn Shortsword");
        assert_eq!(s.bags[0].name.as_deref(), Some("Backpack"));
        assert_eq!(s.bank.bags[0].items[0].name, "Runecloth");
        assert_eq!(s.bank.bags[0].items[0].count, 20);
        assert_eq!(s.mail.messages[0].sender.as_deref(), Some("Coinpurse"));
        assert_eq!(s.professions.len(), 3);
    }

    /// The tooltip's "Looted <date> · <zone>": from the journal's `gain`
    /// events, never for items bought or mailed. Both files are the addon's.
    #[test]
    fn items_know_when_and_where_they_were_looted() {
        let db = Db::open_in_memory().unwrap();
        for name in ["adventure.lua", "snapshot.lua"] {
            let src = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/addon")
                    .join(name),
            )
            .unwrap();
            ingest_text(&db, "Thrandor-Vargur", &src);
        }
        let id = overview(&db, "_classic_beta_").unwrap().characters[0].id;
        let s = sheet(&db, id).unwrap();
        let bag = |item: u32| {
            s.bags
                .iter()
                .flat_map(|b| &b.items)
                .find(|i| i.item_id == item)
                .unwrap()
        };
        // Linen Cloth: picked up in Westfall during the adventure.
        assert_eq!(bag(2589).looted_in.as_deref(), Some("Westfall"));
        assert_eq!(
            bag(2589).looted_at.as_deref(),
            Some(iso(1790964360).as_str())
        );
        // The Hearthstone was never looted in the journal.
        assert_eq!(bag(6948).looted_at, None);
    }

    #[test]
    fn another_flavor_and_unknown_ids() {
        let db = Db::open_in_memory().unwrap();
        ingest_text(&db, "Ellygie-Vargur", &full(1000, 1));
        assert!(overview(&db, "_classic_").unwrap().characters.is_empty());
        assert!(matches!(sheet(&db, 999), Err(AppError::NotFound(_))));
    }

    #[test]
    fn links_give_names_and_qualities() {
        assert_eq!(
            link_name("|cffa335ee|Hitem:19019::::|h[Thunderfury]|h|r").as_deref(),
            Some("Thunderfury")
        );
        assert_eq!(link_quality("|cffa335ee|Hitem:19019|h[x]|h|r"), Some(4));
        assert_eq!(link_quality("|Hitem:1|h[x]|h"), None);
    }

    /// Two characters: Thrandor from the addon's own snapshot.lua (Linen ×4
    /// in bags, ×10 in mail; Runecloth ×20 in the bank), and Ellygie from
    /// `full` (Linen ×20 in bags, Wool ×7 in the bank, no item info for Wool).
    fn two_alts() -> Db {
        let db = Db::open_in_memory().unwrap();
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/addon/snapshot.lua"),
        )
        .unwrap();
        ingest_text(&db, "Thrandor-Vargur", &src);
        ingest_text(&db, "Ellygie-Vargur", &full(1000, 1));
        db
    }

    #[test]
    fn search_finds_an_item_across_alts_and_places() {
        let db = two_alts();
        let r = search(&db, "_classic_beta_", "linen").unwrap();
        let rows: Vec<(&str, &str, u32)> = r
            .hits
            .iter()
            .map(|h| (h.character.as_str(), h.location.as_str(), h.count))
            .collect();
        assert_eq!(
            rows,
            [
                ("Ellygie Vargur", "bag", 20),
                ("Thrandor Vargur", "mail", 10),
                ("Thrandor Vargur", "bag", 4),
            ],
            "most first"
        );
        assert_eq!((r.total, r.characters.len(), r.more), (34, 2, false));
        // Every word, any case.
        assert_eq!(
            search(&db, "_classic_beta_", "CLOTH linen").unwrap().total,
            34
        );
        assert_eq!(
            search(&db, "_classic_beta_", "linen wool")
                .unwrap()
                .hits
                .len(),
            0
        );
        // The bank, as of the last visit.
        let rune = search(&db, "_classic_beta_", "rune").unwrap();
        assert_eq!(
            (rune.hits[0].location.as_str(), rune.hits[0].count),
            ("bank", 20)
        );
    }

    #[test]
    fn search_leaves_out_gear_other_flavors_and_empty_queries() {
        let db = two_alts();
        // The Worn Shortsword is equipped, not carried.
        assert!(search(&db, "_classic_beta_", "shortsword")
            .unwrap()
            .hits
            .is_empty());
        assert!(search(&db, "_classic_", "linen").unwrap().hits.is_empty());
        let empty = search(&db, "_classic_beta_", "   ").unwrap();
        assert!(empty.hits.is_empty() && empty.characters.is_empty());
    }

    #[test]
    fn search_filters_by_item_level() {
        let db = two_alts();
        // Wool has no item info, so no known item level: an ilvl filter
        // leaves it out.
        let r = search(&db, "_classic_beta_", "ilvl>=10").unwrap();
        assert!(r.hits.iter().any(|h| h.name == "Runecloth"));
        assert!(!r.hits.iter().any(|h| h.name == "Wool"));
        assert!(search(&db, "_classic_beta_", "rune ilvl>10")
            .unwrap()
            .hits
            .is_empty());
        assert_eq!(
            search(&db, "_classic_beta_", "rune ilvl=10")
                .unwrap()
                .hits
                .len(),
            1
        );
    }

    #[test]
    fn queries_parse_words_and_item_level_filters() {
        let q = Query::parse("Arcanite  ilvl>60 ILVL<=70 ilvl>x");
        assert_eq!(
            q.words,
            ["arcanite", "ilvl>x"],
            "a filter without a number is a word"
        );
        assert_eq!(q.ilvl, [(Cmp::Gt, 60), (Cmp::Le, 70)]);
        assert!(Query::parse(" ").is_empty());
    }
}
