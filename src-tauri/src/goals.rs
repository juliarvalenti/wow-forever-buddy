//! Goals (G1, IMPLEMENTING §21): "Brannic to level 55", "500g for
//! Fizzwick's mount", optionally by a date. Two kinds: a character reaching
//! a level, or a character (or the whole account) holding an amount of gold.
//! Item goals are Lists. Set in the app, or proposed by an agent and approved
//! through the P2 queue.
//!
//! Progress is never stored: it's read from what ingest already keeps (level
//! and XP, gold snapshots), so it's "as of logout". `mark_reached` stamps a
//! goal done the first time an ingest finds it reached. Goals only display;
//! nothing acts on them.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::sv::{LuaTable, LuaValue};

/// Open goals in a flavor, so the briefing slot stays small.
const MAX_OPEN: i64 = 20;
const MAX_LEVEL: i64 = 100;
/// 10 million gold, in copper.
const MAX_COPPER: i64 = 10_000_000 * COPPER_PER_GOLD;
const COPPER_PER_GOLD: i64 = 10_000;
pub const MAX_LABEL: usize = 24;
/// Reached goals stay listed this long ("done Thu").
const DONE_SHOWN: i64 = 3 * 86_400;
const DAY: f64 = 86_400.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    Level,
    /// The target is in copper.
    Gold,
}

impl GoalKind {
    fn as_str(self) -> &'static str {
        match self {
            GoalKind::Level => "level",
            GoalKind::Gold => "gold",
        }
    }

    fn parse(s: &str) -> GoalKind {
        if s == "gold" {
            GoalKind::Gold
        } else {
            GoalKind::Level
        }
    }
}

/// What the app's form (or an approved proposal) sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct NewGoal {
    /// `None`: the whole account (gold only).
    pub character_id: Option<u32>,
    pub kind: GoalKind,
    /// A level, or copper.
    pub target: f64,
    /// Gold only: "for the mount".
    pub label: Option<String>,
    /// Unix seconds; in the future.
    pub by: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Goal {
    pub id: u32,
    /// `None` for an account gold goal.
    pub character_id: Option<u32>,
    pub character: Option<String>,
    /// File token, lowercase, for the class colour.
    pub class: Option<String>,
    pub kind: GoalKind,
    /// A level, or copper.
    pub target: f64,
    pub label: Option<String>,
    /// The value when it was set, where the bar starts.
    pub start: f64,
    /// The value now, as of the last logout: the level with its XP fraction
    /// (52.4), or copper. `None` before anything's known.
    pub current: Option<f64>,
    /// Levels or copper a day since the goal was set, once it's been a day
    /// and there's been progress.
    pub per_day: Option<f64>,
    /// When the data behind `current` was written (RFC 3339).
    pub as_of: Option<String>,
    /// RFC 3339.
    pub by: Option<String>,
    /// "app" or "agent:<client name>", as a claim.
    pub producer: String,
    pub created_at: String,
    /// When an ingest first found it reached (RFC 3339).
    pub done_at: Option<String>,
}

fn rfc3339(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0)
        .unwrap_or_default()
        .to_rfc3339()
}

fn invalid(why: &str) -> AppError {
    AppError::InvalidSettings(format!("goal: {why}"))
}

