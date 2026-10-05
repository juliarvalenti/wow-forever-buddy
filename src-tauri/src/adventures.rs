//! Adventures (spec v0.2-addon §5, `session.html`): one login to logout,
//! recapped from the addon's session log (V3's events, as ingest stored them
//! in `adventure_events`: the addon's fields minus `kind` and `t`). Read-only
//! over migration 004, except the user's own note.
//!
//! Combat details the client withholds from addons (who killed you, which
//! mob dropped what) never appear; those lines are marked `withheld` so the
//! page can say so once.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{AppError, AppResult};

/// Notes longer than this are cut (a note, not an essay).
pub const NOTE_MAX: usize = 2000;
/// Loot this rare or better gets its own timeline line (3 = rare).
const NOTABLE_QUALITY: i64 = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct AdventureLink {
    pub id: u32,
    pub name: String,
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Adventure {
    pub id: u32,
    pub character_id: u32,
    pub name: String,
    /// File tokens, e.g. "WARRIOR" and "Human".
    pub class: Option<String>,
    pub race: Option<String>,
    /// RFC 3339, UTC.
    pub login: String,
    pub logout: Option<String>,
    pub played_secs: Option<u32>,
    /// The zone or instance with the most time, or the top two joined
    /// ("Stratholme & Eastern Plaguelands") when the second had at least
    /// half as long.
    pub title: String,
    pub level_start: Option<u32>,
    pub level_end: Option<u32>,
    /// Where the character logged out.
    pub last_zone: Option<String>,
    /// Zones in the order first visited.
    pub travelled: Vec<String>,
    pub tally: Tally,
    /// Gold through the session: login, every money point, logout.
    pub money: Vec<MoneyPoint>,
    /// Deaths, bosses, level-ups and quests, for the money chart.
    pub markers: Vec<Marker>,
    pub timeline: Vec<Line>,
    pub gained: Vec<ItemLine>,
    pub spent: Vec<ItemLine>,
    pub quests: Vec<QuestLine>,
    pub note: Option<String>,
    /// The adventures before and after this one, across characters.
    pub prev: Option<AdventureLink>,
    pub next: Option<AdventureLink>,
}

/// Money is copper, as `f64` (specta sends no `i64`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Tally {
    pub gold: Option<f64>,
    /// Only when the level didn't change: across a level the client's XP
    /// numbers restart, and the addon doesn't record the old maximum.
    pub xp: Option<f64>,
    /// Items looted (not bought or taken from mail).
    pub loot: u32,
    pub deaths: u32,
    pub repairs: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct MoneyPoint {
    pub at: String,
    pub money: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Marker {
    pub at: String,
    /// death | encounter | level | quest
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Line {
    pub at: String,
    /// login | zone | death | repair | encounter | level | quest | loot | logout
    pub kind: String,
    pub text: String,
    pub detail: Option<String>,
    /// For an item line, its quality (the letter tile's colour).
    pub quality: Option<u32>,
    /// The client withholds part of this (a killer, a loot source).
    pub withheld: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ItemLine {
    pub item_id: u32,
    pub name: String,
    pub quality: Option<u32>,
    pub count: u32,
    /// sold | used | mailed for what was spent; bought | mail for a gain
    /// that didn't drop; `None` for loot.
    pub how: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct QuestLine {
    pub title: String,
    /// The zone the character was in when they turned it in.
    pub zone: Option<String>,
}

pub struct Event {
    pub at: i64,
    pub kind: String,
    pub data: serde_json::Value,
}

fn int(v: &serde_json::Value, key: &str) -> Option<i64> {
    v.get(key).and_then(serde_json::Value::as_i64)
}

fn text<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(serde_json::Value::as_str)
}

fn rfc3339(t: i64) -> String {
    DateTime::<Utc>::from_timestamp(t, 0)
        .unwrap_or_default()
        .to_rfc3339()
}

/// "1,828g", "47s" under a gold, "9c" under a silver: as the UI writes it.
pub fn gold_text(copper: i64) -> String {
    let c = copper.unsigned_abs();
    let sign = if copper < 0 { "−" } else { "" };
    let body = if c >= 10_000 {
        let g = (c + 5_000) / 10_000;
        let digits = g.to_string();
        let mut out = String::new();
        for (i, ch) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(ch);
        }
        format!("{out}g")
    } else if c >= 100 {
        format!("{}s", c / 100)
    } else {
        format!("{c}c")
    };
    format!("{sign}{body}")
}

/// Seconds in each zone: from its zone event to the next one (or `end`).
/// In the order first visited.
pub fn zone_times(events: &[Event], end: Option<i64>) -> Vec<(String, i64)> {
    let zones: Vec<&Event> = events.iter().filter(|e| e.kind == "zone").collect();
    let mut out: Vec<(String, i64)> = Vec::new();
    for (i, z) in zones.iter().enumerate() {
        let Some(name) = text(&z.data, "zone") else {
            continue;
        };
        let until = zones.get(i + 1).map(|n| n.at).or(end).unwrap_or(z.at);
        let secs = (until - z.at).max(0);
        match out.iter_mut().find(|(n, _)| n == name) {
            Some((_, t)) => *t += secs,
            None => out.push((name.to_string(), secs)),
        }
    }
    out
}

pub fn load_events(db: &Db, adventure: i64) -> AppResult<Vec<Event>> {
    db.with_conn(|c| {
        Ok(c.prepare(
            "SELECT at, kind, data FROM adventure_events WHERE adventure_id = ?1 ORDER BY seq",
        )?
        .query_map([adventure], |r| {
            let data: String = r.get(2)?;
            Ok(Event {
                at: r.get(0)?,
                kind: r.get(1)?,
                data: serde_json::from_str(&data).unwrap_or_default(),
            })
        })?
        .collect::<Result<_, _>>()?)
    })
}

/// An item's name and quality, as ingest stored them.
type ItemInfo = (Option<String>, Option<i64>);

fn item_info(db: &Db, ids: &[i64]) -> AppResult<HashMap<i64, ItemInfo>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare("SELECT name, quality FROM items WHERE item_id = ?1")?;
        let mut out = HashMap::new();
        for id in ids {
            if let Some(info) = stmt
                .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?
            {
                out.insert(*id, info);
            }
        }
        Ok(out)
    })
}

