//! Play sessions (T14): when WoW ran, from the process watcher, and which
//! characters were played. There's no addon in v0.1 and no WTF watcher, so
//! the character is the one whose WTF folder changed during the run: each
//! character folder's newest mtime is noted at start and compared once WoW's
//! exit writes have settled.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
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
    /// Characters whose settings changed during the session, in WTF folder
    /// order. Empty while running, or if nothing changed (e.g. WoW was
    /// closed at character select).
    pub characters: Vec<CharacterRef>,
    /// WoW wrote a crash report (`<flavor>/Errors`) during the run. A process
    /// killed without one can't be told apart from a clean exit.
    pub crashed: bool,
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
/// new ones), in WTF folder order (account, realm, name).
fn changed(before: &Mtimes, after: &Mtimes) -> Vec<CharacterRef> {
    let mut out: Vec<CharacterRef> = after
        .iter()
        .filter(|(c, m)| before.get(*c).is_none_or(|b| *m > b))
        .map(|(c, _)| c.clone())
        .collect();
    out.sort_by(|a, b| (&a.account, &a.realm, &a.name).cmp(&(&b.account, &b.realm, &b.name)));
    out
}

/// A session and the game folder's state when it started.
pub struct Open {
    id: i64,
    before: Mtimes,
    /// Newest crash report in `<flavor>/Errors` at the start.
    errors_before: Option<SystemTime>,
}

/// WoW started (`started_at` from the watcher): inserts the session and notes
/// every character folder's newest mtime under `flavor_dir/WTF`, and the
/// newest crash report.
pub fn start(db: &Db, flavor: &str, flavor_dir: &Path, started_at: &str) -> AppResult<Open> {
    let before = snapshot(&flavor_dir.join("WTF"));
    let errors_before = newest_mtime(&flavor_dir.join("Errors"));
    let id = db.with_conn(|c| {
        c.execute(
            "INSERT INTO play_sessions (flavor, started_at) VALUES (?1, ?2)",
            params![flavor, started_at],
        )?;
        Ok(c.last_insert_rowid())
    })?;
    Ok(Open {
        id,
        before,
        errors_before,
    })
}

impl Open {
    /// WoW exited at `ended_at`: records the end right away, so the session
    /// stops counting.
    pub fn stop(&self, db: &Db, ended_at: &str) -> AppResult<()> {
        db.with_conn(|c| {
            c.execute(
                "UPDATE play_sessions SET ended_at = ?2 WHERE id = ?1",
                params![self.id, ended_at],
            )?;
            Ok(())
        })
    }

    /// After WoW's exit writes have settled: stores which characters changed,
    /// and whether WoW left a new crash report.
    pub fn attribute(self, db: &Db, flavor_dir: &Path) -> AppResult<()> {
        let characters = changed(&self.before, &snapshot(&flavor_dir.join("WTF")));
        let crashed = newest_mtime(&flavor_dir.join("Errors")) > self.errors_before;
        let json = serde_json::to_string(&characters).expect("characters serialize");
        db.with_conn(|c| {
            c.execute(
                "UPDATE play_sessions SET characters = ?2, crashed = ?3 WHERE id = ?1",
                params![self.id, json, crashed],
            )?;
            Ok(())
        })
    }
}

/// What the process watcher saw, with when (RFC 3339, UTC).
#[derive(Debug, Clone)]
pub enum SessionEvent {
    Started(String),
    Stopped(String),
}

/// The active flavor and its folder, looked up when an event is handled.
pub struct GameContext {
    pub flavor: String,
    pub dir: PathBuf,
}

