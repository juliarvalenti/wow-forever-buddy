//! Local SQLite database (spec §6): one connection behind a mutex, used from
//! blocking threads. Feature tickets add their own migrations to the same db.

pub mod copies;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OptionalExtension};
use rusqlite_migration::{Migrations, M};

use crate::error::{AppError, AppResult};

const MIGRATION_LIST: &[M<'_>] = &[
    M::up(include_str!("migrations/001_init.sql")),
    M::up(include_str!("migrations/002_hash_cache_key.sql")),
    M::up(include_str!("migrations/003_play_sessions.sql")),
    M::up(include_str!("migrations/004_addon_data.sql")),
    M::up(include_str!("migrations/005_char_bags.sql")),
    M::up(include_str!("migrations/006_bank_alt.sql")),
    M::up(include_str!("migrations/007_ah_prices.sql")),
    M::up(include_str!("migrations/008_bridge.sql")),
    M::up(include_str!("migrations/009_quests.sql")),
    M::up(include_str!("migrations/010_login_notes.sql")),
];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATION_LIST);
/// The schema version this build migrates to (`PRAGMA user_version`, which
/// rusqlite_migration sets to the number of applied migrations).
const LATEST_VERSION: i64 = MIGRATION_LIST.len() as i64;

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

/// Meta key set when the db was recreated, so derived indexes get rebuilt.
pub const NEEDS_REINDEX: &str = "needs_reindex";
/// Meta key: the date of the daily copy a corrupt db was restored from.
pub const RESTORED_FROM_COPY: &str = "restored_from_copy";

impl Db {
    /// Opens (creating if needed) and migrates the db. A file SQLite reports as
    /// corrupt or not-a-database is moved aside (kept for inspection) and
    /// replaced by the newest daily copy that opens cleanly (`copies`), or by
    /// a fresh db if there's none. The v0.1 tables are caches or rebuildable
    /// from backup manifests; the v0.2 addon data isn't, which is what the
    /// copies are for. After a quarantine the `needs_reindex` meta flag is
    /// set, so the backup store rebuilds its index before it prunes anything,
    /// and `restored_from_copy` names the copy's date if one was used.
    pub fn open(path: &Path) -> AppResult<Self> {
        // `false`: copying it showed the db is corrupt, so don't migrate it.
        let sound = backup_before_migrating(path)?;
        let first = sound.then(|| Self::open_and_migrate(path));
        let mut quarantined = false;
        let mut restored = None;
        let conn = match first {
            Some(Ok(conn)) => conn,
            Some(Err(e)) if !is_corruption(&e) => return Err(migration_error(e)),
            _ => {
                quarantine(path)?;
                quarantined = true;
                restored = copies::restore_newest(path, LATEST_VERSION);
                match Self::open_and_migrate(path) {
                    Ok(conn) => conn,
                    // A copy that passed the check but still won't migrate:
                    // set it aside too and start fresh rather than not start.
                    Err(e) if restored.is_some() && is_corruption(&e) => {
                        quarantine(path)?;
                        restored = None;
                        Self::open_and_migrate(path).map_err(migration_error)?
                    }
                    Err(e) => return Err(migration_error(e)),
                }
            }
        };
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.set_meta("last_opened_by", env!("CARGO_PKG_VERSION"))?;
        if quarantined {
            db.set_meta(NEEDS_REINDEX, "1")?;
        }
        if let Some(day) = restored {
            db.set_meta(RESTORED_FROM_COPY, &day.format("%Y-%m-%d").to_string())?;
        }
        Ok(db)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        MIGRATIONS.to_latest(&mut conn).map_err(migration_error)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn open_and_migrate(path: &Path) -> Result<Connection, rusqlite_migration::Error> {
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        MIGRATIONS.to_latest(&mut conn)?;
        Ok(conn)
    }

    /// Runs `f` on the connection, blocking the current thread. For code that
    /// already runs off the async runtime (job queue, setup).
    pub fn with_conn<T>(&self, f: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
        let mut conn = self.conn.lock().expect("db lock poisoned");
        f(&mut conn)
    }

    /// Runs `f` on a blocking thread so async commands never stall the runtime.
    #[allow(dead_code)] // first used by the backup commands (T7)
    pub async fn call<T, F>(&self, f: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
    {
        let db = self.clone();
        tauri::async_runtime::spawn_blocking(move || db.with_conn(f))
            .await
            .map_err(|e| AppError::Db(format!("db task failed: {e}")))?
    }

    pub fn get_meta(&self, key: &str) -> AppResult<Option<String>> {
        self.with_conn(|c| {
            Ok(
                c.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
                    .optional()?,
            )
        })
    }

    pub fn set_meta(&self, key: &str, value: &str) -> AppResult<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [key, value],
            )?;
            Ok(())
        })
    }
}

