//! The Characters screen and the character sheet (v0.2 V7): what ingest put
//! in the db, shaped for the UI. Read-only.
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
    pub spec: Option<String>,
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
             WHERE i.character_id = c.id AND i.location = 'bank')
    FROM characters c
    LEFT JOIN char_snapshots s
      ON s.character_id = c.id
     AND s.at = (SELECT max(at) FROM char_snapshots WHERE character_id = c.id)";

fn card(r: &Row<'_>) -> rusqlite::Result<CharacterCard> {
    let f = |v: Option<i64>| v.map(|v| v as f64);
    Ok(CharacterCard {
        id: r.get::<_, i64>(0)? as u32,
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
    let mut stmt = c.prepare(
        "SELECT i.container, i.slot, i.item_id, i.link, i.count, it.name, it.quality, it.ilvl
         FROM char_items i LEFT JOIN items it ON it.item_id = i.item_id
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
            "SELECT name, skill, max, spec FROM professions WHERE character_id = ?1
             ORDER BY coalesce(skill, 0) DESC, name",
        )?;
        let professions = stmt
            .query_map([id], |r| {
                Ok(ProfessionRow {
                    name: r.get(0)?,
                    skill: opt_u32(r.get(1)?),
                    max: opt_u32(r.get(2)?),
                    spec: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let since = chrono::Utc::now().timestamp() - 30 * 86_400;
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
            gold_30d,
            card,
        })
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
    bank = {{ at = {bank}, tabs = {{ [1] = {{ name = "Tab 1", items = {{ {{ link = "|Hitem:2592:|h[Wool]|h", count = 7 }} }} }} }} }},
    mail = {{ at = {at}, items = {{ {{ sender = "Someone", subject = "Hi", money = 50,
                                     items = {{ {{ link = "|Hitem:858:|h[Potion]|h", count = 2 }} }} }} }} }},
    professions = {{ {{ name = "Tailoring", skill = 150, max = 225 }} }},
  }},
  items = {{ [2589] = {{ name = "Linen Cloth", quality = 1, ilvl = 5 }} }},
  sessions = {{}},
}}
"#,
            bank = at - 100
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
}