/// Records sessions on one worker thread, strictly in event order: a stop
/// is never handled before its start, and a quick relaunch waits until the
/// previous session has been attributed, so two sessions can't cross.
/// `settle` waits for WoW's exit writes to the given WTF folder;
/// `on_change` runs after each database change. The worker ends when every
/// sender is dropped.
pub fn spawn_worker(
    db: Db,
    game: impl Fn() -> Option<GameContext> + Send + 'static,
    settle: impl Fn(&Path) + Send + 'static,
    on_change: impl Fn() + Send + 'static,
) -> (Sender<SessionEvent>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<SessionEvent>();
    let handle = std::thread::Builder::new()
        .name("sessions".into())
        .spawn(move || {
            let mut open: Option<Open> = None;
            for event in rx {
                match event {
                    SessionEvent::Started(at) => {
                        // No game folder: nothing to attribute or show.
                        let Some(g) = game() else { continue };
                        if let Ok(o) = start(&db, &g.flavor, &g.dir, &at) {
                            open = Some(o);
                            on_change();
                        }
                    }
                    SessionEvent::Stopped(at) => {
                        let Some(o) = open.take() else { continue };
                        if o.stop(&db, &at).is_ok() {
                            on_change();
                        }
                        if let Some(g) = game() {
                            settle(&g.dir.join("WTF"));
                            if o.attribute(&db, &g.dir).is_ok() {
                                on_change();
                            }
                        }
                    }
                }
            }
        })
        .expect("spawn sessions thread");
    (tx, handle)
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
            "SELECT id, flavor, started_at, ended_at, characters, crashed FROM play_sessions
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
                crashed: r.get(5)?,
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

    const SINCE: &str = "2026-10-01T00:00:00+00:00";

    /// Runs `events` through the worker and waits for it to finish.
    /// `settle` stands in for WoW's exit writes.
    fn run_worker(
        db: &Db,
        game: &Path,
        events: Vec<SessionEvent>,
        settle: impl Fn(&Path) + Send + 'static,
    ) {
        let dir = game.to_path_buf();
        let (tx, handle) = spawn_worker(
            db.clone(),
            move || {
                Some(GameContext {
                    flavor: "_classic_beta_".into(),
                    dir: dir.clone(),
                })
            },
            settle,
            || {},
        );
        for e in events {
            tx.send(e).unwrap();
        }
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn a_session_records_its_times_and_the_characters_that_changed() {
        let (_dir, root) = fixture_copy();
        let game = root.join("_classic_beta_");
        let wtf = game.join("WTF");
        let db = Db::open_in_memory().unwrap();

        let open = start(&db, "_classic_beta_", &game, "2026-10-04T19:12:00+00:00").unwrap();
        let running = list(&db, "_classic_beta_", SINCE).unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].ended_at, None);

        // An alt swap: Velyra logs out last, but the list is in WTF order.
        let now = SystemTime::now();
        touch(
            &wtf.join("Account/ACCOUNT1/Ashenvale/Thrandor/macros-cache.txt"),
            now + Duration::from_secs(10),
        );
        touch(
            &wtf.join("Account/ACCOUNT1/Ashenvale/Velyra/AddOns.txt"),
            now + Duration::from_secs(20),
        );
        open.stop(&db, "2026-10-04T20:54:00+00:00").unwrap();
        open.attribute(&db, &game).unwrap();

        let done = list(&db, "_classic_beta_", SINCE).unwrap();
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
        assert!(!done[0].crashed);
    }

    #[test]
    fn a_session_where_nothing_changed_has_no_character() {
        let (_dir, root) = fixture_copy();
        let game = root.join("_classic_beta_");
        let db = Db::open_in_memory().unwrap();
        let open = start(&db, "_classic_beta_", &game, "2026-10-04T19:12:00+00:00").unwrap();
        open.stop(&db, "2026-10-04T19:13:00+00:00").unwrap();
        open.attribute(&db, &game).unwrap();
        let s = list(&db, "_classic_beta_", SINCE).unwrap();
        assert!(s[0].characters.is_empty());
    }

    #[test]
    fn a_stop_right_after_a_start_still_ends_the_session() {
        let (_dir, root) = fixture_copy();
        let game = root.join("_classic_beta_");
        let db = Db::open_in_memory().unwrap();
        // WoW closed at character select: both events arrive back to back.
        run_worker(
            &db,
            &game,
            vec![
                SessionEvent::Started("2026-10-04T19:12:00+00:00".into()),
                SessionEvent::Stopped("2026-10-04T19:12:30+00:00".into()),
            ],
            |_| {},
        );
        let s = list(&db, "_classic_beta_", SINCE).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].ended_at.as_deref(), Some("2026-10-04T19:12:30+00:00"));
    }

    #[test]
    fn a_quick_relaunch_keeps_the_two_sessions_apart() {
        let (_dir, root) = fixture_copy();
        let game = root.join("_classic_beta_");
        let db = Db::open_in_memory().unwrap();
        // WoW is relaunched while the first run's exit writes are settling;
        // those writes (Thrandor's) belong to the first run only.
        let thrandor = game.join("WTF/Account/ACCOUNT1/Ashenvale/Thrandor/macros-cache.txt");
        run_worker(
            &db,
            &game,
            vec![
                SessionEvent::Started("2026-10-04T19:00:00+00:00".into()),
                SessionEvent::Stopped("2026-10-04T20:00:00+00:00".into()),
                SessionEvent::Started("2026-10-04T20:00:30+00:00".into()),
            ],
            move |_| touch(&thrandor, SystemTime::now() + Duration::from_secs(30)),
        );
        let s = list(&db, "_classic_beta_", SINCE).unwrap();
        assert_eq!(s.len(), 2);
        let (second, first) = (&s[0], &s[1]);
        assert_eq!(first.ended_at.as_deref(), Some("2026-10-04T20:00:00+00:00"));
        assert_eq!(
            first.characters,
            vec![character("ACCOUNT1", "Ashenvale", "Thrandor")]
        );
        assert_eq!(second.started_at, "2026-10-04T20:00:30+00:00");
        assert_eq!(second.ended_at, None);
        assert!(second.characters.is_empty());
    }

    #[test]
    fn a_new_crash_report_marks_the_session() {
        let (_dir, root) = fixture_copy();
        let game = root.join("_classic_beta_");
        // An old report from before the session doesn't count.
        std::fs::create_dir_all(game.join("Errors")).unwrap();
        std::fs::write(game.join("Errors/old.txt"), "old").unwrap();
        touch(
            &game.join("Errors/old.txt"),
            SystemTime::now() - Duration::from_secs(3600),
        );
        let db = Db::open_in_memory().unwrap();
        let open = start(&db, "_classic_beta_", &game, "2026-10-04T19:12:00+00:00").unwrap();
        std::fs::write(game.join("Errors/crash.txt"), "WowB.exe crashed").unwrap();
        open.stop(&db, "2026-10-04T19:40:00+00:00").unwrap();
        open.attribute(&db, &game).unwrap();
        let s = list(&db, "_classic_beta_", SINCE).unwrap();
        assert!(s[0].crashed);
    }

    #[test]
    fn unfinished_sessions_from_an_earlier_run_are_dropped() {
        let (_dir, root) = fixture_copy();
        let game = root.join("_classic_beta_");
        let db = Db::open_in_memory().unwrap();
        start(&db, "_classic_beta_", &game, "2026-10-03T19:00:00+00:00")
            .unwrap()
            .stop(&db, "2026-10-03T20:00:00+00:00")
            .unwrap();
        // The app quits mid-session.
        start(&db, "_classic_beta_", &game, "2026-10-04T19:00:00+00:00").unwrap();

        drop_unfinished(&db).unwrap();
        let s = list(&db, "_classic_beta_", SINCE).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].started_at, "2026-10-03T19:00:00+00:00");
    }

    #[test]
    fn list_filters_by_flavor_and_start() {
        let (_dir, root) = fixture_copy();
        let game = root.join("_classic_beta_");
        let db = Db::open_in_memory().unwrap();
        for (flavor, at) in [
            ("_classic_beta_", "2026-09-20T10:00:00+00:00"),
            ("_classic_era_", "2026-10-02T10:00:00+00:00"),
            ("_classic_beta_", "2026-10-02T10:00:00+00:00"),
        ] {
            start(&db, flavor, &game, at)
                .unwrap()
                .stop(&db, at)
                .unwrap();
        }
        let s = list(&db, "_classic_beta_", "2026-09-27T00:00:00+00:00").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].started_at, "2026-10-02T10:00:00+00:00");
    }
}
