//! Play sessions (T14): when WoW ran, from the process watcher, and which
//! characters were played. There's no addon in v0.1 and no WTF watcher, so
//! the character is the one whose WTF folder changed during the run: each
//! character folder's newest mtime is noted at start and compared once WoW's
//! exit writes have settled.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use crate::db::Db;
use crate::error::AppResult;

/// One character folder: `WTF/Account/<account>/<realm>/<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct CharacterRef {
    pub account: String,
    pub realm: String,
    pub name: String,
}

/// A character found in the WTF folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct WtfCharacter {
    #[serde(flatten)]
    pub character: CharacterRef,
    /// When any file in its folder last changed (RFC 3339, UTC): close to
    /// when it was last logged out.
    pub last_played: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PlaySession {
    /// A row id; `u32` because specta won't send an `i64` to TypeScript, and
    /// four billion sessions is plenty.
    pub id: u32,
    pub flavor: String,
    /// RFC 3339, UTC.
    pub started_at: String,
    /// `None` while WoW is still running.
    pub ended_at: Option<String>,
    /// Characters whose settings changed during the session, last written
    /// (usually the last one logged out) first. Empty while running, or if
    /// nothing changed.
    pub characters: Vec<CharacterRef>,
}

/// Emitted when a session starts, ends, or learns its characters.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct SessionsChanged;

type Mtimes = HashMap<CharacterRef, SystemTime>;

fn subdirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| !n.eq_ignore_ascii_case("SavedVariables"))
        .collect();
    names.sort();
    names
}

fn newest_mtime(dir: &Path) -> Option<SystemTime> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

/// Every character folder under `wtf`, with its newest file mtime.
fn character_mtimes(wtf: &Path) -> Vec<(CharacterRef, Option<SystemTime>)> {
    let accounts_dir = wtf.join("Account");
    let mut out = Vec::new();
    for account in subdirs(&accounts_dir) {
        let account_dir = accounts_dir.join(&account);
        for realm in subdirs(&account_dir) {
            for name in subdirs(&account_dir.join(&realm)) {
                let mtime = newest_mtime(&account_dir.join(&realm).join(&name));
                let character = CharacterRef {
                    account: account.clone(),
                    realm: realm.clone(),
                    name,
                };
                out.push((character, mtime));
            }
        }
    }
    out
}

fn rfc3339(t: SystemTime) -> String {
    DateTime::<Utc>::from(t).to_rfc3339()
}

/// The characters in the WTF folder, most recently played first.
pub fn wtf_characters(wtf: &Path) -> Vec<WtfCharacter> {
    let mut list: Vec<(CharacterRef, Option<SystemTime>)> = character_mtimes(wtf);
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
    list.into_iter()
        .map(|(character, mtime)| WtfCharacter {
            character,
            last_played: mtime.map(rfc3339),
        })
        .collect()
}

fn snapshot(wtf: &Path) -> Mtimes {
    character_mtimes(wtf)
        .into_iter()
        .filter_map(|(c, m)| Some((c, m?)))
        .collect()
}

/// Characters whose folder changed between `before` and `after` (including
/// new ones), last written first.
fn changed(before: &Mtimes, after: &Mtimes) -> Vec<CharacterRef> {
    let mut out: Vec<(&CharacterRef, SystemTime)> = after
        .iter()
        .filter(|(c, m)| before.get(*c).is_none_or(|b| *m > b))
        .map(|(c, m)| (c, *m))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
    out.into_iter().map(|(c, _)| c.clone()).collect()
}

/// The session in progress, and the WTF state when it started.
struct Open {
    id: i64,
    before: Mtimes,
}

/// Records sessions as the process watcher reports them.
#[derive(Default)]
pub struct SessionTracker {
    open: Mutex<Option<Open>>,
}

