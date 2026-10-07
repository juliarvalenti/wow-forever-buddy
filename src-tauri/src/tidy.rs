//! O2 (IMPLEMENTING §22): tidying characters that are gone from WTF, and
//! the app's own database size. Only Forever Buddy's data changes here:
//! WTF, SavedVariables and backups are never touched.
//!
//! A character is **gone** when its folder (`WTF/Account/<account>/<group>/
//! <character>`) isn't on disk any more. **Hidden** is a flag: the
//! character leaves every list, total and agent tool (they read
//! `visible_characters`), and nothing is deleted. **Forget** deletes its
//! history (the tables cascade from `characters`) and keeps its folder in
//! `forgotten`, which ingest skips in WTF and in backups until Remember
//! again; then it's read as a new character.

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::ingest::apply::Target;

/// A character in Settings › Data: a gone one, or a hidden one.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct TidyCharacter {
    pub id: u32,
    pub name: String,
    /// File token, lowercase, for the class colour.
    pub class: Option<String>,
    /// RFC 3339.
    pub last_seen: String,
    /// RFC 3339, when it was hidden.
    pub hidden_at: Option<String>,
    /// Its WTF folder isn't on disk.
    pub gone: bool,
    /// The folder, so Characters can leave a hidden one out of the WTF list
    /// too: `WTF/Account/<account>/<group_dir>/<char_dir>`.
    pub account: String,
    pub group_dir: String,
    pub char_dir: String,
}

/// "Forever Buddy's own data" in Settings › Data.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct DataSize {
    /// The database file and its journal (write-ahead log).
    pub bytes: f64,
    /// The daily copies (up to 7), which hold the same data.
    pub copies: u32,
    pub copies_bytes: f64,
    pub characters: u32,
    pub gold_days: u32,
    pub adventures: u32,
    pub items_seen: u32,
    pub price_days: u32,
}

/// A forgotten character, for "Forgotten (1)".
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Forgotten {
    /// For Remember again.
    pub id: u32,
    pub name: String,
    pub class: Option<String>,
    /// RFC 3339.
    pub forgotten_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Tidy {
    /// Gone from WTF, hidden or not. Empty when the game folder can't be read.
    pub gone: Vec<TidyCharacter>,
    pub hidden: Vec<TidyCharacter>,
    pub forgotten: Vec<Forgotten>,
}

/// What Forget removes, for the dialog: "212 adventures, gold since March
/// (…), bags, bank, quests, recipes, lockouts, 3 notes and a plan".
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct ForgetPreview {
    pub id: u32,
    pub name: String,
    pub adventures: u32,
    /// RFC 3339, its first gold point.
    pub gold_since: Option<String>,
    pub notes: u32,
    pub plans: u32,
}

fn rfc3339(t: i64) -> String {
    DateTime::<Utc>::from_timestamp(t, 0)
        .unwrap_or_default()
        .to_rfc3339()
}

/// Characters of `flavor` whose folder isn't under `flavor_dir`. None at all
/// when `WTF/Account` itself is missing: an unplugged drive or a moved game
/// isn't every character gone.
fn gone_ids(c: &Connection, flavor: &str, flavor_dir: &Path) -> AppResult<Vec<u32>> {
    let accounts = flavor_dir.join("WTF").join("Account");
    if !accounts.is_dir() {
        return Ok(Vec::new());
    }
    let mut stmt =
        c.prepare("SELECT id, account, group_dir, char_dir FROM characters WHERE flavor = ?1")?;
    let rows = stmt
        .query_map([flavor], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|(_, account, group, char_dir)| {
            !accounts.join(account).join(group).join(char_dir).is_dir()
        })
        .map(|(id, ..)| id)
        .collect())
}