fn display_name(name: String, surname: Option<String>) -> String {
    match surname {
        Some(s) if !s.is_empty() => format!("{name} {s}"),
        _ => name,
    }
}

/// The newest adventure in `flavor`, if there is one.
pub fn latest(db: &Db, flavor: &str) -> AppResult<Option<u32>> {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT a.id FROM adventures a JOIN characters c ON c.id = a.character_id
             WHERE c.flavor = ?1 ORDER BY a.login DESC, a.id DESC LIMIT 1",
            [flavor],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .map(|id| id as u32))
    })
}

fn neighbour(
    db: &Db,
    flavor: &str,
    login: i64,
    id: i64,
    newer: bool,
) -> AppResult<Option<AdventureLink>> {
    let sql = if newer {
        "SELECT a.id, c.name, c.surname, a.login FROM adventures a JOIN characters c ON c.id = a.character_id
         WHERE c.flavor = ?1 AND (a.login > ?2 OR (a.login = ?2 AND a.id > ?3))
         ORDER BY a.login, a.id LIMIT 1"
    } else {
        "SELECT a.id, c.name, c.surname, a.login FROM adventures a JOIN characters c ON c.id = a.character_id
         WHERE c.flavor = ?1 AND (a.login < ?2 OR (a.login = ?2 AND a.id < ?3))
         ORDER BY a.login DESC, a.id DESC LIMIT 1"
    };
    db.with_conn(|c| {
        Ok(c.query_row(sql, params![flavor, login, id], |r| {
            Ok(AdventureLink {
                id: r.get::<_, i64>(0)? as u32,
                name: display_name(r.get(1)?, r.get(2)?),
                login: rfc3339(r.get(3)?),
            })
        })
        .optional()?)
    })
}

struct Row {
    character_id: i64,
    name: String,
    class: Option<String>,
    race: Option<String>,
    login: i64,
    logout: Option<i64>,
    start_money: Option<i64>,
    end_money: Option<i64>,
    start_xp: Option<i64>,
    start_level: Option<i64>,
    end_level: Option<i64>,
    note: Option<String>,
}