/// A goal's value now and when it was written: a character's level with its
/// XP fraction, its gold, or the account's gold (every character's latest).
fn current(
    c: &Connection,
    flavor: &str,
    character: Option<i64>,
    kind: GoalKind,
) -> AppResult<(Option<f64>, Option<i64>)> {
    let latest = "SELECT money, level, xp, xp_max, at FROM char_snapshots
                  WHERE character_id = ?1 ORDER BY at DESC LIMIT 1";
    Ok(match (kind, character) {
        (GoalKind::Level, Some(id)) => {
            type Snap = (Option<i64>, Option<i64>, Option<i64>, i64);
            let snap: Option<Snap> = c
                .query_row(latest, [id], |r| {
                    Ok((r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .optional()?;
            let level: Option<i64> = c
                .query_row("SELECT level FROM characters WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()?
                .flatten();
            match snap {
                Some((Some(l), xp, max, at)) => {
                    let frac = match (xp, max) {
                        (Some(x), Some(m)) if m > 0 => (x as f64 / m as f64).clamp(0.0, 0.999),
                        _ => 0.0,
                    };
                    (Some(l as f64 + frac), Some(at))
                }
                Some((None, _, _, at)) => (level.map(|l| l as f64), Some(at)),
                None => (level.map(|l| l as f64), None),
            }
        }
        (GoalKind::Gold, Some(id)) => c
            .query_row(latest, [id], |r| {
                Ok((Some(r.get::<_, i64>(0)? as f64), Some(r.get(4)?)))
            })
            .optional()?
            .unwrap_or((None, None)),
        (GoalKind::Gold, None) => c.query_row(
            "SELECT sum(s.money), min(s.at) FROM char_snapshots s
             JOIN visible_characters ch ON ch.id = s.character_id
             WHERE ch.flavor = ?1
               AND s.at = (SELECT max(at) FROM char_snapshots WHERE character_id = s.character_id)",
            [flavor],
            |r| Ok((r.get::<_, Option<i64>>(0)?.map(|v| v as f64), r.get(1)?)),
        )?,
        (GoalKind::Level, None) => (None, None),
    })
}

fn name_of(c: &Connection, id: i64) -> AppResult<String> {
    Ok(
        c.query_row("SELECT name FROM characters WHERE id = ?1", [id], |r| {
            r.get(0)
        })?,
    )
}

/// "Brannic is already level 55.", "The account already has 3,500g": what
/// it is now, which may be past the target.
fn already(c: &Connection, character: Option<i64>, kind: GoalKind, now: f64) -> AppResult<String> {
    let who = match character {
        Some(id) => name_of(c, id)?,
        None => "The account".into(),
    };
    Ok(match kind {
        GoalKind::Level => format!("{who} is already level {}.", now.floor() as i64),
        GoalKind::Gold => format!(
            "{who} already has {}g.",
            thousands(now as i64 / COPPER_PER_GOLD)
        ),
    })
}

fn thousands(n: i64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Checks a goal against the limits and the app's data. Returns the target,
/// the label, the date and the value now.
fn check(
    c: &Connection,
    flavor: &str,
    g: &NewGoal,
    now: i64,
) -> AppResult<(i64, Option<String>, Option<i64>, f64)> {
    if !g.target.is_finite() || g.target.fract() != 0.0 {
        return Err(invalid("a whole number"));
    }
    let target = g.target as i64;
    match g.kind {
        GoalKind::Level if !(1..=MAX_LEVEL).contains(&target) => {
            return Err(invalid("a level of 1 to 100"))
        }
        GoalKind::Gold if !(1..=MAX_COPPER).contains(&target) => {
            return Err(invalid("1 to 10,000,000 gold"))
        }
        _ => {}
    }
    let character = match g.character_id {
        Some(id) => {
            let owned: Option<i64> = c
                .query_row(
                    "SELECT id FROM visible_characters WHERE id = ?1 AND flavor = ?2",
                    params![id, flavor],
                    |r| r.get(0),
                )
                .optional()?;
            Some(owned.ok_or_else(|| AppError::NotFound("that character".into()))?)
        }
        None if g.kind == GoalKind::Gold => None,
        None => return Err(invalid("a level goal is for one character")),
    };
    let label = match (&g.label, g.kind) {
        (None, _) => None,
        (Some(l), GoalKind::Gold) => {
            let l = l.trim();
            if l.is_empty() {
                None
            } else if l.chars().count() > MAX_LABEL || l.chars().any(char::is_control) {
                return Err(invalid("a label of up to 24 characters"));
            } else {
                Some(l.to_string())
            }
        }
        (Some(_), GoalKind::Level) => return Err(invalid("only gold goals have a label")),
    };
    let by = match g.by {
        None => None,
        Some(t) if t.is_finite() && (t as i64) > now => Some(t as i64),
        Some(_) => return Err(invalid("a date that hasn't passed")),
    };
    let open: i64 = c.query_row(
        "SELECT count(*) FROM goals WHERE flavor = ?1 AND archived_at IS NULL AND done_at IS NULL",
        [flavor],
        |r| r.get(0),
    )?;
    if open >= MAX_OPEN {
        return Err(invalid("at most 20 open goals"));
    }
    let (now_value, _) = current(c, flavor, character, g.kind)?;
    let start = now_value.unwrap_or(0.0);
    if start >= target as f64 {
        return Err(AppError::InvalidSettings(already(
            c, character, g.kind, start,
        )?));
    }
    Ok((target, label, by, start))
}

/// Adds a goal. `producer` is "app", or "agent:<client>" for an approved
/// proposal.
pub fn add(db: &Db, flavor: &str, g: &NewGoal, producer: &str, now: i64) -> AppResult<u32> {
    db.with_conn(|c| {
        let (target, label, by, start) = check(c, flavor, g, now)?;
        c.execute(
            "INSERT INTO goals (flavor, character_id, kind, target, label, start, by_at, producer, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![flavor, g.character_id, g.kind.as_str(), target, label, start, by, producer, now],
        )?;
        Ok(c.last_insert_rowid() as u32)
    })
}

/// Checks a goal without adding it: the proposal tool's early answer.
pub fn validate(db: &Db, flavor: &str, g: &NewGoal, now: i64) -> AppResult<()> {
    db.with_conn(|c| check(c, flavor, g, now).map(|_| ()))
}

/// Removes a goal (the app's ×): it stops showing everywhere.
pub fn delete(db: &Db, flavor: &str, id: u32, now: i64) -> AppResult<()> {
    db.with_conn(|c| {
        c.execute(
            "UPDATE goals SET archived_at = ?3 WHERE id = ?1 AND flavor = ?2 AND archived_at IS NULL",
            params![id, flavor, now],
        )?;
        Ok(())
    })
}

struct Row {
    id: i64,
    character_id: Option<i64>,
    kind: GoalKind,
    target: i64,
    label: Option<String>,
    start: f64,
    by_at: Option<i64>,
    producer: String,
    created_at: i64,
    done_at: Option<i64>,
}

fn rows(
    c: &Connection,
    flavor: &str,
    filter: &str,
    args: &[&dyn rusqlite::ToSql],
) -> AppResult<Vec<Row>> {
    let sql = format!(
        "SELECT id, character_id, kind, target, label, start, by_at, producer, created_at, done_at
         FROM goals WHERE flavor = ? AND archived_at IS NULL
           AND (character_id IS NULL OR character_id IN (SELECT id FROM visible_characters))
           AND {filter}
         ORDER BY done_at IS NOT NULL, by_at IS NULL, by_at, created_at DESC, id DESC"
    );
    let mut all: Vec<&dyn rusqlite::ToSql> = vec![&flavor];
    all.extend_from_slice(args);
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt
        .query_map(all.as_slice(), |r| {
            Ok(Row {
                id: r.get(0)?,
                character_id: r.get(1)?,
                kind: GoalKind::parse(&r.get::<_, String>(2)?),
                target: r.get(3)?,
                label: r.get(4)?,
                start: r.get(5)?,
                by_at: r.get(6)?,
                producer: r.get(7)?,
                created_at: r.get(8)?,
                done_at: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// After ingest writes a character's file: its goals, and the account's
/// gold goals, that are now reached are done.
pub fn mark_reached(tx: &rusqlite::Transaction<'_>, character_id: i64, now: i64) -> AppResult<()> {
    let flavor: String = tx.query_row(
        "SELECT flavor FROM characters WHERE id = ?1",
        [character_id],
        |r| r.get(0),
    )?;
    let open = rows(
        tx,
        &flavor,
        "done_at IS NULL AND (character_id = ? OR character_id IS NULL)",
        &[&character_id],
    )?;
    for g in open {
        if let (Some(v), _) = current(tx, &flavor, g.character_id, g.kind)? {
            if v >= g.target as f64 {
                tx.execute(
                    "UPDATE goals SET done_at = ?2 WHERE id = ?1",
                    params![g.id, now],
                )?;
            }
        }
    }
    Ok(())
}

/// `flavor`'s open goals, and the ones reached in the last 3 days; soonest
/// date first, then undated, done last.
pub fn list(db: &Db, flavor: &str, now: i64) -> AppResult<Vec<Goal>> {
    db.with_conn(|c| {
        let since = now - DONE_SHOWN;
        let mut out = Vec::new();
        for g in rows(c, flavor, "(done_at IS NULL OR done_at > ?)", &[&since])? {
            let (value, as_of) = current(c, flavor, g.character_id, g.kind)?;
            let who: Option<(String, Option<String>)> = match g.character_id {
                Some(id) => c
                    .query_row(
                        "SELECT name, class FROM characters WHERE id = ?1",
                        [id],
                        |r| {
                            Ok((
                                r.get(0)?,
                                r.get::<_, Option<String>>(1)?.map(|s| s.to_lowercase()),
                            ))
                        },
                    )
                    .optional()?,
                None => None,
            };
            let days = (now - g.created_at) as f64 / DAY;
            let per_day = match value {
                Some(v) if days >= 1.0 && v > g.start => Some((v - g.start) / days),
                _ => None,
            };
            out.push(Goal {
                id: g.id as u32,
                character_id: g.character_id.map(|v| v as u32),
                character: who.as_ref().map(|w| w.0.clone()),
                class: who.and_then(|w| w.1),
                kind: g.kind,
                target: g.target as f64,
                label: g.label,
                start: g.start,
                current: value,
                per_day,
                as_of: as_of.map(rfc3339),
                by: g.by_at.map(rfc3339),
                producer: g.producer,
                created_at: rfc3339(g.created_at),
                done_at: g.done_at.map(rfc3339),
            });
        }
        Ok(out)
    })
}

/// The Briefing slot's `goals` (INGAME §16): open goals, and reached ones
/// until the next logout after they were found reached (`done`, so the
/// briefing says "level 55 done" at the login after, with no state kept in
/// game). Kinds, numbers, names and the player's own short label; the addon
/// works out progress live. An account gold goal carries each character's
/// gold at its last logout (`alts`), so the addon can add its own live gold
/// to the rest.
pub fn slot_entries(db: &Db, flavor: &str, now: i64) -> AppResult<Vec<LuaValue>> {
    let s = LuaValue::str;
    let table = |hash| {
        LuaValue::Table(Box::new(LuaTable {
            array: Vec::new(),
            hash,
        }))
    };
    db.with_conn(|c| {
        let since = now - DONE_SHOWN;
        // The newest logout: a character's, or (account goals) anyone's.
        let last_logout = |character: Option<i64>| -> rusqlite::Result<Option<i64>> {
            match character {
                Some(id) => c.query_row(
                    "SELECT max(at) FROM char_snapshots WHERE character_id = ?1",
                    [id],
                    |r| r.get(0),
                ),
                None => c.query_row(
                    "SELECT max(s.at) FROM char_snapshots s
                     JOIN characters ch ON ch.id = s.character_id WHERE ch.flavor = ?1",
                    [flavor],
                    |r| r.get(0),
                ),
            }
        };
        let mut out = Vec::new();
        for g in rows(c, flavor, "(done_at IS NULL OR done_at > ?)", &[&since])? {
            if let Some(done) = g.done_at {
                if last_logout(g.character_id)?.is_some_and(|at| at > done) {
                    continue;
                }
            }
            let mut hash = vec![
                (LuaValue::str("id"), LuaValue::Int(g.id)),
                (LuaValue::str("kind"), LuaValue::str(g.kind.as_str())),
                (LuaValue::str("target"), LuaValue::Int(g.target)),
            ];
            if let Some(id) = g.character_id {
                let (name, surname): (String, String) = c.query_row(
                    "SELECT name, coalesce(surname, '') FROM characters WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                hash.push((LuaValue::str("name"), LuaValue::str(&name)));
                hash.push((LuaValue::str("surname"), LuaValue::str(&surname)));
            }
            if let Some(by) = g.by_at {
                hash.push((LuaValue::str("by"), LuaValue::Int(by)));
            }
            if let Some(l) = &g.label {
                hash.push((LuaValue::str("label"), LuaValue::str(l)));
            }
            if g.done_at.is_some() {
                hash.push((LuaValue::str("done"), LuaValue::Bool(true)));
            }
            if g.character_id.is_none() {
                let mut stmt = c.prepare(
                    "SELECT ch.name, coalesce(ch.surname, ''), s.money FROM char_snapshots s
                     JOIN visible_characters ch ON ch.id = s.character_id
                     WHERE ch.flavor = ?1
                       AND s.at = (SELECT max(at) FROM char_snapshots WHERE character_id = s.character_id)
                     ORDER BY ch.id",
                )?;
                let alts = stmt
                    .query_map([flavor], |r| -> rusqlite::Result<LuaValue> {
                        let name: String = r.get(0)?;
                        let surname: String = r.get(1)?;
                        Ok(table(vec![
                            (s("name"), LuaValue::str(name)),
                            (s("surname"), LuaValue::str(surname)),
                            (s("money"), LuaValue::Int(r.get(2)?)),
                        ]))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                hash.push((
                    s("alts"),
                    LuaValue::Table(Box::new(LuaTable { array: alts, hash: Vec::new() })),
                ));
            }
            out.push(table(hash));
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAVOR: &str = "_classic_beta_";
    const NOW: i64 = 1_790_000_000;

    /// Kaelor at 52 and 40% with 300 gold; Sela with 50 gold.
    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            c.execute_batch(&format!(
                "INSERT INTO characters (id, flavor, account, group_dir, char_dir, name, level, first_seen, last_seen)
                 VALUES (1, '{FLAVOR}', 'A', '70', 'Kaelor', 'Kaelor', 52, 0, 0),
                        (2, '{FLAVOR}', 'A', '70', 'Sela', 'Sela', 30, 0, 0);
                 INSERT INTO char_snapshots (character_id, at, money, level, xp, xp_max)
                 VALUES (1, 100, 3000000, 52, 40, 100), (2, 100, 500000, 30, 0, 100);"
            ))?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn level(target: f64) -> NewGoal {
        NewGoal {
            character_id: Some(1),
            kind: GoalKind::Level,
            target,
            label: None,
            by: None,
        }
    }

    fn gold(character_id: Option<u32>, gold: i64, label: Option<&str>) -> NewGoal {
        NewGoal {
            character_id,
            kind: GoalKind::Gold,
            target: (gold * COPPER_PER_GOLD) as f64,
            label: label.map(str::to_string),
            by: None,
        }
    }

    #[test]
    fn progress_comes_from_what_ingest_keeps() {
        let db = db();
        add(&db, FLAVOR, &level(55.0), "app", NOW).unwrap();
        add(
            &db,
            FLAVOR,
            &gold(Some(1), 500, Some("for the mount")),
            "app",
            NOW,
        )
        .unwrap();
        add(&db, FLAVOR, &gold(None, 1000, None), "app", NOW).unwrap();
        let goals = list(&db, FLAVOR, NOW).unwrap();
        let by_kind = |k: GoalKind, who: Option<u32>| {
            goals
                .iter()
                .find(|g| g.kind == k && g.character_id == who)
                .unwrap()
        };
        assert_eq!(
            by_kind(GoalKind::Level, Some(1)).current,
            Some(52.4),
            "level with its XP fraction"
        );
        let mount = by_kind(GoalKind::Gold, Some(1));
        assert_eq!(
            (mount.current, mount.label.as_deref()),
            (Some(3_000_000.0), Some("for the mount"))
        );
        assert_eq!(
            by_kind(GoalKind::Gold, None).current,
            Some(3_500_000.0),
            "the whole account"
        );
    }

    #[test]
    fn a_reached_goal_is_stamped_done_and_shown_for_three_days() {
        let db = db();
        let id = add(&db, FLAVOR, &level(53.0), "app", NOW).unwrap();
        // A logout at 53 and 10%: the next ingest marks it, and the pace is known.
        db.with_conn(|c| {
            c.execute("INSERT INTO char_snapshots (character_id, at, money, level, xp, xp_max) VALUES (1, 200, 3000000, 53, 10, 100)", [])?;
            let tx = c.transaction()?;
            mark_reached(&tx, 1, NOW + 2 * 86_400)?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
        let g = &list(&db, FLAVOR, NOW + 2 * 86_400).unwrap()[0];
        assert_eq!(g.id, id);
        assert!(g.done_at.is_some());
        assert!(
            (g.per_day.unwrap() - 0.35).abs() < 1e-9,
            "0.7 levels in 2 days"
        );
        let slot = slot_entries(&db, FLAVOR, NOW + 2 * 86_400).unwrap();
        assert_eq!(
            slot.len(),
            1,
            "a done goal rides the slot so the briefing says so once"
        );
        // The login after: it said so, and the next logout takes it off.
        db.with_conn(|c| {
            c.execute("INSERT INTO char_snapshots (character_id, at, money, level, xp, xp_max) VALUES (1, ?1, 3000000, 53, 20, 100)", [NOW + 3 * 86_400])?;
            Ok(())
        })
        .unwrap();
        assert!(slot_entries(&db, FLAVOR, NOW + 3 * 86_400)
            .unwrap()
            .is_empty());
        assert_eq!(
            list(&db, FLAVOR, NOW + 3 * 86_400).unwrap().len(),
            1,
            "still listed in the app"
        );
        assert!(
            list(&db, FLAVOR, NOW + 6 * 86_400).unwrap().is_empty(),
            "gone after 3 days"
        );
    }

    #[test]
    fn refused_inputs() {
        let db = db();
        let reached = add(&db, FLAVOR, &level(52.0), "app", NOW)
            .unwrap_err()
            .to_string();
        assert!(reached.contains("Kaelor is already level 52."), "{reached}");
        let rich = add(&db, FLAVOR, &gold(None, 100, None), "app", NOW)
            .unwrap_err()
            .to_string();
        assert!(
            rich.contains("The account already has 350g."),
            "what it has, not the target: {rich}"
        );
        assert_eq!(thousands(1_234_567), "1,234,567");
        let bad = [
            level(0.0),
            level(101.0),
            level(60.5),
            NewGoal {
                character_id: None,
                ..level(60.0)
            },
            NewGoal {
                label: Some("x".into()),
                ..level(60.0)
            },
            gold(Some(1), 500, Some("a label much longer than twenty-four")),
            NewGoal {
                by: Some((NOW - 1) as f64),
                ..level(60.0)
            },
            NewGoal {
                character_id: Some(9),
                ..level(60.0)
            },
        ];
        for g in bad {
            assert!(add(&db, FLAVOR, &g, "app", NOW).is_err(), "{g:?}");
        }
    }

    #[test]
    fn delete_removes_it_everywhere() {
        let db = db();
        let id = add(&db, FLAVOR, &level(60.0), "app", NOW).unwrap();
        delete(&db, FLAVOR, id, NOW).unwrap();
        assert!(list(&db, FLAVOR, NOW).unwrap().is_empty());
        assert!(slot_entries(&db, FLAVOR, NOW).unwrap().is_empty());
    }

    /// O2: a hidden character's goals leave the list and the slot, and its
    /// gold leaves the account's; Unhide brings them back.
    #[test]
    fn a_hidden_characters_goals_are_hidden() {
        let db = db();
        add(&db, FLAVOR, &level(60.0), "app", NOW).unwrap();
        add(&db, FLAVOR, &gold(None, 1000, None), "app", NOW).unwrap();
        db.with_conn(|c| {
            c.execute("UPDATE characters SET hidden_at = 1 WHERE id = 1", [])?;
            Ok(())
        })
        .unwrap();
        let goals = list(&db, FLAVOR, NOW).unwrap();
        assert_eq!(goals.len(), 1, "only the account's");
        assert_eq!(
            goals[0].current,
            Some(500_000.0),
            "Sela's 50g, not Kaelor's"
        );
        assert_eq!(slot_entries(&db, FLAVOR, NOW).unwrap().len(), 1);
        let hidden = NewGoal {
            character_id: Some(1),
            ..level(61.0)
        };
        assert!(
            add(&db, FLAVOR, &hidden, "app", NOW).is_err(),
            "not for a hidden character"
        );
    }
}
