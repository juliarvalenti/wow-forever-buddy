//! Login notes (B1, INGAME §9, IMPLEMENTING §15): a line written in the app
//! for one character, shown in the game's chat when it logs in, through the
//! Briefing slot (`bridge::briefing`).
//!
//! A note is "once" (the next login shows it, then it's archived when the
//! addon's `briefed` receipt comes back) or "until" a date (every login
//! until then). Deleting a note archives it. The text is plain: the addon
//! escapes `|` before it reaches the chat frame.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{AppError, AppResult};

/// The longest note: one chat line, comfortably.
pub const MAX_TEXT: usize = 300;
/// Active notes per character, so the slot stays small.
const MAX_ACTIVE: i64 = 20;

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct LoginNote {
    pub id: u32,
    pub character_id: u32,
    pub text: String,
    /// Shown at the next login only.
    pub once: bool,
    /// For a non-once note: shown at each login until this (RFC 3339).
    pub until: Option<String>,
    /// "you", or "claude" for an approved agent proposal (P2).
    pub author: String,
    pub created_at: String,
    /// The first login that showed it (RFC 3339).
    pub shown_at: Option<String>,
}

/// What the app's form sends.
#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct NewNote {
    pub character_id: u32,
    pub text: String,
    pub once: bool,
    /// Required when `once` is false: unix seconds, in the future.
    pub until: Option<f64>,
}

fn rfc3339(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0)
        .unwrap_or_default()
        .to_rfc3339()
}

/// The note text as it will be stored: trimmed, one line, within the cap.
pub fn clean(text: &str) -> AppResult<String> {
    let one_line: String = text
        .split(|c: char| c.is_control())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let t = one_line.trim();
    if t.is_empty() {
        return Err(AppError::InvalidSettings("a note needs some text".into()));
    }
    if t.chars().count() > MAX_TEXT {
        return Err(AppError::InvalidSettings(format!(
            "a note is at most {MAX_TEXT} characters"
        )));
    }
    Ok(t.to_string())
}

