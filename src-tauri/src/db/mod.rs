//! Local SQLite database (spec §6): one connection behind a mutex, used from
//! blocking threads. Feature tickets add their own migrations to the same db.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OptionalExtension};
use rusqlite_migration::{Migrations, M};

use crate::error::{AppError, AppResult};

const MIGRATION_LIST: &[M<'_>] = &[
    M::up(include_str!("migrations/001_init.sql")),
    M::up(include_str!("migrations/002_hash_cache_key.sql")),
];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATION_LIST);

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

impl Db {
    /// Opens (creating if needed) and migrates the db. A file SQLite reports as
    /// corrupt or not-a-database is moved aside and replaced with a fresh one:
    /// everything in it is either a cache or rebuildable from backup manifests
    /// (except the write audit, which starts over). After a quarantine the
    /// `needs_reindex` meta flag is set, and the backup store rebuilds its
    /// index before it prunes anything.
    pub fn open(path: &Path) -> AppResult<Self> {
        let mut quarantined = false;
        let conn = match Self::open_and_migrate(path) {
            Ok(conn) => conn,
            Err(e) if is_corruption(&e) => {
                quarantine(path)?;
                quarantined = true;
                Self::open_and_migrate(path).map_err(migration_error)?
            }
            Err(e) => return Err(migration_error(e)),
        };
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.set_meta("last_opened_by", env!("CARGO_PKG_VERSION"))?;
        if quarantined {
            db.set_meta(NEEDS_REINDEX, "1")?;
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

/// Moves a broken db (and its WAL/SHM side files) out of the way, keeping it for inspection.
fn quarantine(path: &Path) -> AppResult<()> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for suffix in ["", "-wal", "-shm"] {
        let mut from = path.as_os_str().to_owned();
        from.push(suffix);
        let from = std::path::PathBuf::from(from);
        if from.exists() {
            let mut to = path.as_os_str().to_owned();
            to.push(format!(".corrupt-{stamp}{suffix}"));
            std::fs::rename(&from, to)?;
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
            ["file_hash_cache", "meta", "snapshots", "write_audit"]
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
        assert_eq!(tables(&db).len(), 4);
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