impl SessionTracker {
    /// WoW started (`started_at` from the watcher). Notes every character
    /// folder's newest mtime.
    pub fn started(&self, db: &Db, flavor: &str, wtf: &Path, started_at: &str) -> AppResult<()> {
        let before = snapshot(wtf);
        let id = db.with_conn(|c| {
            c.execute(
                "INSERT INTO play_sessions (flavor, started_at) VALUES (?1, ?2)",
                params![flavor, started_at],
            )?;
            Ok(c.last_insert_rowid())
        })?;
        *self.open.lock().expect("sessions lock poisoned") = Some(Open { id, before });
        Ok(())
    }

    /// WoW exited at `ended_at`. Records the end right away, so the session
    /// stops counting; call `attribute` once WTF has settled.
    pub fn stopped(&self, db: &Db, ended_at: &str) -> AppResult<()> {
        let open = self.open.lock().expect("sessions lock poisoned");
        if let Some(open) = open.as_ref() {
            db.with_conn(|c| {
                c.execute(
                    "UPDATE play_sessions SET ended_at = ?2 WHERE id = ?1",
                    params![open.id, ended_at],
                )?;
                Ok(())
            })?;
        }
        Ok(())
    }

    /// After WoW's exit writes have settled: stores which characters changed.
    pub fn attribute(&self, db: &Db, wtf: &Path) -> AppResult<()> {
        let Some(open) = self.open.lock().expect("sessions lock poisoned").take() else {
            return Ok(());
        };
        let characters = changed(&open.before, &snapshot(wtf));
        let json = serde_json::to_string(&characters).expect("characters serialize");
        db.with_conn(|c| {
            c.execute(
                "UPDATE play_sessions SET characters = ?2 WHERE id = ?1",
                params![open.id, json],
            )?;
            Ok(())
        })
    }
}

/// Drops sessions left open by an earlier run of the app: their end (and so
/// their length) is unknown, and showing a made-up duration would mislead.
pub fn drop_unfinished(db: &Db) -> AppResult<()> {
    db.with_conn(|c| {
        c.execute("DELETE FROM play_sessions WHERE ended_at IS NULL", [])?;
        Ok(())
    })
}