/// One adventure of `flavor`, recapped; `None` if there's no such adventure
/// in that flavor.
pub fn adventure(db: &Db, flavor: &str, id: u32) -> AppResult<Option<Adventure>> {
    let id = i64::from(id);
    let row: Option<Row> = db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT a.character_id, c.name, c.surname, c.class, c.race, a.login, a.logout,
                    a.start_money, a.end_money, a.start_xp, a.start_level, a.end_level, a.note
             FROM adventures a JOIN characters c ON c.id = a.character_id
             WHERE a.id = ?1 AND c.flavor = ?2",
            params![id, flavor],
            |r| {
                Ok(Row {
                    character_id: r.get(0)?,
                    name: display_name(r.get(1)?, r.get(2)?),
                    class: r.get(3)?,
                    race: r.get(4)?,
                    login: r.get(5)?,
                    logout: r.get(6)?,
                    start_money: r.get(7)?,
                    end_money: r.get(8)?,
                    start_xp: r.get(9)?,
                    start_level: r.get(10)?,
                    end_level: r.get(11)?,
                    note: r.get(12)?,
                })
            },
        )
        .optional()?)
    })?;
    let Some(row) = row else { return Ok(None) };
    let events = load_events(db, id)?;

    // The logout snapshot: XP at the end, and the zone if no event named one.
    let end_snapshot: Option<(Option<i64>, Option<String>)> = match row.logout {
        Some(at) => db.with_conn(|c| {
            Ok(c.query_row(
                "SELECT xp, zone FROM char_snapshots WHERE character_id = ?1 AND at = ?2",
                params![row.character_id, at],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
        })?,
        None => None,
    };

    let times = zone_times(&events, row.logout);
    let mut ranked = times.clone();
    ranked.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
    let last_zone = events
        .iter()
        .rev()
        .find(|e| e.kind == "zone")
        .and_then(|e| text(&e.data, "zone").map(str::to_string))
        .or_else(|| end_snapshot.as_ref().and_then(|s| s.1.clone()));
    let title = match ranked.as_slice() {
        [(a, ta), (b, tb), ..] if *tb * 2 >= *ta && *tb > 0 => format!("{a} & {b}"),
        [(a, _), ..] => a.clone(),
        [] => last_zone
            .clone()
            .unwrap_or_else(|| format!("{}'s session", row.name)),
    };

    let item_ids: Vec<i64> = events
        .iter()
        .filter(|e| e.kind == "gain" || e.kind == "lose")
        .filter_map(|e| int(&e.data, "item"))
        .collect();
    let items = item_info(db, &item_ids)?;
    let item_name = |id: i64| {
        items
            .get(&id)
            .and_then(|i| i.0.clone())
            .unwrap_or_else(|| format!("Item {id}"))
    };
    let item_quality = |id: i64| items.get(&id).and_then(|i| i.1);

    // Timeline, money and markers ---------------------------------------------------
    let mut timeline = vec![Line {
        at: rfc3339(row.login),
        kind: "login".into(),
        text: "Logged in".into(),
        detail: row.start_money.map(gold_text),
        quality: None,
        withheld: false,
    }];
    let mut money = row
        .start_money
        .map(|m| MoneyPoint {
            at: rfc3339(row.login),
            money: m as f64,
        })
        .into_iter()
        .collect::<Vec<_>>();
    let mut markers = Vec::new();
    let mut quests = Vec::new();
    let (mut deaths, mut repairs, mut loot) = (0u32, 0i64, 0u32);
    let mut gained: Vec<(i64, Option<String>, u32)> = Vec::new();
    let mut spent: Vec<(i64, Option<String>, u32)> = Vec::new();
    let mut zone_now: Option<String> = None;

    for e in &events {
        let at = rfc3339(e.at);
        let mut line = |kind: &str,
                        text: String,
                        detail: Option<String>,
                        quality: Option<u32>,
                        withheld: bool| {
            timeline.push(Line {
                at: at.clone(),
                kind: kind.into(),
                text,
                detail,
                quality,
                withheld,
            });
        };
        match e.kind.as_str() {
            "zone" => {
                let Some(zone) = text(&e.data, "zone") else {
                    continue;
                };
                zone_now = Some(zone.to_string());
                let instance =
                    e.data.get("instance").and_then(serde_json::Value::as_bool) == Some(true);
                let verb = if instance { "Entered" } else { "Travelled to" };
                line("zone", format!("{verb} {zone}"), None, None, false);
            }
            "money" => {
                if let Some(m) = int(&e.data, "money") {
                    money.push(MoneyPoint {
                        at: at.clone(),
                        money: m as f64,
                    });
                }
            }
            "death" => {
                deaths += 1;
                let zone = text(&e.data, "zone");
                let text = zone.map_or_else(|| "Died".to_string(), |z| format!("Died in {z}"));
                markers.push(Marker {
                    at: at.clone(),
                    kind: "death".into(),
                    label: text.clone(),
                });
                line("death", text, None, None, true);
            }
            "repair" => {
                let cost = int(&e.data, "cost").unwrap_or(0);
                repairs += cost;
                line(
                    "repair",
                    format!("Repaired for {}", gold_text(cost)),
                    None,
                    None,
                    false,
                );
            }
            "encounter" => {
                let name = text(&e.data, "name").unwrap_or("a boss");
                let text = format!("Defeated {name}");
                markers.push(Marker {
                    at: at.clone(),
                    kind: "encounter".into(),
                    label: text.clone(),
                });
                line("encounter", text, None, None, false);
            }
            "level" => {
                let text = int(&e.data, "level").map_or_else(
                    || "Levelled up".to_string(),
                    |l| format!("Reached level {l}"),
                );
                markers.push(Marker {
                    at: at.clone(),
                    kind: "level".into(),
                    label: text.clone(),
                });
                line("level", text, None, None, false);
            }
            "quest" => {
                let title = text(&e.data, "title").map(str::to_string);
                let shown = title.clone().unwrap_or_else(|| "a quest".to_string());
                let mut reward = Vec::new();
                if let Some(m) = int(&e.data, "money").filter(|m| *m > 0) {
                    reward.push(format!("+{}", gold_text(m)));
                }
                if let Some(x) = int(&e.data, "xp").filter(|x| *x > 0) {
                    reward.push(format!("+{x} XP"));
                }
                markers.push(Marker {
                    at: at.clone(),
                    kind: "quest".into(),
                    label: format!("Turned in {shown}"),
                });
                line(
                    "quest",
                    format!("Turned in {shown}"),
                    (!reward.is_empty()).then(|| reward.join(" · ")),
                    None,
                    false,
                );
                quests.push(QuestLine {
                    title: shown,
                    zone: zone_now.clone(),
                });
            }
            "gain" => {
                let Some(item) = int(&e.data, "item") else {
                    continue;
                };
                let count = int(&e.data, "count").unwrap_or(1).max(0) as u32;
                let how = text(&e.data, "how").map(str::to_string);
                if how.is_none() {
                    loot += count;
                    if item_quality(item).is_some_and(|q| q >= NOTABLE_QUALITY) {
                        // The source (which mob) is withheld from addons.
                        line(
                            "loot",
                            format!("Looted {}", item_name(item)),
                            None,
                            item_quality(item).map(|q| q as u32),
                            true,
                        );
                    }
                }
                add(&mut gained, item, how, count);
            }
            "lose" => {
                let Some(item) = int(&e.data, "item") else {
                    continue;
                };
                let count = int(&e.data, "count").unwrap_or(1).max(0) as u32;
                add(
                    &mut spent,
                    item,
                    text(&e.data, "how").map(str::to_string),
                    count,
                );
            }
            _ => {}
        }
    }

    if let Some(out) = row.logout {
        if let Some(m) = row.end_money {
            money.push(MoneyPoint {
                at: rfc3339(out),
                money: m as f64,
            });
        }
        timeline.push(Line {
            at: rfc3339(out),
            kind: "logout".into(),
            text: last_zone.as_deref().map_or_else(
                || "Logged out".to_string(),
                |z| format!("Logged out in {z}"),
            ),
            detail: row.end_money.map(gold_text),
            quality: None,
            withheld: false,
        });
    }

    let lines = |list: Vec<(i64, Option<String>, u32)>| -> Vec<ItemLine> {
        let mut out: Vec<ItemLine> = list
            .into_iter()
            .map(|(item, how, count)| ItemLine {
                item_id: item as u32,
                name: item_name(item),
                quality: item_quality(item).map(|q| q as u32),
                count,
                how,
            })
            .collect();
        // Best first, then the biggest stacks.
        out.sort_by(|a, b| {
            b.quality
                .cmp(&a.quality)
                .then(b.count.cmp(&a.count))
                .then(a.name.cmp(&b.name))
        });
        out
    };

    let xp = match (
        row.start_level,
        row.end_level,
        row.start_xp,
        end_snapshot.as_ref().and_then(|s| s.0),
    ) {
        (Some(a), Some(b), Some(start), Some(end)) if a == b => Some((end - start) as f64),
        _ => None,
    };
    let prev = neighbour(db, flavor, row.login, id, false)?;
    let next = neighbour(db, flavor, row.login, id, true)?;
    Ok(Some(Adventure {
        id: id as u32,
        character_id: row.character_id as u32,
        name: row.name,
        class: row.class,
        race: row.race,
        login: rfc3339(row.login),
        logout: row.logout.map(rfc3339),
        played_secs: row.logout.map(|o| (o - row.login).max(0) as u32),
        title,
        level_start: row.start_level.map(|l| l as u32),
        level_end: row.end_level.map(|l| l as u32),
        last_zone,
        travelled: times.into_iter().map(|(z, _)| z).collect(),
        tally: Tally {
            gold: row
                .start_money
                .zip(row.end_money)
                .map(|(s, e)| (e - s) as f64),
            xp,
            loot,
            deaths,
            repairs: repairs as f64,
        },
        money,
        markers,
        timeline,
        gained: lines(gained),
        spent: lines(spent),
        quests,
        note: row.note,
        prev,
        next,
    }))
}

/// Adds `count` of `item` (got or lost `how`) to a running list.
fn add(list: &mut Vec<(i64, Option<String>, u32)>, item: i64, how: Option<String>, count: u32) {
    match list.iter_mut().find(|(i, h, _)| *i == item && *h == how) {
        Some((_, _, n)) => *n += count,
        None => list.push((item, how, count)),
    }
}

/// The user's note on an adventure, kept across re-ingest. Blank clears it.
pub fn set_note(db: &Db, flavor: &str, id: u32, note: &str) -> AppResult<()> {
    let note = note.trim();
    let note: Option<String> = (!note.is_empty()).then(|| note.chars().take(NOTE_MAX).collect());
    let changed = db.with_conn(|c| {
        Ok(c.execute(
            "UPDATE adventures SET note = ?1
             WHERE id = ?2 AND character_id IN (SELECT id FROM characters WHERE flavor = ?3)",
            params![note, i64::from(id), flavor],
        )?)
    })?;
    if changed == 0 {
        return Err(AppError::Io(format!("no adventure {id}")));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const FLAVOR: &str = "_classic_beta_";
    /// Sat 3 Oct 2026, 19:40 UTC.
    const L: i64 = 1_791_056_400;
    const G: i64 = 10_000;

    pub fn character(db: &Db, flavor: &str, dir: &str, name: &str) -> i64 {
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO characters (flavor, account, group_dir, char_dir, name, class, race, first_seen, last_seen)
                 VALUES (?1, 'ACCOUNT1', '70', ?2, ?3, 'PALADIN', 'Human', 0, 0)",
                params![flavor, dir, name],
            )?;
            Ok(c.last_insert_rowid())
        })
        .unwrap()
    }

    pub struct Adv<'a> {
        pub login: i64,
        pub logout: i64,
        pub money: (i64, i64),
        pub level: (i64, i64),
        pub xp: i64,
        pub events: &'a [(i64, &'a str, &'a str)],
    }

    pub fn adventure_row(db: &Db, character: i64, a: &Adv<'_>) -> i64 {
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO adventures (character_id, login, logout, start_money, end_money, start_xp, start_level, end_level)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![character, a.login, a.logout, a.money.0, a.money.1, a.xp, a.level.0, a.level.1],
            )?;
            let id = c.last_insert_rowid();
            for (seq, (at, kind, data)) in a.events.iter().enumerate() {
                c.execute(
                    "INSERT INTO adventure_events (adventure_id, seq, at, kind, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, seq as i64, at, kind, data],
                )?;
            }
            Ok(id)
        })
        .unwrap()
    }

    fn item(db: &Db, id: i64, name: &str, quality: i64) {
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO items (item_id, name, quality, seen_at) VALUES (?1, ?2, ?3, 0)",
                params![id, name, quality],
            )?;
            Ok(())
        })
        .unwrap();
    }

    /// session.html's evening, condensed: every kind of line once.
    #[test]
    fn recaps_an_evening() {
        let db = Db::open_in_memory().unwrap();
        let t = character(&db, FLAVOR, "Thrandor", "Thrandor");
        item(&db, 16_000, "Truestrike Shoulders", 3);
        item(&db, 14_047, "Runecloth", 1);
        item(&db, 999, "Vendor junk", 0);
        let events = [
            (L, "zone", r#"{"zone":"Eastern Plaguelands"}"#),
            (L + 720, "zone", r#"{"zone":"Stratholme","instance":true}"#),
            (L + 3_480, "death", r#"{"zone":"Stratholme"}"#),
            (L + 3_500, "repair", r#"{"cost":60000}"#),
            (
                L + 7_560,
                "encounter",
                r#"{"id":456,"name":"Baron Rivendare"}"#,
            ),
            (L + 7_620, "gain", r#"{"item":16000,"count":1}"#),
            (L + 7_700, "gain", r#"{"item":14047,"count":40}"#),
            (
                L + 7_800,
                "gain",
                r#"{"item":117,"count":5,"how":"bought"}"#,
            ),
            (L + 7_900, "lose", r#"{"item":117,"count":1,"how":"used"}"#),
            (L + 7_950, "lose", r#"{"item":999,"count":22,"how":"sold"}"#),
            (L + 8_100, "level", r#"{"level":60}"#),
            (
                L + 9_000,
                "quest",
                r#"{"id":5263,"title":"The Archivist","xp":38400,"money":620000}"#,
            ),
            (L + 9_010, "money", r#"{"money":21400000}"#),
        ];
        let id = adventure_row(
            &db,
            t,
            &Adv {
                login: L,
                logout: L + 11_520,
                money: (1_828 * G, 2_140 * G),
                level: (59, 60),
                xp: 100,
                events: &events,
            },
        );
        let a = adventure(&db, FLAVOR, id as u32).unwrap().unwrap();

        assert_eq!(
            a.title, "Stratholme",
            "most time; the plaguelands had too little to share it"
        );
        assert_eq!((a.level_start, a.level_end), (Some(59), Some(60)));
        assert_eq!(a.class.as_deref(), Some("PALADIN"));
        assert_eq!(a.played_secs, Some(11_520));
        assert_eq!(a.last_zone.as_deref(), Some("Stratholme"));
        assert_eq!(a.travelled, ["Eastern Plaguelands", "Stratholme"]);
        assert_eq!(a.tally.gold, Some(312.0 * G as f64));
        assert_eq!(a.tally.xp, None, "levelled, so XP can't be summed");
        assert_eq!(
            (a.tally.loot, a.tally.deaths, a.tally.repairs),
            (41, 1, 60_000.0)
        );

        let lines: Vec<(&str, &str, bool)> = a
            .timeline
            .iter()
            .map(|l| (l.kind.as_str(), l.text.as_str(), l.withheld))
            .collect();
        assert_eq!(
            lines,
            [
                ("login", "Logged in", false),
                ("zone", "Travelled to Eastern Plaguelands", false),
                ("zone", "Entered Stratholme", false),
                ("death", "Died in Stratholme", true),
                ("repair", "Repaired for 6g", false),
                ("encounter", "Defeated Baron Rivendare", false),
                ("loot", "Looted Truestrike Shoulders", true),
                ("level", "Reached level 60", false),
                ("quest", "Turned in The Archivist", false),
                ("logout", "Logged out in Stratholme", false),
            ]
        );
        assert_eq!(a.timeline[0].detail.as_deref(), Some("1,828g"));
        assert_eq!(a.timeline[8].detail.as_deref(), Some("+62g · +38400 XP"));
        assert_eq!(a.timeline[9].detail.as_deref(), Some("2,140g"));

        let marks: Vec<&str> = a.markers.iter().map(|m| m.kind.as_str()).collect();
        assert_eq!(marks, ["death", "encounter", "level", "quest"]);
        assert_eq!(a.money.first().map(|p| p.money), Some((1_828 * G) as f64));
        assert_eq!(a.money.last().map(|p| p.money), Some((2_140 * G) as f64));
        assert_eq!(a.money.len(), 3);

        let gained: Vec<(&str, u32, Option<&str>)> = a
            .gained
            .iter()
            .map(|i| (i.name.as_str(), i.count, i.how.as_deref()))
            .collect();
        assert_eq!(
            gained,
            [
                ("Truestrike Shoulders", 1, None),
                ("Runecloth", 40, None),
                ("Item 117", 5, Some("bought"))
            ]
        );
        let spent: Vec<(&str, u32, Option<&str>)> = a
            .spent
            .iter()
            .map(|i| (i.name.as_str(), i.count, i.how.as_deref()))
            .collect();
        assert_eq!(
            spent,
            [
                ("Vendor junk", 22, Some("sold")),
                ("Item 117", 1, Some("used"))
            ]
        );
        assert_eq!(
            a.quests,
            [QuestLine {
                title: "The Archivist".into(),
                zone: Some("Stratholme".into())
            }]
        );
    }

    #[test]
    fn two_zones_share_the_title_and_xp_counts_within_a_level() {
        let db = Db::open_in_memory().unwrap();
        let t = character(&db, FLAVOR, "Thrandor", "Thrandor");
        let events = [
            (L, "zone", r#"{"zone":"Eastern Plaguelands"}"#),
            (
                L + 1_000,
                "zone",
                r#"{"zone":"Stratholme","instance":true}"#,
            ),
        ];
        let id = adventure_row(
            &db,
            t,
            &Adv {
                login: L,
                logout: L + 1_800,
                money: (G, G),
                level: (58, 58),
                xp: 1_000,
                events: &events,
            },
        );
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO char_snapshots (character_id, at, money, xp) VALUES (?1, ?2, ?3, 5000)",
                params![t, L + 1_800, G],
            )?;
            Ok(())
        })
        .unwrap();
        let a = adventure(&db, FLAVOR, id as u32).unwrap().unwrap();
        assert_eq!(a.title, "Eastern Plaguelands & Stratholme");
        assert_eq!(a.tally.xp, Some(4_000.0));
    }

    #[test]
    fn neighbours_latest_and_flavors() {
        let db = Db::open_in_memory().unwrap();
        let a = character(&db, FLAVOR, "Thrandor", "Thrandor");
        let b = character(&db, FLAVOR, "Velyra", "Velyra");
        let other = character(&db, "_retail_", "Thrandor", "Thrandor");
        let adv = |c, login| {
            adventure_row(
                &db,
                c,
                &Adv {
                    login,
                    logout: login + 60,
                    money: (G, G),
                    level: (1, 1),
                    xp: 0,
                    events: &[],
                },
            )
        };
        let first = adv(a, L);
        let middle = adv(b, L + 100);
        let last = adv(a, L + 200);
        let elsewhere = adv(other, L + 300);

        let m = adventure(&db, FLAVOR, middle as u32).unwrap().unwrap();
        assert_eq!(m.prev.map(|p| p.id), Some(first as u32));
        assert_eq!(
            m.next.as_ref().map(|n| (n.id, n.name.as_str())),
            Some((last as u32, "Thrandor"))
        );
        assert_eq!(
            latest(&db, FLAVOR).unwrap(),
            Some(last as u32),
            "not the other flavor's"
        );
        assert!(adventure(&db, FLAVOR, elsewhere as u32).unwrap().is_none());
        assert_eq!(
            adventure(&db, FLAVOR, first as u32).unwrap().unwrap().title,
            "Thrandor's session"
        );
    }

    #[test]
    fn notes_are_trimmed_capped_and_cleared() {
        let db = Db::open_in_memory().unwrap();
        let t = character(&db, FLAVOR, "Thrandor", "Thrandor");
        let id = adventure_row(
            &db,
            t,
            &Adv {
                login: L,
                logout: L + 60,
                money: (G, G),
                level: (1, 1),
                xp: 0,
                events: &[],
            },
        ) as u32;
        let note = |db: &Db| adventure(db, FLAVOR, id).unwrap().unwrap().note;

        set_note(&db, FLAVOR, id, "  Ding at last.  ").unwrap();
        assert_eq!(note(&db).as_deref(), Some("Ding at last."));
        set_note(&db, FLAVOR, id, &"é".repeat(3000)).unwrap();
        assert_eq!(note(&db).unwrap().chars().count(), NOTE_MAX);
        set_note(&db, FLAVOR, id, "   ").unwrap();
        assert_eq!(note(&db), None);
        assert!(
            set_note(&db, "_retail_", id, "x").is_err(),
            "not this flavor's"
        );
    }

    #[test]
    fn gold_reads_like_the_ui() {
        assert_eq!(gold_text(18_280_000), "1,828g");
        assert_eq!(gold_text(-60_000), "−6g");
        assert_eq!(gold_text(4_700), "47s");
        assert_eq!(gold_text(9), "9c");
    }
}
