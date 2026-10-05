//! The Ledger (spec v0.2-addon §5, `gold.html`): gold over time per
//! character and for the account, three tiles, the journal of adventures,
//! and CSV exports of both. Read-only queries over what ingest stored
//! (migration 004).
//!
//! Days are calendar days in the user's time zone: a character's value for a
//! day is its money at its last point before that day ended, carried forward
//! between logouts. Mail senders and subjects are never read here, so they
//! can't reach an export.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Days, FixedOffset, NaiveDate, TimeZone, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::backup::export::check_destination;
use crate::db::Db;
use crate::error::AppResult;
use crate::fsx::atomic::atomic_replace;

const DAY: i64 = 86_400;
/// Characters drawn on their own; the rest are summed into "N others".
const TOP: usize = 4;
const JOURNAL_ROWS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LedgerRange {
    Week,
    Month,
    Quarter,
    All,
}

impl LedgerRange {
    fn days(self) -> Option<u64> {
        match self {
            Self::Week => Some(7),
            Self::Month => Some(30),
            Self::Quarter => Some(90),
            Self::All => None,
        }
    }
}

/// Money is copper, as `f64` so TypeScript can hold it safely (specta sends
/// no `i64`). Times are RFC 3339, UTC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Ledger {
    /// The earliest gold point ("from every logout since 2 Sep"); `None`
    /// until the addon has written anything.
    pub since: Option<String>,
    pub tiles: Tiles,
    pub chart: Chart,
    pub journal: Vec<JournalEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Tiles {
    /// Every character's latest money, added up.
    pub account_gold: f64,
    pub characters: u32,
    /// What the account gained (or lost) over 30 and 7 days. A character
    /// first seen inside the window counts from its first point, so a new
    /// character's starting gold isn't "earned".
    pub last_30_days: f64,
    pub this_week: f64,
    pub best_earner: Option<Earner>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Earner {
    pub character_id: u32,
    pub name: String,
    pub gained: f64,
    /// Adventures in the same 30 days.
    pub sessions: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Chart {
    /// One per calendar day, oldest first, "2026-10-04".
    pub days: Vec<String>,
    /// The top characters by gold now, then "N others" summed, if any.
    pub series: Vec<Series>,
    /// Every character added up, per day.
    pub account: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Series {
    /// `None` for the "others" line.
    pub character_id: Option<u32>,
    pub name: String,
    /// How many characters the "others" line adds up; 1 for a character.
    pub count: u32,
    /// Per day; `None` before the character's first point.
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct JournalEntry {
    pub adventure_id: u32,
    pub character_id: u32,
    pub name: String,
    pub login: String,
    pub logout: Option<String>,
    pub played_secs: Option<u32>,
    pub gold_delta: Option<f64>,
    /// The level reached, if the character levelled.
    pub level: Option<u32>,
    pub of_note: Option<OfNote>,
}

/// The journal's "Of note" (spec §5): a level-up, else the zone with the most
/// time and the quest count, else the biggest gain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct OfNote {
    pub text: String,
    /// For an item: its quality, for the letter tile's colour.
    pub quality: Option<u32>,
    /// For an item: its icon's FileDataID, for `icon://`.
    #[serde(default)]
    pub icon: Option<u32>,
}

/// One character's points, oldest first.
struct Character {
    id: i64,
    name: String,
    points: Vec<(i64, i64)>,
}

impl Character {
    fn latest(&self) -> Option<i64> {
        self.points.last().map(|p| p.1)
    }

    /// Money at its last point before `t`.
    fn before(&self, t: i64) -> Option<i64> {
        let i = self.points.partition_point(|p| p.0 < t);
        i.checked_sub(1).map(|i| self.points[i].1)
    }

    /// Gained since `t`: from its value then, or from its first point if it
    /// was first seen later.
    fn gained_since(&self, t: i64) -> i64 {
        let Some(now) = self.latest() else { return 0 };
        now - self.before(t).unwrap_or(self.points[0].1)
    }
}

fn rfc3339(t: i64) -> String {
    DateTime::<Utc>::from_timestamp(t, 0)
        .unwrap_or_default()
        .to_rfc3339()
}

fn display_name(name: String, surname: Option<String>) -> String {
    match surname {
        Some(s) if !s.is_empty() => format!("{name} {s}"),
        _ => name,
    }
}

fn characters(db: &Db, flavor: &str) -> AppResult<Vec<Character>> {
    db.with_conn(|c| {
        let mut chars: Vec<Character> = c
            .prepare("SELECT id, name, surname FROM characters WHERE flavor = ?1 ORDER BY id")?
            .query_map([flavor], |r| {
                Ok(Character {
                    id: r.get(0)?,
                    name: display_name(r.get(1)?, r.get(2)?),
                    points: Vec::new(),
                })
            })?
            .collect::<Result<_, _>>()?;
        let mut stmt =
            c.prepare("SELECT at, money FROM gold_points WHERE character_id = ?1 ORDER BY at")?;
        for ch in &mut chars {
            ch.points = stmt
                .query_map([ch.id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
        }
        Ok(chars)
    })
}

/// Midnight starting `day`, in `tz`, as Unix seconds.
fn day_start(day: NaiveDate, tz: &FixedOffset) -> i64 {
    tz.from_local_datetime(&day.and_hms_opt(0, 0, 0).expect("midnight exists"))
        .single()
        .map_or(0, |d| d.timestamp())
}

fn local_day(t: i64, tz: &FixedOffset) -> NaiveDate {
    DateTime::<Utc>::from_timestamp(t, 0)
        .unwrap_or_default()
        .with_timezone(tz)
        .date_naive()
}

/// The chart's days, and when each ends (the next midnight): a day's value
/// is the money at the last point before its end. Empty without any points.
fn days(
    chars: &[Character],
    range: LedgerRange,
    now: i64,
    tz: &FixedOffset,
) -> (Vec<NaiveDate>, Vec<i64>) {
    let today = local_day(now, tz);
    let Some(first) = chars
        .iter()
        .filter_map(|c| c.points.first())
        .map(|p| p.0)
        .min()
    else {
        return (Vec::new(), Vec::new());
    };
    let start = match range.days() {
        Some(n) => today - Days::new(n - 1),
        None => local_day(first, tz).min(today),
    };
    let days: Vec<NaiveDate> = start.iter_days().take_while(|d| *d <= today).collect();
    let ends = days
        .iter()
        .map(|d| day_start(*d + Days::new(1), tz))
        .collect();
    (days, ends)
}

fn chart(chars: &[Character], range: LedgerRange, now: i64, tz: &FixedOffset) -> Chart {
    let (days, ends) = days(chars, range, now, tz);
    let per_day =
        |c: &Character| -> Vec<Option<i64>> { ends.iter().map(|e| c.before(*e)).collect() };

    let mut ranked: Vec<&Character> = chars.iter().filter(|c| !c.points.is_empty()).collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.latest()));
    // Grouping one character as "1 others" would only hide its name.
    let shown = if ranked.len() == TOP + 1 {
        TOP + 1
    } else {
        TOP
    };
    let (top, rest) = ranked.split_at(ranked.len().min(shown));

    let money = |v: Option<i64>| v.map(|m| m as f64);
    let mut series: Vec<Series> = top
        .iter()
        .map(|c| Series {
            character_id: Some(c.id as u32),
            name: c.name.clone(),
            count: 1,
            values: per_day(c).into_iter().map(money).collect(),
        })
        .collect();
    if !rest.is_empty() {
        let mut sum: Vec<Option<i64>> = vec![None; days.len()];
        for c in rest {
            for (s, v) in sum.iter_mut().zip(per_day(c)) {
                if let Some(v) = v {
                    *s = Some(s.unwrap_or(0) + v);
                }
            }
        }
        series.push(Series {
            character_id: None,
            name: format!("{} others", rest.len()),
            count: rest.len() as u32,
            values: sum.into_iter().map(money).collect(),
        });
    }
    let account = ends
        .iter()
        .map(|e| chars.iter().filter_map(|c| c.before(*e)).sum::<i64>() as f64)
        .collect();
    Chart {
        days: days
            .iter()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .collect(),
        series,
        account,
    }
}

fn tiles(db: &Db, chars: &[Character], now: i64) -> AppResult<Tiles> {
    let seen: Vec<&Character> = chars.iter().filter(|c| !c.points.is_empty()).collect();
    let month = now - 30 * DAY;
    let best = seen
        .iter()
        .map(|c| (c, c.gained_since(month)))
        .filter(|(_, g)| *g > 0)
        .max_by_key(|(_, g)| *g);
    let best_earner = match best {
        Some((c, gained)) => Some(Earner {
            character_id: c.id as u32,
            name: c.name.clone(),
            gained: gained as f64,
            sessions: db.with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT count(*) FROM adventures WHERE character_id = ?1 AND login >= ?2",
                    params![c.id, month],
                    |r| r.get(0),
                )?)
            })?,
        }),
        None => None,
    };
    Ok(Tiles {
        account_gold: seen.iter().filter_map(|c| c.latest()).sum::<i64>() as f64,
        characters: seen.len() as u32,
        last_30_days: seen.iter().map(|c| c.gained_since(month)).sum::<i64>() as f64,
        this_week: seen
            .iter()
            .map(|c| c.gained_since(now - 7 * DAY))
            .sum::<i64>() as f64,
        best_earner,
    })
}