/// Sessions of `flavor` that started at or after `since` (RFC 3339), newest
/// first.
pub fn list(db: &Db, flavor: &str, since: &str) -> AppResult<Vec<PlaySession>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT id, flavor, started_at, ended_at, characters FROM play_sessions
             WHERE flavor = ?1 COLLATE NOCASE AND started_at >= ?2
             ORDER BY started_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![flavor, since], |r| {
            let characters: String = r.get(4)?;
            Ok(PlaySession {
                id: r.get(0)?,
                flavor: r.get(1)?,
                started_at: r.get(2)?,
                ended_at: r.get(3)?,
                // A row we can't read just shows no character.
                characters: serde_json::from_str(&characters).unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_copy;
    use std::time::Duration;

    fn touch(path: &Path, at: SystemTime) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    fn character(account: &str, realm: &str, name: &str) -> CharacterRef {
        CharacterRef {
            account: account.into(),
            realm: realm.into(),
            name: name.into(),
        }
    }

    #[test]
    fn lists_characters_from_wtf_folders_last_played_first() {
        let (_dir, root) = fixture_copy();
        let wtf = root.join("_classic_beta_/WTF");
        let later = SystemTime::now() + Duration::from_secs(60);
        touch(
            &wtf.join("Account/ACCOUNT1/Ashenvale/Velyra/AddOns.txt"),
            later,
        );

        let list = wtf_characters(&wtf);
        let names: Vec<&str> = list.iter().map(|c| c.character.name.as_str()).collect();
        assert_eq!(names[0], "Velyra");
        for expected in ["Thrandor", "Velyra", "Brannic", "Fizzwick", "Lúthien"] {
            assert!(
                names.contains(&expected),
                "{expected} missing from {names:?}"
            );
        }
        // Account-wide SavedVariables is not a realm.
        assert!(list.iter().all(|c| c.character.realm != "SavedVariables"));
        assert!(list[0].last_played.is_some());
    }

    #[test]
    fn a_session_records_its_times_and_the_character_that_changed() {
        let (_dir, root) = fixture_copy();
        let wtf = root.join("_classic_beta_/WTF");
        let db = Db::open_in_memory().unwrap();
        let tracker = SessionTracker::default();

        tracker
            .started(&db, "_classic_beta_", &wtf, "2026-10-04T19:12:00+00:00")
            .unwrap();
        let open = list(&db, "_classic_beta_", "2026-10-01T00:00:00+00:00").unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].ended_at, None);

        // Thrandor logs out last; Velyra was played earlier in the run.
        let now = SystemTime::now();
        touch(
            &wtf.join("Account/ACCOUNT1/Ashenvale/Velyra/AddOns.txt"),
            now + Duration::from_secs(10),
        );
        touch(
            &wtf.join("Account/ACCOUNT1/Ashenvale/Thrandor/macros-cache.txt"),
            now + Duration::from_secs(20),
        );
        tracker.stopped(&db, "2026-10-04T20:54:00+00:00").unwrap();
        tracker.attribute(&db, &wtf).unwrap();

        let done = list(&db, "_classic_beta_", "2026-10-01T00:00:00+00:00").unwrap();
        assert_eq!(
            done[0].ended_at.as_deref(),
            Some("2026-10-04T20:54:00+00:00")
        );
        assert_eq!(
            done[0].characters,
            vec![
                character("ACCOUNT1", "Ashenvale", "Thrandor"),
                character("ACCOUNT1", "Ashenvale", "Velyra"),
            ]
        );
    }

    #[test]
    fn a_session_where_nothing_changed_has_no_character() {
        let (_dir, root) = fixture_copy();
        let wtf = root.join("_classic_beta_/WTF");
        let db = Db::open_in_memory().unwrap();
        let tracker = SessionTracker::default();
        tracker
            .started(&db, "_classic_beta_", &wtf, "2026-10-04T19:12:00+00:00")
            .unwrap();
        tracker.stopped(&db, "2026-10-04T19:13:00+00:00").unwrap();
        tracker.attribute(&db, &wtf).unwrap();
        let s = list(&db, "_classic_beta_", "2026-10-01T00:00:00+00:00").unwrap();
        assert!(s[0].characters.is_empty());
    }

    #[test]
    fn unfinished_sessions_from_an_earlier_run_are_dropped() {
        let (_dir, root) = fixture_copy();
        let wtf = root.join("_classic_beta_/WTF");
        let db = Db::open_in_memory().unwrap();
        let first = SessionTracker::default();
        first
            .started(&db, "_classic_beta_", &wtf, "2026-10-03T19:00:00+00:00")
            .unwrap();
        first.stopped(&db, "2026-10-03T20:00:00+00:00").unwrap();
        // The app quits mid-session.
        first
            .started(&db, "_classic_beta_", &wtf, "2026-10-04T19:00:00+00:00")
            .unwrap();

        drop_unfinished(&db).unwrap();
        let s = list(&db, "_classic_beta_", "2026-10-01T00:00:00+00:00").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].started_at, "2026-10-03T19:00:00+00:00");
    }

    #[test]
    fn list_filters_by_flavor_and_start() {
        let (_dir, root) = fixture_copy();
        let wtf = root.join("_classic_beta_/WTF");
        let db = Db::open_in_memory().unwrap();
        let t = SessionTracker::default();
        for (flavor, at) in [
            ("_classic_beta_", "2026-09-20T10:00:00+00:00"),
            ("_classic_era_", "2026-10-02T10:00:00+00:00"),
            ("_classic_beta_", "2026-10-02T10:00:00+00:00"),
        ] {
            t.started(&db, flavor, &wtf, at).unwrap();
            t.stopped(&db, at).unwrap();
        }
        let s = list(&db, "_classic_beta_", "2026-09-27T00:00:00+00:00").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].started_at, "2026-10-02T10:00:00+00:00");
    }
}