fn characters(c: &Connection, flavor: &str, gone: &[u32]) -> AppResult<Vec<TidyCharacter>> {
    let mut stmt = c.prepare(
        "SELECT id, name, surname, lower(class), last_seen, hidden_at, account, group_dir, char_dir
         FROM characters WHERE flavor = ?1 ORDER BY name, id",
    )?;
    let rows = stmt
        .query_map([flavor], |r| {
            let id: u32 = r.get(0)?;
            let name: String = r.get(1)?;
            let surname: Option<String> = r.get(2)?;
            Ok(TidyCharacter {
                id,
                name: match surname {
                    Some(s) if !s.is_empty() => format!("{name} {s}"),
                    _ => name,
                },
                class: r.get(3)?,
                last_seen: rfc3339(r.get(4)?),
                hidden_at: r.get::<_, Option<i64>>(5)?.map(rfc3339),
                gone: gone.contains(&id),
                account: r.get(6)?,
                group_dir: r.get(7)?,
                char_dir: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn data_size(c: &Connection, db_path: &Path) -> AppResult<DataSize> {
    let wal = db_path.with_file_name(format!(
        "{}-wal",
        db_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("buddy.db")
    ));
    // The daily copies (`buddy-<date>.db`), not the pre-migration ones.
    let copies: Vec<u64> = std::fs::read_dir(crate::db::copies::dir_for(db_path))
        .map(|dir| {
            dir.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("buddy-"))
                .map(|e| file_len(&e.path()))
                .collect()
        })
        .unwrap_or_default();
    let count = |sql: &str| -> AppResult<u32> { Ok(c.query_row(sql, [], |r| r.get(0))?) };
    Ok(DataSize {
        bytes: (file_len(db_path) + file_len(&wal)) as f64,
        copies: copies.len() as u32,
        copies_bytes: copies.iter().sum::<u64>() as f64,
        characters: count("SELECT count(*) FROM characters")?,
        gold_days: count("SELECT count(DISTINCT at / 86400) FROM gold_points")?,
        adventures: count("SELECT count(*) FROM adventures")?,
        items_seen: count("SELECT count(*) FROM items")?,
        price_days: count("SELECT count(DISTINCT day) FROM ah_prices")?,
    })
}

/// Settings › Data's numbers.
pub fn data(db: &Db, db_path: &Path) -> AppResult<DataSize> {
    db.with_conn(|c| data_size(c, db_path))
}

/// Gone, hidden and forgotten characters, for Characters and Settings ›
/// Data. `flavor_dir` is `None` without a game folder: nothing is gone then.
pub fn status(db: &Db, flavor: &str, flavor_dir: Option<&Path>) -> AppResult<Tidy> {
    db.with_conn(|c| {
        let gone = match flavor_dir {
            Some(dir) => gone_ids(c, flavor, dir)?,
            None => Vec::new(),
        };
        let all = characters(c, flavor, &gone)?;
        let mut stmt = c.prepare(
            "SELECT rowid, name, lower(class), forgotten_at FROM forgotten
             WHERE flavor = ?1 ORDER BY forgotten_at DESC, rowid",
        )?;
        let forgotten = stmt
            .query_map([flavor], |r| {
                Ok(Forgotten {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    class: r.get(2)?,
                    forgotten_at: rfc3339(r.get(3)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Tidy {
            gone: all.iter().filter(|t| t.gone).cloned().collect(),
            hidden: all.into_iter().filter(|t| t.hidden_at.is_some()).collect(),
            forgotten,
        })
    })
}

/// Hide or unhide. Either way nothing is deleted.
pub fn set_hidden(db: &Db, id: u32, hidden: bool, now: i64) -> AppResult<()> {
    db.with_conn(|c| {
        let n = c.execute(
            "UPDATE characters SET hidden_at = CASE WHEN ?2 THEN coalesce(hidden_at, ?3) END
             WHERE id = ?1",
            params![id, hidden, now],
        )?;
        if n == 0 {
            return Err(AppError::NotFound(format!("no character {id}")));
        }
        Ok(())
    })
}

/// Forget is only for a character whose folder is gone (§22, "Never").
fn check_gone(c: &Connection, flavor: &str, flavor_dir: &Path, id: u32) -> AppResult<String> {
    let name: Option<String> = c
        .query_row(
            "SELECT name FROM characters WHERE id = ?1 AND flavor = ?2",
            params![id, flavor],
            |r| r.get(0),
        )
        .optional()?;
    let name = name.ok_or_else(|| AppError::NotFound(format!("no character {id}")))?;
    if !gone_ids(c, flavor, flavor_dir)?.contains(&id) {
        return Err(AppError::InvalidSettings(format!(
            "{name} is still in your WTF folder, so it can be hidden but not forgotten"
        )));
    }
    Ok(name)
}

pub fn preview(db: &Db, flavor: &str, flavor_dir: &Path, id: u32) -> AppResult<ForgetPreview> {
    db.with_conn(|c| {
        let name = check_gone(c, flavor, flavor_dir, id)?;
        let count = |sql: &str| -> AppResult<u32> { Ok(c.query_row(sql, [id], |r| r.get(0))?) };
        Ok(ForgetPreview {
            id,
            name,
            adventures: count("SELECT count(*) FROM adventures WHERE character_id = ?1")?,
            gold_since: c
                .query_row(
                    "SELECT min(at) FROM gold_points WHERE character_id = ?1",
                    [id],
                    |r| r.get::<_, Option<i64>>(0),
                )?
                .map(rfc3339),
            notes: count(
                "SELECT count(*) FROM login_notes WHERE character_id = ?1 AND archived_at IS NULL",
            )?,
            plans: count(
                "SELECT count(*) FROM quest_plans WHERE character_id = ?1 AND status = 'active'",
            )?,
        })
    })
}

/// Deletes the character's app history and remembers its folder. Its
/// waiting agent suggestions are declined first, since approving one would
/// point at a character that no longer exists.
pub fn forget(db: &Db, flavor: &str, flavor_dir: &Path, id: u32, now: i64) -> AppResult<String> {
    db.with_conn(|c| {
        let name = check_gone(c, flavor, flavor_dir, id)?;
        let tx = c.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO forgotten (flavor, account, group_dir, char_dir, name, class,
                                               forgotten_at)
             SELECT flavor, account, group_dir, char_dir,
                    name || CASE WHEN coalesce(surname, '') = '' THEN '' ELSE ' ' || surname END,
                    class, ?2
             FROM characters WHERE id = ?1",
            params![id, now],
        )?;
        tx.execute(
            "UPDATE proposals SET status = 'discarded', status_reason = 'the character was forgotten',
                                  decided_at = ?2
             WHERE status = 'staged' AND body IS NOT NULL AND json_valid(body)
               AND (json_extract(body, '$.character_id') = ?1
                    OR json_extract(body, '$.for_character_id') = ?1)",
            params![id, now],
        )?;
        // So a folder that comes back with the same file is read again.
        tx.execute(
            "DELETE FROM ingest_state WHERE lower(path) = (
                 SELECT lower(flavor || '/WTF/Account/' || account || '/' || group_dir || '/'
                              || char_dir || '/SavedVariables/ForeverBuddy.lua')
                 FROM characters WHERE id = ?1)",
            [id],
        )?;
        tx.execute("DELETE FROM characters WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(name)
    })
}

/// Ingest skips a forgotten folder, in WTF and in backups.
pub fn is_forgotten(db: &Db, t: &Target) -> AppResult<bool> {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT EXISTS (SELECT 1 FROM forgotten
                            WHERE flavor = ?1 AND account = ?2 AND group_dir = ?3 AND char_dir = ?4)",
            params![t.flavor, t.account, t.group_dir, t.char_dir],
            |r| r.get(0),
        )?)
    })
}

/// Remember again: the folder can be read again, from WTF or a newer
/// backup, as a new character. Old history doesn't come back.
pub fn remember(db: &Db, id: u32) -> AppResult<()> {
    db.with_conn(|c| {
        if c.execute("DELETE FROM forgotten WHERE rowid = ?1", [id])? == 0 {
            return Err(AppError::NotFound(format!("no forgotten character {id}")));
        }
        Ok(())
    })
}

/// Settings › Data's Compact: gives freed pages back to the disk.
pub fn compact(db: &Db) -> AppResult<()> {
    db.with_conn(|c| {
        c.execute_batch("VACUUM")?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const FLAVOR: &str = "_classic_beta_";

    /// Here (1) is in WTF; Oldmain (2) isn't. Oldmain has gold, an
    /// adventure, a bag item, a note and a waiting agent suggestion, and
    /// Here has a bag mark sending something to Oldmain.
    fn fixture() -> (tempfile::TempDir, Db, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("buddy.db");
        let db = Db::open(&db_path).unwrap();
        let flavor_dir = dir.path().join(FLAVOR);
        std::fs::create_dir_all(flavor_dir.join("WTF/Account/A/70/Here")).unwrap();
        db.with_conn(|c| {
            c.execute_batch(&format!(
                "INSERT INTO characters (id, flavor, account, group_dir, char_dir, name, class, first_seen, last_seen)
                 VALUES (1, '{FLAVOR}', 'A', '70', 'Here', 'Here', 'MAGE', 1, 100),
                        (2, '{FLAVOR}', 'A', '70', 'Oldmain', 'Oldmain', 'WARRIOR', 1, 50);
                 INSERT INTO gold_points (character_id, at, money) VALUES (1, 10, 500), (2, 10, 900);
                 INSERT INTO adventures (character_id, login, logout) VALUES (2, 10, 20);
                 INSERT INTO char_items (character_id, location, container, slot, item_id, link, count, as_of)
                 VALUES (2, 'bag', 0, 1, 2589, '', 20, 10);
                 INSERT INTO login_notes (character_id, text, once, author, created_at) VALUES (2, 'hi', 1, 'you', 10);
                 INSERT INTO cleanup_marks (character_id, item_id, action, to_character_id, created_at)
                 VALUES (1, 2589, 'send', 2, 'x');
                 INSERT INTO proposals (flavor, file, kind, body, producer, created_at, received_at, status)
                 VALUES ('{FLAVOR}', 'a.json', 'login_note', '{{\"character_id\":2,\"text\":\"x\"}}', 'Claude', 1, 1, 'staged'),
                        ('{FLAVOR}', 'b.json', 'login_note', '{{\"character_id\":1,\"text\":\"x\"}}', 'Claude', 1, 1, 'staged');"
            ))?;
            Ok(())
        })
        .unwrap();
        (dir, db, db_path, flavor_dir)
    }

    fn count(db: &Db, sql: &str) -> i64 {
        db.with_conn(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    }

    #[test]
    fn gone_is_a_missing_folder_and_a_missing_wtf_is_nothing_gone() {
        let (dir, db, db_path, flavor_dir) = fixture();
        let t = status(&db, FLAVOR, Some(&flavor_dir)).unwrap();
        assert_eq!(t.gone.iter().map(|g| g.id).collect::<Vec<_>>(), vec![2]);
        assert!(t.hidden.is_empty());
        let d = data(&db, &db_path).unwrap();
        assert!(d.bytes > 0.0);
        assert_eq!((d.characters, d.gold_days, d.adventures), (2, 1, 1));

        let elsewhere = dir.path().join("moved");
        let t = status(&db, FLAVOR, Some(&elsewhere)).unwrap();
        assert!(t.gone.is_empty(), "no WTF/Account: nothing is gone");
    }

    #[test]
    fn hidden_leaves_the_lists_and_comes_back() {
        let (_dir, db, _db_path, flavor_dir) = fixture();
        set_hidden(&db, 2, true, 5).unwrap();
        let cards = crate::characters::overview(&db, FLAVOR).unwrap().characters;
        assert_eq!(cards.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1]);
        assert!(
            crate::characters::by_name(&db, FLAVOR, "Oldmain").is_err(),
            "nor the agent"
        );
        let t = status(&db, FLAVOR, Some(&flavor_dir)).unwrap();
        assert_eq!(t.hidden[0].id, 2);
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM gold_points WHERE character_id = 2"
            ),
            1,
            "nothing deleted"
        );

        set_hidden(&db, 2, false, 6).unwrap();
        assert_eq!(
            crate::characters::overview(&db, FLAVOR)
                .unwrap()
                .characters
                .len(),
            2
        );
        assert!(set_hidden(&db, 99, true, 5).is_err());
    }

    #[test]
    fn forget_is_only_for_gone_characters_and_removes_only_app_history() {
        let (_dir, db, _db_path, flavor_dir) = fixture();
        let e = forget(&db, FLAVOR, &flavor_dir, 1, 7).unwrap_err();
        assert!(e.to_string().contains("still in your WTF folder"), "{e}");
        assert!(preview(&db, FLAVOR, &flavor_dir, 1).is_err());

        let p = preview(&db, FLAVOR, &flavor_dir, 2).unwrap();
        assert_eq!((p.adventures, p.notes, p.plans), (1, 1, 0));
        assert!(p.gold_since.is_some());

        assert_eq!(forget(&db, FLAVOR, &flavor_dir, 2, 7).unwrap(), "Oldmain");
        for sql in [
            "SELECT count(*) FROM characters WHERE id = 2",
            "SELECT count(*) FROM gold_points WHERE character_id = 2",
            "SELECT count(*) FROM adventures",
            "SELECT count(*) FROM login_notes",
            "SELECT count(*) FROM cleanup_marks",
        ] {
            assert_eq!(count(&db, sql), 0, "{sql}");
        }
        assert_eq!(
            count(&db, "SELECT count(*) FROM gold_points"),
            1,
            "Here's stays"
        );
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM proposals WHERE status = 'discarded'"
            ),
            1,
            "only Oldmain's suggestion is declined"
        );
        assert!(
            flavor_dir.join("WTF/Account/A/70/Here").is_dir(),
            "WTF untouched"
        );

        let t = Target {
            flavor: FLAVOR.into(),
            account: "a".into(),
            group_dir: "70".into(),
            char_dir: "OLDMAIN".into(),
        };
        assert!(
            is_forgotten(&db, &t).unwrap(),
            "folder names compare like Windows"
        );
        let f = status(&db, FLAVOR, Some(&flavor_dir)).unwrap().forgotten;
        assert_eq!(f[0].name, "Oldmain");
        assert_eq!(f[0].class.as_deref(), Some("warrior"));
        remember(&db, f[0].id).unwrap();
        assert!(!is_forgotten(&db, &t).unwrap());
        assert!(remember(&db, f[0].id).is_err());
    }

    #[test]
    fn compact_keeps_the_data() {
        let (_dir, db, db_path, flavor_dir) = fixture();
        compact(&db).unwrap();
        assert_eq!(
            status(&db, FLAVOR, Some(&flavor_dir)).unwrap().gone.len(),
            1
        );
        assert_eq!(data(&db, &db_path).unwrap().characters, 2);
    }
}