struct Event {
    at: i64,
    kind: String,
    data: serde_json::Value,
}

fn int(v: &serde_json::Value, key: &str) -> Option<i64> {
    v.get(key).and_then(serde_json::Value::as_i64)
}

fn text<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(serde_json::Value::as_str)
}

/// Spec §5: a level-up, else the zone with the most time (with the quest
/// count), else the biggest gain.
fn of_note(
    db: &Db,
    level: Option<u32>,
    events: &[Event],
    logout: Option<i64>,
) -> AppResult<Option<OfNote>> {
    if let Some(level) = level {
        return Ok(Some(OfNote {
            text: format!("Reached level {level}"),
            quality: None,
            icon: None,
        }));
    }

    // Time in each zone: from its zone event to the next one (or logout).
    let zones: Vec<&Event> = events.iter().filter(|e| e.kind == "zone").collect();
    let mut time: HashMap<&str, i64> = HashMap::new();
    for (i, z) in zones.iter().enumerate() {
        let until = zones.get(i + 1).map(|n| n.at).or(logout).unwrap_or(z.at);
        if let Some(name) = text(&z.data, "zone") {
            *time.entry(name).or_default() += (until - z.at).max(0);
        }
    }
    let quests = events.iter().filter(|e| e.kind == "quest").count();
    if let Some((zone, _)) = time
        .iter()
        .max_by_key(|(name, t)| (**t, std::cmp::Reverse(**name)))
    {
        let text = match quests {
            0 => zone.to_string(),
            1 => format!("{zone} · 1 quest"),
            n => format!("{zone} · {n} quests"),
        };
        return Ok(Some(OfNote {
            text,
            quality: None,
            icon: None,
        }));
    }

    let gain = events
        .iter()
        .filter(|e| e.kind == "gain")
        .filter_map(|e| Some((int(&e.data, "item")?, int(&e.data, "count").unwrap_or(1))))
        .max_by_key(|(_, n)| *n);
    if let Some((item, count)) = gain {
        type Info = (Option<String>, Option<i64>, Option<i64>);
        let info: Option<Info> = db.with_conn(|c| {
            Ok(c.query_row(
                "SELECT name, quality, icon_file_id FROM items WHERE item_id = ?1",
                [item],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
        })?;
        let (name, quality, icon) = info.unwrap_or((None, None, None));
        let name = name.unwrap_or_else(|| format!("Item {item}"));
        return Ok(Some(OfNote {
            text: if count > 1 {
                format!("{name} ×{count}")
            } else {
                name
            },
            quality: quality.map(|q| q as u32),
            icon: icon.and_then(|i| u32::try_from(i).ok()),
        }));
    }
    Ok(None)
}

fn journal(db: &Db, chars: &[Character], since: Option<i64>) -> AppResult<Vec<JournalEntry>> {
    let names: HashMap<i64, &str> = chars.iter().map(|c| (c.id, c.name.as_str())).collect();
    type Row = (
        i64,
        i64,
        i64,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    );
    let rows: Vec<Row> = db.with_conn(|c| {
        let ids: Vec<String> = chars.iter().map(|c| c.id.to_string()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        // The ids are our own integers, so joining them into the query is safe.
        let sql = format!(
            "SELECT id, character_id, login, logout, start_money, end_money, start_level, end_level
             FROM adventures WHERE character_id IN ({}) AND login >= ?1
             ORDER BY login DESC LIMIT {JOURNAL_ROWS}",
            ids.join(",")
        );
        let rows = c
            .prepare(&sql)?
            .query_map([since.unwrap_or(i64::MIN)], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    })?;

    let mut out = Vec::with_capacity(rows.len());
    for (id, character, login, logout, start_money, end_money, start_level, end_level) in rows {
        let events: Vec<Event> = db.with_conn(|c| {
            Ok(c.prepare(
                "SELECT at, kind, data FROM adventure_events WHERE adventure_id = ?1 ORDER BY seq",
            )?
            .query_map([id], |r| {
                let data: String = r.get(2)?;
                Ok(Event {
                    at: r.get(0)?,
                    kind: r.get(1)?,
                    data: serde_json::from_str(&data).unwrap_or_default(),
                })
            })?
            .collect::<Result<_, _>>()?)
        })?;
        let level = match (start_level, end_level) {
            (Some(s), Some(e)) if e > s => Some(e as u32),
            _ => None,
        };
        out.push(JournalEntry {
            adventure_id: id as u32,
            character_id: character as u32,
            name: names
                .get(&character)
                .copied()
                .unwrap_or_default()
                .to_string(),
            login: rfc3339(login),
            logout: logout.map(rfc3339),
            played_secs: logout.map(|l| (l - login).max(0) as u32),
            gold_delta: start_money.zip(end_money).map(|(s, e)| (e - s) as f64),
            level,
            of_note: of_note(db, level, &events, logout)?,
        });
    }
    Ok(out)
}

/// The whole Ledger for `flavor` over `range`, as of `now` in time zone `tz`.
pub fn ledger(
    db: &Db,
    flavor: &str,
    range: LedgerRange,
    now: i64,
    tz: &FixedOffset,
) -> AppResult<Ledger> {
    let chars = characters(db, flavor)?;
    let first = chars
        .iter()
        .filter_map(|c| c.points.first())
        .map(|p| p.0)
        .min();
    let chart = chart(&chars, range, now, tz);
    let since = range
        .days()
        .map(|n| day_start(local_day(now, tz) - Days::new(n - 1), tz));
    Ok(Ledger {
        since: first.map(rfc3339),
        tiles: tiles(db, &chars, now)?,
        journal: journal(db, &chars, since)?,
        chart,
    })
}

// CSV ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LedgerExport {
    /// The chart as a table: a row per day, a column per character.
    Gold,
    /// The journal: a row per adventure.
    Journal,
}

/// A text cell, RFC 4180 quoted. A spreadsheet runs a cell starting with
/// `=`, `+`, `-`, `@`, a tab or a CR as a formula, and item names, zones,
/// quest titles and the user's notes can start that way: prefix a `'`.
fn csv_text(s: &str) -> String {
    let guarded = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

/// Copper as gold with its copper kept: 1234567 → 123.4567.
fn csv_gold(copper: Option<f64>) -> String {
    copper.map_or_else(String::new, |c| format!("{:.4}", c / 10_000.0))
}

fn local_time(iso: &str, tz: &FixedOffset) -> String {
    DateTime::parse_from_rfc3339(iso)
        .map(|d| d.with_timezone(tz).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// The CSV text of an export. The gold table has every character as its own
/// column (not grouped into "others"). The journal includes the user's own
/// notes; mail senders and subjects are never read here, so they can't
/// reach an export.
fn csv(
    db: &Db,
    flavor: &str,
    range: LedgerRange,
    kind: LedgerExport,
    now: i64,
    tz: &FixedOffset,
) -> AppResult<String> {
    let mut out = String::new();
    match kind {
        LedgerExport::Gold => {
            let chars: Vec<Character> = characters(db, flavor)?
                .into_iter()
                .filter(|c| !c.points.is_empty())
                .collect();
            let (days, ends) = days(&chars, range, now, tz);
            let mut header = vec![csv_text("Date"), csv_text("Account (gold)")];
            header.extend(
                chars
                    .iter()
                    .map(|c| csv_text(&format!("{} (gold)", c.name))),
            );
            let _ = write!(out, "{}\r\n", header.join(","));
            for (day, end) in days.iter().zip(&ends) {
                let values: Vec<Option<i64>> = chars.iter().map(|c| c.before(*end)).collect();
                let account = values.iter().flatten().sum::<i64>() as f64;
                let mut row = vec![
                    csv_text(&day.format("%Y-%m-%d").to_string()),
                    csv_gold(Some(account)),
                ];
                row.extend(values.iter().map(|v| csv_gold(v.map(|m| m as f64))));
                let _ = write!(out, "{}\r\n", row.join(","));
            }
        }
        LedgerExport::Journal => {
            let header = [
                "Login",
                "Logout",
                "Character",
                "Level reached",
                "Played (minutes)",
                "Gold change",
                "Of note",
                "Note",
            ];
            let header: Vec<String> = header.iter().map(|h| csv_text(h)).collect();
            let _ = write!(out, "{}\r\n", header.join(","));
            let ledger = ledger(db, flavor, range, now, tz)?;
            let notes: HashMap<i64, String> = db.with_conn(|c| {
                Ok(
                    c.prepare("SELECT id, note FROM adventures WHERE note IS NOT NULL")?
                        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                        .collect::<Result<_, _>>()?,
                )
            })?;
            for e in &ledger.journal {
                let row = [
                    csv_text(&local_time(&e.login, tz)),
                    csv_text(
                        &e.logout
                            .as_deref()
                            .map(|l| local_time(l, tz))
                            .unwrap_or_default(),
                    ),
                    csv_text(&e.name),
                    e.level.map(|l| l.to_string()).unwrap_or_default(),
                    e.played_secs
                        .map(|s| (s / 60).to_string())
                        .unwrap_or_default(),
                    csv_gold(e.gold_delta),
                    csv_text(e.of_note.as_ref().map_or("", |n| n.text.as_str())),
                    csv_text(
                        notes
                            .get(&i64::from(e.adventure_id))
                            .map_or("", String::as_str),
                    ),
                ];
                let _ = write!(out, "{}\r\n", row.join(","));
            }
        }
    }
    Ok(out)
}

/// Writes a CSV export to `dest` (from the save dialog). Refused inside the
/// game or backup folders, like the zip export.
#[allow(clippy::too_many_arguments)]
pub fn export(
    db: &Db,
    flavor: &str,
    range: LedgerRange,
    kind: LedgerExport,
    dest: &Path,
    forbidden: &[PathBuf],
    now: i64,
    tz: &FixedOffset,
) -> AppResult<PathBuf> {
    let dest = check_destination(dest, forbidden, "csv")?;
    // A BOM so Excel reads the UTF-8 names (accents, surnames) correctly.
    let text = format!("\u{feff}{}", csv(db, flavor, range, kind, now, tz)?);
    atomic_replace(&dest, text.as_bytes())?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;

    const FLAVOR: &str = "_classic_beta_";
    /// Mon 5 Oct 2026, 12:00 UTC.
    const NOW: i64 = 1_791_201_600;
    const H: i64 = 3_600;

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn character(db: &Db, dir: &str, name: &str, surname: Option<&str>) -> i64 {
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO characters (flavor, account, group_dir, char_dir, name, surname, first_seen, last_seen)
                 VALUES (?1, 'ACCOUNT1', '70', ?2, ?3, ?4, 0, 0)",
                params![FLAVOR, dir, name, surname],
            )?;
            Ok(c.last_insert_rowid())
        })
        .unwrap()
    }

    fn point(db: &Db, id: i64, at: i64, money: i64) {
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO gold_points (character_id, at, money) VALUES (?1, ?2, ?3)",
                params![id, at, money],
            )?;
            Ok(())
        })
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    fn adventure(
        db: &Db,
        id: i64,
        login: i64,
        logout: i64,
        money: (i64, i64),
        level: (i64, i64),
        events: &[(i64, &str, &str)],
        note: Option<&str>,
    ) -> i64 {
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO adventures (character_id, login, logout, start_money, end_money, start_level, end_level, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![id, login, logout, money.0, money.1, level.0, level.1, note],
            )?;
            let adv = c.last_insert_rowid();
            for (seq, (at, kind, data)) in events.iter().enumerate() {
                c.execute(
                    "INSERT INTO adventure_events (adventure_id, seq, at, kind, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![adv, seq as i64, at, kind, data],
                )?;
            }
            Ok(adv)
        })
        .unwrap()
    }

    #[test]
    fn the_chart_carries_gold_forward_by_day() {
        let db = Db::open_in_memory().unwrap();
        let t = character(&db, "Thrandor-Vargur", "Thrandor", Some("Vargur"));
        let v = character(&db, "Velyra", "Velyra", None);
        point(&db, t, NOW - 3 * DAY, 100);
        point(&db, t, NOW - DAY, 300);
        point(&db, v, NOW - 10 * DAY, 50);

        let l = ledger(&db, FLAVOR, LedgerRange::Week, NOW, &utc()).unwrap();
        assert_eq!(l.chart.days.first().unwrap(), "2026-09-29");
        assert_eq!(l.chart.days.last().unwrap(), "2026-10-05");
        let thrandor = l
            .chart
            .series
            .iter()
            .find(|s| s.character_id == Some(t as u32))
            .unwrap();
        assert_eq!(thrandor.name, "Thrandor Vargur");
        assert_eq!(
            thrandor.values,
            [
                None,
                None,
                None,
                Some(100.0),
                Some(100.0),
                Some(300.0),
                Some(300.0)
            ],
            "nothing before its first point, then carried forward"
        );
        let velyra = l
            .chart
            .series
            .iter()
            .find(|s| s.character_id == Some(v as u32))
            .unwrap();
        assert_eq!(
            velyra.values,
            [Some(50.0); 7],
            "a point before the range carries in"
        );
        assert_eq!(
            l.chart.account,
            [50.0, 50.0, 50.0, 150.0, 150.0, 350.0, 350.0]
        );
        assert_eq!(l.since, Some(rfc3339(NOW - 10 * DAY)));

        let all = ledger(&db, FLAVOR, LedgerRange::All, NOW, &utc()).unwrap();
        assert_eq!(all.chart.days.len(), 11, "from the first point's day");
    }

    #[test]
    fn the_top_four_are_drawn_and_the_rest_summed() {
        let db = Db::open_in_memory().unwrap();
        for (i, name) in ["A", "B", "C", "D", "E", "F"].iter().enumerate() {
            let id = character(&db, name, name, None);
            point(&db, id, NOW - H, 600 - 100 * i as i64);
        }
        let l = ledger(&db, FLAVOR, LedgerRange::Week, NOW, &utc()).unwrap();
        let names: Vec<&str> = l.chart.series.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["A", "B", "C", "D", "2 others"]);
        let others = l.chart.series.last().unwrap();
        assert_eq!((others.character_id, others.count), (None, 2));
        assert_eq!(
            others.values.last().copied().flatten(),
            Some(300.0),
            "E + F"
        );

        // With five there's no "1 others": all five are drawn.
        let db = Db::open_in_memory().unwrap();
        for name in ["A", "B", "C", "D", "E"] {
            let id = character(&db, name, name, None);
            point(&db, id, NOW - H, 100);
        }
        let l = ledger(&db, FLAVOR, LedgerRange::Week, NOW, &utc()).unwrap();
        assert_eq!(l.chart.series.len(), 5);
        assert!(l.chart.series.iter().all(|s| s.count == 1));
    }

    #[test]
    fn tiles_count_gains_not_new_characters_starting_gold() {
        let db = Db::open_in_memory().unwrap();
        let t = character(&db, "Thrandor", "Thrandor", None);
        let n = character(&db, "Newbie", "Newbie", None);
        point(&db, t, NOW - 40 * DAY, 100);
        point(&db, t, NOW - 20 * DAY, 150);
        point(&db, t, NOW - 2 * DAY, 400);
        point(&db, n, NOW - 5 * DAY, 1000);
        point(&db, n, NOW - DAY, 1100);
        adventure(
            &db,
            t,
            NOW - 20 * DAY,
            NOW - 20 * DAY + H,
            (100, 150),
            (10, 10),
            &[],
            None,
        );
        adventure(
            &db,
            t,
            NOW - 2 * DAY,
            NOW - 2 * DAY + H,
            (150, 400),
            (10, 10),
            &[],
            None,
        );
        adventure(
            &db,
            t,
            NOW - 40 * DAY,
            NOW - 40 * DAY + H,
            (0, 100),
            (10, 10),
            &[],
            None,
        );

        let tiles = ledger(&db, FLAVOR, LedgerRange::Month, NOW, &utc())
            .unwrap()
            .tiles;
        assert_eq!(tiles.account_gold, 1500.0);
        assert_eq!(tiles.characters, 2);
        assert_eq!(
            tiles.last_30_days,
            300.0 + 100.0,
            "Newbie counts from its first point"
        );
        assert_eq!(tiles.this_week, 250.0 + 100.0);
        let best = tiles.best_earner.unwrap();
        assert_eq!(
            (best.character_id, best.gained, best.sessions),
            (t as u32, 300.0, 2)
        );
    }

    #[test]
    fn the_journal_says_what_was_of_note() {
        let db = Db::open_in_memory().unwrap();
        let t = character(&db, "Thrandor", "Thrandor", None);
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO items (item_id, name, quality, seen_at) VALUES (14047, 'Runecloth', 1, 0)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let start = NOW - 10 * DAY;
        let levelled = adventure(
            &db,
            t,
            start,
            start + H,
            (100, 150),
            (10, 11),
            &[(start + 60, "zone", r#"{"zone":"Westfall"}"#)],
            None,
        );
        let s2 = start + DAY;
        let questing = adventure(
            &db,
            t,
            s2,
            s2 + 4000,
            (150, 120),
            (11, 11),
            &[
                (s2, "zone", r#"{"zone":"Westfall"}"#),
                (
                    s2 + 600,
                    "zone",
                    r#"{"zone":"The Deadmines","instance":true}"#,
                ),
                (s2 + 700, "quest", r#"{"id":176}"#),
                (s2 + 800, "quest", r#"{"id":177}"#),
            ],
            None,
        );
        let s3 = start + 2 * DAY;
        let looting = adventure(
            &db,
            t,
            s3,
            s3 + H,
            (120, 200),
            (11, 11),
            &[
                (s3 + 1, "gain", r#"{"item":2589,"count":3}"#),
                (s3 + 2, "gain", r#"{"item":14047,"count":20}"#),
            ],
            None,
        );
        let s4 = start + 3 * DAY;
        let quiet = adventure(&db, t, s4, s4 + H, (200, 200), (11, 11), &[], None);

        let j = ledger(&db, FLAVOR, LedgerRange::Month, NOW, &utc())
            .unwrap()
            .journal;
        let ids: Vec<u32> = j.iter().map(|e| e.adventure_id).collect();
        assert_eq!(
            ids,
            [quiet, looting, questing, levelled].map(|i| i as u32),
            "newest first"
        );

        let note = |i: usize| j[i].of_note.as_ref().map(|n| (n.text.as_str(), n.quality));
        assert_eq!(note(3), Some(("Reached level 11", None)));
        assert_eq!(j[3].level, Some(11));
        assert_eq!(note(2), Some(("The Deadmines · 2 quests", None)));
        assert_eq!(j[2].gold_delta, Some(-30.0));
        assert_eq!(j[2].played_secs, Some(4000));
        assert_eq!(note(1), Some(("Runecloth ×20", Some(1))));
        assert_eq!(note(0), None);
        assert_eq!(j[0].level, None);
    }

    #[test]
    fn csv_guards_formulas_and_never_has_mail() {
        let db = Db::open_in_memory().unwrap();
        let t = character(&db, "Thrandor", "Thrandor", None);
        point(&db, t, NOW - DAY, 2_000_000);
        point(&db, t, NOW - H, 1_999_914);
        let s = NOW - DAY;
        adventure(
            &db,
            t,
            s,
            s + H,
            (2_000_000, 1_999_914),
            (10, 10),
            &[(s, "zone", r#"{"zone":"+Odd \"zone\""}"#)],
            Some("=HYPERLINK(\"http://x\")"),
        );
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO char_mail (character_id, idx, sender, subject, as_of)
                 VALUES (?1, 1, 'Mailsender', 'Secret subject', 0)",
                [t],
            )?;
            Ok(())
        })
        .unwrap();

        let journal = csv(
            &db,
            FLAVOR,
            LedgerRange::Week,
            LedgerExport::Journal,
            NOW,
            &utc(),
        )
        .unwrap();
        assert!(
            journal.contains(r#""'=HYPERLINK(""http://x"")""#),
            "{journal}"
        );
        assert!(journal.contains(r#""'+Odd ""zone"""#), "{journal}");
        assert!(
            journal.contains(",-0.0086,"),
            "numbers aren't prefixed: {journal}"
        );
        let gold = csv(
            &db,
            FLAVOR,
            LedgerRange::Week,
            LedgerExport::Gold,
            NOW,
            &utc(),
        )
        .unwrap();
        assert!(gold.starts_with("\"Date\",\"Account (gold)\",\"Thrandor (gold)\"\r\n"));
        assert!(
            gold.contains("\"2026-10-05\",199.9914,199.9914\r\n"),
            "{gold}"
        );
        for text in [&journal, &gold] {
            assert!(!text.contains("Mailsender") && !text.contains("Secret subject"));
        }
    }

    /// The Account gold tile and the chart's last day are the same gold:
    /// every character's latest point (S2: the mismatch seen was the mock's).
    #[test]
    fn the_tile_total_is_the_charts_last_day() {
        let db = Db::open_in_memory().unwrap();
        for (i, name) in ["A", "B", "C", "D", "E", "F"].iter().enumerate() {
            let id = character(&db, name, name, None);
            let i = i as i64;
            point(&db, id, NOW - (40 - i) * DAY, 1_000 * (i + 1));
            point(&db, id, NOW - (10 - i) * DAY, 2_345 * (i + 1));
            point(&db, id, NOW - (i + 1) * H, 6_789 * (i + 1) + 7);
        }
        for range in [
            LedgerRange::Week,
            LedgerRange::Month,
            LedgerRange::Quarter,
            LedgerRange::All,
        ] {
            let l = ledger(&db, FLAVOR, range, NOW, &utc()).unwrap();
            assert_eq!(
                l.chart.account.last().copied(),
                Some(l.tiles.account_gold),
                "{range:?}"
            );
            let lines: f64 = l
                .chart
                .series
                .iter()
                .filter_map(|s| *s.values.last().unwrap())
                .sum();
            assert_eq!(
                lines, l.tiles.account_gold,
                "{range:?}: the lines add up to it too"
            );
        }
    }

    #[test]
    fn export_writes_a_csv_but_never_into_the_game_folder() {
        let db = Db::open_in_memory().unwrap();
        let t = character(&db, "Thrandor", "Thrandor", None);
        point(&db, t, NOW - H, 100);
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("World of Warcraft");
        std::fs::create_dir_all(&game).unwrap();
        let forbidden = vec![game.clone()];

        let refused = export(
            &db,
            FLAVOR,
            LedgerRange::Week,
            LedgerExport::Gold,
            &game.join("gold.csv"),
            &forbidden,
            NOW,
            &utc(),
        );
        assert!(matches!(refused, Err(AppError::BadDestination(_))));

        let written = export(
            &db,
            FLAVOR,
            LedgerRange::Week,
            LedgerExport::Gold,
            &dir.path().join("gold"),
            &forbidden,
            NOW,
            &utc(),
        )
        .unwrap();
        assert_eq!(written, dir.path().join("gold.csv"));
        assert!(std::fs::read_to_string(&written)
            .unwrap()
            .starts_with("\u{feff}\"Date\""));
    }
}