fn is_corruption(e: &rusqlite_migration::Error) -> bool {
    let sqlite_err = match e {
        rusqlite_migration::Error::RusqliteError { err, .. } => err,
        _ => return false,
    };
    matches!(
        sqlite_err.sqlite_error_code(),
        Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt)
    )
}

fn migration_error(e: rusqlite_migration::Error) -> AppError {
    AppError::Db(e.to_string())
}

/// Before migrating an existing db (schema older than this build's), takes a
/// copy of it as it is, so irreplaceable history survives a migration that
/// goes wrong. If the copy can't be made (disk full, permissions), the
/// migration doesn't run: the app shows its startup error instead of risking
/// the only copy.
///
/// Returns `false` if copying showed the db is corrupt (a readable header
/// over damaged pages): `open` then quarantines and restores it rather than
/// migrating it. A file that can't even be read is left to `open` too.
fn backup_before_migrating(path: &Path) -> AppResult<bool> {
    if !path.exists() {
        return Ok(true);
    }
    let Ok(conn) = Connection::open(path) else {
        return Ok(true);
    };
    let Ok(version) = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)) else {
        return Ok(true);
    };
    if version == 0 || version >= LATEST_VERSION {
        return Ok(true);
    }
    let copy = copies::take_pre_migration(&conn, &copies::dir_for(path), version).map_err(|e| {
        AppError::Db(format!(
            "couldn't back up the database before upgrading it, so it wasn't upgraded: {e}"
        ))
    })?;
    Ok(copy.is_some())
}

