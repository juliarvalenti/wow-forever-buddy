//! The Briefing slot (B1, INGAME §9): what the login line needs from the
//! app, since the game can't see other characters. Which alts have letters
//! waiting (how many, the soonest expiry), and each character's login note.
//! Also, for the entrance line (L2, INGAME §13), each character's saved
//! instances that haven't reset yet: `lockouts = { { name, surname, class,
//! saves = { "Molten Core", "40 Player", resetAt, … } } }`.
//!
//! The only strings are the account's own character names and the notes
//! Julia wrote (or approved); the addon escapes `|` at display. Mail is
//! counted, never quoted: no sender or subject leaves the app (spec §4).

use std::collections::HashMap;

use crate::db::Db;
use crate::error::AppResult;
use crate::notes;
use crate::sv::{LuaTable, LuaValue};

use super::{header, render, Slot};

fn key(k: &str) -> LuaValue {
    LuaValue::str(k)
}

fn table(array: Vec<LuaValue>, hash: Vec<(LuaValue, LuaValue)>) -> LuaValue {
    LuaValue::Table(Box::new(LuaTable { array, hash }))
}

struct Who {
    name: String,
    surname: String,
    class: String,
}

pub fn build(db: &Db, flavor: &str, stamp: i64) -> AppResult<Vec<u8>> {
    let (who, mail, saves) = db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT id, name, coalesce(surname, ''), upper(coalesce(class, ''))
             FROM characters WHERE flavor = ?1",
        )?;
        let who: HashMap<i64, Who> = stmt
            .query_map([flavor], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    Who {
                        name: r.get(1)?,
                        surname: r.get(2)?,
                        class: r.get(3)?,
                    },
                ))
            })?
            .collect::<Result<_, _>>()?;
        // Letters per character, and when the first of them expires.
        let mut stmt = c.prepare(
            "SELECT m.character_id, count(*),
                    min(CASE WHEN m.days_left IS NULL THEN NULL
                             ELSE m.as_of + CAST(m.days_left * 86400 AS INTEGER) END)
             FROM char_mail m JOIN characters ch ON ch.id = m.character_id
             WHERE ch.flavor = ?1
             GROUP BY m.character_id",
        )?;
        let mut mail: Vec<(i64, i64, Option<i64>)> = stmt
            .query_map([flavor], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        // Soonest expiry first (none last), then the most letters.
        mail.sort_by_key(|&(id, n, exp)| (exp.is_none(), exp, -n, id));
        // Saves that haven't reset by now (an expired one is already gone).
        let mut stmt = c.prepare(
            "SELECT l.character_id, l.name, l.difficulty, l.reset_at
             FROM lockouts l JOIN characters ch ON ch.id = l.character_id
             WHERE ch.flavor = ?1 AND l.reset_at > ?2
             ORDER BY l.character_id, l.name, l.difficulty",
        )?;
        let mut saves: Vec<(i64, Vec<LuaValue>)> = Vec::new();
        for row in stmt.query_map(rusqlite::params![flavor, stamp], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })? {
            let (id, name, difficulty, reset) = row?;
            let entry = [
                LuaValue::str(name),
                LuaValue::str(difficulty),
                LuaValue::Int(reset),
            ];
            match saves.last_mut() {
                Some((last, flat)) if *last == id => flat.extend(entry),
                _ => saves.push((id, entry.into())),
            }
        }
        Ok((who, mail, saves))
    })?;

    let person = |id: i64, extra: Vec<(LuaValue, LuaValue)>| -> Option<LuaValue> {
        let w = who.get(&id)?;
        let mut hash = vec![
            (key("name"), LuaValue::str(&w.name)),
            (key("surname"), LuaValue::str(&w.surname)),
            (key("class"), LuaValue::str(&w.class)),
        ];
        hash.extend(extra);
        Some(table(Vec::new(), hash))
    };

    let mail_rows: Vec<LuaValue> = mail
        .iter()
        .filter_map(|&(id, letters, expires)| {
            let mut extra = vec![(key("letters"), LuaValue::Int(letters))];
            if let Some(t) = expires {
                extra.push((key("expires"), LuaValue::Int(t)));
            }
            person(id, extra)
        })
        .collect();
    let note_rows: Vec<LuaValue> = notes::due(db, flavor, stamp)?
        .into_iter()
        .filter_map(|n| {
            person(
                n.character_id,
                vec![
                    (key("id"), LuaValue::Int(n.id)),
                    (key("text"), LuaValue::str(&n.text)),
                    (key("once"), LuaValue::Bool(n.once)),
                ],
            )
        })
        .collect();

    let lockout_rows: Vec<LuaValue> = saves
        .into_iter()
        .filter_map(|(id, flat)| person(id, vec![(key("saves"), table(flat, Vec::new()))]))
        .collect();

    let mut body = header(stamp);
    body.hash.push((key("mail"), table(mail_rows, Vec::new())));
    body.hash.push((key("notes"), table(note_rows, Vec::new())));
    body.hash
        .push((key("lockouts"), table(lockout_rows, Vec::new())));
    render(Slot::Briefing, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::check;
    use crate::notes::NewNote;
    use rusqlite::params;

    const FLAVOR: &str = "_classic_beta_";

    #[test]
    fn mail_counts_soonest_first_and_each_characters_newest_note() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            for (name, class) in [
                ("Sela", "PRIEST"),
                ("Kaelor", "ROGUE"),
                ("Brannic", "HUNTER"),
            ] {
                c.execute(
                    "INSERT INTO characters (flavor, account, group_dir, char_dir, name, class,
                                             first_seen, last_seen)
                     VALUES (?1, 'A', '70', ?2, ?2, ?3, 0, 0)",
                    params![FLAVOR, name, class],
                )?;
            }
            // Sela: 2 letters, one expiring in 2 days. Kaelor: 1, in 20 days.
            // Brannic: none.
            for (character, idx, days, sender) in [
                (1, 0, 2.0, "Secret Sender"),
                (1, 1, 29.0, "x"),
                (2, 0, 20.0, "y"),
            ] {
                c.execute(
                    "INSERT INTO char_mail (character_id, idx, sender, subject, days_left, as_of)
                     VALUES (?1, ?2, ?3, 'Private subject', ?4, 1000)",
                    params![character, idx, sender, days],
                )?;
            }
            Ok(())
        })
        .unwrap();
        let note = |character_id, text: &str| NewNote {
            character_id,
            text: text.into(),
            once: true,
            until: None,
        };
        notes::add(&db, FLAVOR, &note(2, "Older"), notes::Author::You, 10).unwrap();
        notes::add(
            &db,
            FLAVOR,
            &note(2, "Train poisons"),
            notes::Author::You,
            20,
        )
        .unwrap();

        let bytes = build(&db, FLAVOR, 5_000).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("Secret Sender") && !text.contains("Private subject"));
        let LuaValue::Table(t) = check(Slot::Briefing, &bytes).unwrap() else {
            unreachable!()
        };

        let mail = t.get("mail").unwrap().as_table().unwrap();
        assert_eq!(mail.array.len(), 2, "Brannic has none");
        let first = mail.array[0].as_table().unwrap();
        assert_eq!(
            first.get("name").and_then(|v| v.as_bytes()),
            Some(&b"Sela"[..])
        );
        assert_eq!(first.get("letters"), Some(&LuaValue::Int(2)));
        assert_eq!(first.get("expires"), Some(&LuaValue::Int(1000 + 2 * 86400)));

        let notes = t.get("notes").unwrap().as_table().unwrap();
        assert_eq!(notes.array.len(), 1);
        let n = notes.array[0].as_table().unwrap();
        assert_eq!(
            n.get("name").and_then(|v| v.as_bytes()),
            Some(&b"Kaelor"[..])
        );
        assert_eq!(
            n.get("text").and_then(|v| v.as_bytes()),
            Some(&b"Train poisons"[..])
        );
    }

    /// L2: each character's saves that haven't reset, grouped; an expired
    /// one is left out, and so is a character with none.
    #[test]
    fn lockouts_not_yet_reset() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            for name in ["Velyra", "Kaelor"] {
                c.execute(
                    "INSERT INTO characters (flavor, account, group_dir, char_dir, name, class,
                                             first_seen, last_seen)
                     VALUES (?1, 'A', '70', ?2, ?2, 'DRUID', 0, 0)",
                    params![FLAVOR, name],
                )?;
            }
            for (character, name, difficulty, reset) in [
                (1, "Molten Core", "40 Player", 9_000),
                (1, "Onyxia's Lair", "40 Player", 9_000),
                (2, "Molten Core", "40 Player", 4_000), // reset already
            ] {
                c.execute(
                    "INSERT INTO lockouts (character_id, name, difficulty, reset_at, raid, as_of)
                     VALUES (?1, ?2, ?3, ?4, 1, 1)",
                    params![character, name, difficulty, reset],
                )?;
            }
            Ok(())
        })
        .unwrap();
        let bytes = build(&db, FLAVOR, 5_000).unwrap();
        let LuaValue::Table(t) = check(Slot::Briefing, &bytes).unwrap() else {
            unreachable!()
        };
        let lockouts = t.get("lockouts").unwrap().as_table().unwrap();
        assert_eq!(lockouts.array.len(), 1, "Kaelor's has reset");
        let velyra = lockouts.array[0].as_table().unwrap();
        assert_eq!(
            velyra.get("name").and_then(|v| v.as_bytes()),
            Some(&b"Velyra"[..])
        );
        let saves = velyra.get("saves").unwrap().as_table().unwrap();
        assert_eq!(saves.array.len(), 6, "two saves, three values each");
        assert_eq!(saves.array[0].as_bytes(), Some(&b"Molten Core"[..]));
        assert_eq!(saves.array[2], LuaValue::Int(9_000));
    }
}