/// Active notes (not archived, not expired) for `flavor`'s characters,
/// newest first, plus recently archived ones (shown, last 7 days) so the
/// app can say "shown at Thursday's login".
pub fn list(db: &Db, flavor: &str, now: i64) -> AppResult<Vec<LoginNote>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT n.id, n.character_id, n.text, n.once, n.until_at, n.author,
                    n.created_at, n.shown_at
             FROM login_notes n JOIN characters ch ON ch.id = n.character_id
             WHERE ch.flavor = ?1
               AND ((n.archived_at IS NULL AND (n.once = 1 OR n.until_at > ?2))
                    OR (n.shown_at IS NOT NULL AND n.shown_at > ?2 - 7 * 86400))
             ORDER BY n.created_at DESC, n.id DESC",
        )?;
        let notes = stmt
            .query_map(params![flavor, now], |r| {
                Ok(LoginNote {
                    id: r.get::<_, i64>(0)? as u32,
                    character_id: r.get::<_, i64>(1)? as u32,
                    text: r.get(2)?,
                    once: r.get::<_, i64>(3)? != 0,
                    until: r.get::<_, Option<i64>>(4)?.map(rfc3339),
                    author: r.get(5)?,
                    created_at: rfc3339(r.get(6)?),
                    shown_at: r.get::<_, Option<i64>>(7)?.map(rfc3339),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(notes)
    })
}

pub fn add(db: &Db, flavor: &str, note: &NewNote, author: &str, now: i64) -> AppResult<u32> {
    let text = clean(&note.text)?;
    let until = if note.once {
        None
    } else {
        let until = note
            .until
            .filter(|t| t.is_finite())
            .map(|t| t as i64)
            .ok_or_else(|| {
                AppError::InvalidSettings("pick the date the note shows until".into())
            })?;
        if until <= now {
            return Err(AppError::InvalidSettings(
                "the date the note shows until has passed".into(),
            ));
        }
        Some(until)
    };
    db.with_conn(|c| {
        let owned: Option<i64> = c
            .query_row(
                "SELECT id FROM characters WHERE id = ?1 AND flavor = ?2",
                params![note.character_id, flavor],
                |r| r.get(0),
            )
            .ok();
        if owned.is_none() {
            return Err(AppError::NotFound("that character".into()));
        }
        let active: i64 = c.query_row(
            "SELECT count(*) FROM login_notes
             WHERE character_id = ?1 AND archived_at IS NULL",
            [note.character_id],
            |r| r.get(0),
        )?;
        if active >= MAX_ACTIVE {
            return Err(AppError::InvalidSettings(format!(
                "a character can have at most {MAX_ACTIVE} notes waiting"
            )));
        }
        c.execute(
            "INSERT INTO login_notes (character_id, text, once, until_at, author, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![note.character_id, text, note.once, until, author, now],
        )?;
        Ok(c.last_insert_rowid() as u32)
    })
}

/// Archives a note (the app's Delete): it stops showing at once.
pub fn delete(db: &Db, flavor: &str, id: u32, now: i64) -> AppResult<()> {
    db.with_conn(|c| {
        c.execute(
            "UPDATE login_notes SET archived_at = ?3
             WHERE id = ?1 AND archived_at IS NULL
               AND character_id IN (SELECT id FROM characters WHERE flavor = ?2)",
            params![id, flavor, now],
        )?;
        Ok(())
    })
}

/// The addon's receipt: these notes were shown to `character_id` at
/// `at`. A once note is archived; an until note keeps showing.
pub fn mark_shown(
    tx: &rusqlite::Transaction<'_>,
    character_id: i64,
    shown: &[(i64, i64)],
) -> rusqlite::Result<()> {
    for &(id, at) in shown {
        tx.execute(
            "UPDATE login_notes
             SET shown_at = coalesce(shown_at, ?3),
                 archived_at = CASE WHEN once = 1 THEN coalesce(archived_at, ?3) ELSE archived_at END
             WHERE id = ?1 AND character_id = ?2",
            params![id, character_id, at],
        )?;
    }
    Ok(())
}

/// The note each character would see at its next login: its newest active
/// one. `(character_id, id, text)`.
pub fn due(db: &Db, flavor: &str, now: i64) -> AppResult<Vec<(i64, i64, String)>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT n.character_id, n.id, n.text
             FROM login_notes n JOIN characters ch ON ch.id = n.character_id
             WHERE ch.flavor = ?1 AND n.archived_at IS NULL
               AND (n.once = 1 OR n.until_at > ?2)
               AND n.id = (SELECT m.id FROM login_notes m
                           WHERE m.character_id = n.character_id AND m.archived_at IS NULL
                             AND (m.once = 1 OR m.until_at > ?2)
                           ORDER BY m.created_at DESC, m.id DESC LIMIT 1)
             ORDER BY n.character_id",
        )?;
        let rows = stmt
            .query_map(params![flavor, now], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAVOR: &str = "_classic_beta_";

    fn db_with(names: &[&str]) -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            for name in names {
                c.execute(
                    "INSERT INTO characters (flavor, account, group_dir, char_dir, name,
                                             first_seen, last_seen)
                     VALUES (?1, 'A', '70', ?2, ?2, 0, 0)",
                    params![FLAVOR, name],
                )?;
            }
            Ok(())
        })
        .unwrap();
        db
    }

    fn new(character_id: u32, text: &str, once: bool, until: Option<f64>) -> NewNote {
        NewNote {
            character_id,
            text: text.into(),
            once,
            until,
        }
    }

    #[test]
    fn text_is_one_trimmed_line_within_the_cap() {
        assert_eq!(clean("  Hand in\nOnyxia\r\n  ").unwrap(), "Hand in Onyxia");
        assert!(clean("   ").is_err());
        assert!(clean(&"x".repeat(MAX_TEXT)).is_ok());
        assert!(clean(&"x".repeat(MAX_TEXT + 1)).is_err());
    }

    #[test]
    fn a_once_note_is_due_until_shown_then_archived() {
        let db = db_with(&["Kaelor"]);
        let id = add(&db, FLAVOR, &new(1, "Train skills", true, None), "you", 100).unwrap();
        assert_eq!(
            due(&db, FLAVOR, 200).unwrap(),
            [(1, id as i64, "Train skills".into())]
        );

        db.with_conn(|c| {
            let tx = c.transaction()?;
            mark_shown(&tx, 1, &[(id as i64, 300)])?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
        assert!(due(&db, FLAVOR, 400).unwrap().is_empty());
        // Still listed for a week as shown, then gone.
        let listed = list(&db, FLAVOR, 400).unwrap();
        assert_eq!(listed[0].shown_at.as_deref(), Some(rfc3339(300).as_str()));
        assert!(list(&db, FLAVOR, 300 + 8 * 86400).unwrap().is_empty());
    }

    #[test]
    fn an_until_note_shows_each_login_until_its_date_and_the_newest_wins() {
        let db = db_with(&["Kaelor"]);
        let old = add(
            &db,
            FLAVOR,
            &new(1, "Old", false, Some(1_000.0)),
            "you",
            100,
        )
        .unwrap();
        db.with_conn(|c| {
            let tx = c.transaction()?;
            mark_shown(&tx, 1, &[(old as i64, 150)])?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
        // Shown once, still due: an until note isn't archived by showing.
        assert_eq!(due(&db, FLAVOR, 200).unwrap()[0].1, old as i64);
        let newer = add(&db, FLAVOR, &new(1, "Newer", true, None), "claude", 300).unwrap();
        assert_eq!(due(&db, FLAVOR, 400).unwrap()[0].1, newer as i64);
        delete(&db, FLAVOR, newer, 500).unwrap();
        assert_eq!(due(&db, FLAVOR, 600).unwrap()[0].1, old as i64);
        // Past its date: not due, not listed.
        assert!(due(&db, FLAVOR, 1_000).unwrap().is_empty());
    }

    #[test]
    fn refused_inputs() {
        let db = db_with(&["Kaelor"]);
        assert!(
            add(&db, FLAVOR, &new(1, "x", false, None), "you", 100).is_err(),
            "until needs a date"
        );
        assert!(
            add(&db, FLAVOR, &new(1, "x", false, Some(50.0)), "you", 100).is_err(),
            "in the past"
        );
        assert!(matches!(
            add(&db, FLAVOR, &new(9, "x", true, None), "you", 100),
            Err(AppError::NotFound(_))
        ));
        for i in 0..MAX_ACTIVE {
            add(
                &db,
                FLAVOR,
                &new(1, &format!("n{i}"), true, None),
                "you",
                100,
            )
            .unwrap();
        }
        assert!(add(&db, FLAVOR, &new(1, "one too many", true, None), "you", 100).is_err());
    }
}