/// Moves a broken db (and its WAL/SHM side files) out of the way, keeping it
/// for inspection. Names never collide: a second quarantine in the same
/// second gets `-2`, `-3`… rather than failing (Windows) or overwriting the
/// evidence (macOS).
fn quarantine(path: &Path) -> AppResult<()> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let side = |suffix: &str| {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        std::path::PathBuf::from(p)
    };
    let tag = (1..)
        .map(|n| match n {
            1 => format!(".corrupt-{stamp}"),
            n => format!(".corrupt-{stamp}-{n}"),
        })
        .find(|tag| {
            ["", "-wal", "-shm"]
                .iter()
                .all(|s| !side(&format!("{tag}{s}")).exists())
        })
        .expect("an unused name exists");
    for suffix in ["", "-wal", "-shm"] {
        let from = side(suffix);
        if from.exists() {
            std::fs::rename(&from, side(&format!("{tag}{suffix}")))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(db: &Db) -> Vec<String> {
        db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table'
                 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )?;
            let names = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            Ok(names)
        })
        .unwrap()
    }

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }

    #[test]
    fn open_creates_schema_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");

        let db = Db::open(&path).unwrap();
        assert_eq!(
            tables(&db),
            [
                "adventure_events",
                "adventures",
                "ah_latest",
                "ah_prices",
                "ah_scans",
                "ah_watch",
                "bridge_receipts",
                "bridge_slots",
                "char_bags",
                "char_items",
                "char_mail",
                "char_quests_done",
                "char_snapshots",
                "characters",
                "file_hash_cache",
                "gold_points",
                "ingest_state",
                "items",
                "lockouts",
                "meta",
                "play_sessions",
                "professions",
                "snapshots",
                "write_audit"
            ]
        );
        db.set_meta("probe", "kept").unwrap();
        drop(db);

        let db = Db::open(&path).unwrap();
        assert_eq!(db.get_meta("probe").unwrap().as_deref(), Some("kept"));
        assert_eq!(
            db.get_meta("last_opened_by").unwrap().as_deref(),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn uses_wal_and_foreign_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(&tmp.path().join("buddy.db")).unwrap();
        db.with_conn(|c| {
            let mode: String = c.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
            let fk: i64 = c.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
            assert_eq!(mode.to_lowercase(), "wal");
            assert_eq!(fk, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn corrupt_file_is_quarantined_and_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        std::fs::write(&path, vec![0xAB; 8192]).unwrap();

        let db = Db::open(&path).unwrap();
        assert_eq!(tables(&db).len(), 24);
        assert_eq!(
            db.get_meta(RESTORED_FROM_COPY).unwrap(),
            None,
            "no copy to use"
        );
        let quarantined = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("buddy.db.corrupt-")
            });
        assert!(quarantined);
        assert_eq!(db.get_meta(NEEDS_REINDEX).unwrap().as_deref(), Some("1"));

        // A healthy reopen doesn't set it.
        let fresh = tempfile::tempdir().unwrap();
        let db = Db::open(&fresh.path().join("buddy.db")).unwrap();
        assert_eq!(db.get_meta(NEEDS_REINDEX).unwrap(), None);
    }

    /// V5: a corrupt db comes back from the newest daily copy, data and all,
    /// instead of starting empty.
    #[test]
    fn corrupt_db_is_restored_from_the_newest_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        let db = Db::open(&path).unwrap();
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO characters
                   (flavor, account, group_dir, char_dir, name, first_seen, last_seen)
                 VALUES ('_classic_beta_', 'ACCOUNT1', '70', 'Thrandor', 'Thrandor', 1, 2)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 4).unwrap();
        copies::take_daily(&db, &copies::dir_for(&path), day).unwrap();
        drop(db);
        std::fs::write(&path, vec![0xAB; 8192]).unwrap();
        for side in ["buddy.db-wal", "buddy.db-shm"] {
            let _ = std::fs::remove_file(tmp.path().join(side));
        }

        let db = Db::open(&path).unwrap();
        let name: String = db
            .with_conn(|c| Ok(c.query_row("SELECT name FROM characters", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(name, "Thrandor", "the addon data survived");
        assert_eq!(
            db.get_meta(RESTORED_FROM_COPY).unwrap().as_deref(),
            Some("2026-10-04")
        );
        assert_eq!(db.get_meta(NEEDS_REINDEX).unwrap().as_deref(), Some("1"));
    }

    /// Migration 004: a character is its folder, per flavor (the same folders
    /// in two flavors are two rows), and deleting a character takes its data
    /// with it.
    #[test]
    fn characters_are_per_folder_and_flavor_and_cascade() {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            c.execute_batch("PRAGMA foreign_keys = ON")?;
            let insert = "INSERT INTO characters
                  (flavor, account, group_dir, char_dir, name, surname, first_seen, last_seen)
                VALUES (?1, 'A', '70', ?2, 'Ellygie', ?3, 1, 1)";
            for flavor in ["_classic_beta_", "_classic_"] {
                c.execute(insert, (flavor, "Ellygie-Vargur", "Vargur"))?;
            }
            // Same name without the surname is a different folder, so a
            // different character.
            c.execute(insert, ("_classic_", "Ellygie", None::<&str>))?;
            let dup = c.execute(insert, ("_classic_", "Ellygie-Vargur", "Vargur"));
            assert!(dup.is_err(), "unique per flavor and folder");
            // Folder names compare case-insensitively, like on Windows.
            let recased = c.execute(insert, ("_classic_", "ellygie-vargur", "Vargur"));
            assert!(recased.is_err(), "a case change is the same character");
            let found: i64 = c.query_row(
                "SELECT count(*) FROM characters WHERE flavor = '_classic_'
                   AND account = 'a' AND group_dir = '70' AND char_dir = 'ELLYGIE-VARGUR'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(found, 1, "lookups use the column's NOCASE collation");
            c.execute(
                "INSERT INTO gold_points (character_id, at, money) VALUES (1, 10, 500)",
                [],
            )?;
            c.execute("DELETE FROM characters WHERE id = 1", [])?;
            let left: i64 = c.query_row("SELECT count(*) FROM gold_points", [], |r| r.get(0))?;
            assert_eq!(left, 0, "cascades");
            Ok(())
        })
        .unwrap();
    }

    /// #41 review: opening a db with migrations pending first copies it as
    /// it was; an up-to-date or brand-new db isn't copied.
    #[test]
    fn a_copy_is_taken_before_migrating() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        let mut conn = Connection::open(&path).unwrap();
        Migrations::from_slice(&MIGRATION_LIST[..3])
            .to_latest(&mut conn)
            .unwrap();
        drop(conn);

        drop(Db::open(&path).unwrap());
        let dir = copies::dir_for(&path);
        let pre: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("pre-migration-v3-")
            })
            .collect();
        assert_eq!(pre.len(), 1);
        let version: i64 = Connection::open(&pre[0])
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3, "the copy is the db as it was before migrating");

        drop(Db::open(&path).unwrap());
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "no copy when up to date"
        );
    }

    /// #41 review: an older db with a readable header but a damaged page is
    /// quarantined (and the app starts), not a startup failure from the
    /// pre-migration copy.
    #[test]
    fn a_damaged_older_db_is_quarantined_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        let mut conn = Connection::open(&path).unwrap();
        Migrations::from_slice(&MIGRATION_LIST[..3])
            .to_latest(&mut conn)
            .unwrap();
        // Enough rows to span several pages.
        for i in 0..400 {
            conn.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)",
                (format!("k{i}"), "x".repeat(200)),
            )
            .unwrap();
        }
        let page: usize = conn
            .query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0))
            .unwrap() as usize;
        drop(conn);
        // Zero the last page (holding the rows just inserted, so every full
        // read hits it), leaving the header (page 1) intact.
        let mut bytes = std::fs::read(&path).unwrap();
        let pages = bytes.len() / page;
        assert!(pages > 4);
        bytes[(pages - 1) * page..].fill(0);
        std::fs::write(&path, bytes).unwrap();

        let db = Db::open(&path).expect("starts instead of failing");
        assert_eq!(tables(&db).len(), 24);
        assert_eq!(db.get_meta(NEEDS_REINDEX).unwrap().as_deref(), Some("1"));
        let quarantined = std::fs::read_dir(tmp.path()).unwrap().flatten().any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("buddy.db.corrupt-")
        });
        assert!(quarantined, "the damaged file is kept as evidence");
    }

    /// #41 review: two quarantines in the same second keep both files.
    #[test]
    fn quarantine_names_never_collide() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        for n in 0..2 {
            std::fs::write(&path, format!("broken {n}")).unwrap();
            quarantine(&path).unwrap();
        }
        let kept: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("buddy.db.corrupt-"))
            .collect();
        assert_eq!(kept.len(), 2, "{kept:?}");
    }

    #[test]
    fn newer_schema_is_an_error_not_a_wipe() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        drop(Db::open(&path).unwrap());
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();

        assert!(matches!(Db::open(&path), Err(AppError::Db(_))));
        assert!(
            path.exists(),
            "a db from a newer build must not be discarded"
        );
    }

    #[test]
    fn call_runs_off_thread() {
        let db = Db::open_in_memory().unwrap();
        let count = tauri::async_runtime::block_on(db.call(|c| {
            Ok(c.query_row("SELECT count(*) FROM snapshots", [], |r| r.get::<_, i64>(0))?)
        }))
        .unwrap();
        assert_eq!(count, 0);
    }
}
