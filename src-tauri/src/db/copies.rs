//! Daily copies of the database (docs/specs/v0.2-addon.md §4, "The database
//! stops being disposable"). Gold history and adventures can't be rebuilt
//! from the game's files, so once a day the db is copied with `VACUUM INTO`
//! to `<local data>/db-copies/buddy-<date>.db`, keeping the last 7. When the
//! db is found corrupt at startup, the newest copy that opens cleanly is
//! restored; ingest's gap-fill then replays the backup snapshots newer than
//! that copy (the copy carries its own `last_replayed_snapshot`), so nothing
//! between the copy and the corruption is lost.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::db::Db;
use crate::error::{AppError, AppResult};

/// How many daily copies are kept.
pub const KEEP: usize = 7;
const PREFIX: &str = "buddy-";
const SUFFIX: &str = ".db";
const PRE_MIGRATION: &str = "pre-migration-";
const KEEP_PRE_MIGRATION: usize = 3;

/// Where the copies of the db at `db_path` live: a sibling folder.
///
/// The copies hold everything the db does, including other players' mail
/// senders and subjects (spec §4), so this folder must never go into a zip
/// export, a log or a bug report.
pub fn dir_for(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(|p| p.join("db-copies"))
        .unwrap_or_else(|| PathBuf::from("db-copies"))
}

fn file_name(day: NaiveDate) -> String {
    format!("{PREFIX}{}{SUFFIX}", day.format("%Y-%m-%d"))
}

/// The copies in `dir`, newest first, with their dates. Anything else in the
/// folder (a half-written temp file, something a user put there) is ignored.
pub fn list(dir: &Path) -> Vec<(NaiveDate, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut copies: Vec<(NaiveDate, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let date = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
            let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            Some((day, e.path()))
        })
        .collect();
    copies.sort_by_key(|c| std::cmp::Reverse(c.0));
    copies
}

/// Takes today's copy if there isn't one yet, then keeps only the newest
/// `KEEP`. Returns the new copy's path, or `None` if today's already exists.
///
/// `VACUUM INTO` writes a clean, compacted db from a consistent read, so it's
/// safe while the app runs. It writes to a temp name first and is renamed
/// into place, so a crash never leaves a half-written file that looks like a
/// real copy.
///
/// It runs under the db's mutex, so other db calls wait for it: a second or
/// two once a day at v0.2 sizes. Worth revisiting if the db grows a lot.
pub fn take_daily(db: &Db, dir: &Path, today: NaiveDate) -> AppResult<Option<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    // Temp files a crash left behind on any earlier day.
    for entry in std::fs::read_dir(dir)?.flatten() {
        if entry.file_name().to_string_lossy().ends_with(".wfb-tmp") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let target = dir.join(file_name(today));
    if target.exists() {
        return Ok(None);
    }
    let tmp = dir.join(format!(".{}.wfb-tmp", file_name(today)));
    let _ = std::fs::remove_file(&tmp);
    if !db.with_conn(|c| vacuum_into(c, &tmp).map_err(AppError::from))? {
        let _ = std::fs::remove_file(&tmp);
        return Err(AppError::Db(
            "the database reads as damaged; no copy taken today".into(),
        ));
    }
    std::fs::rename(&tmp, &target)?;
    for (_, old) in list(dir).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(target))
}

/// Why the safety copy before an update couldn't be made, in terms the
/// startup screen can say plainly (design IMPLEMENTING.md §6, `?case=copy`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CopyFailure {
    /// The drive is full. `free_bytes` is `None` if it couldn't be measured.
    DiskFull {
        free_bytes: Option<f64>,
        needed_bytes: f64,
    },
    /// Another program holds the file (antivirus, a sync client, a second
    /// copy of the app), or access was refused. Usually passes.
    Locked,
    /// Anything else; the message is for "Error details".
    Other { message: String },
}

impl std::fmt::Display for CopyFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CopyFailure::DiskFull { .. } => f.write_str("the drive is full"),
            CopyFailure::Locked => f.write_str("another program is using the file"),
            CopyFailure::Other { message } => f.write_str(message),
        }
    }
}

/// A copy attempt's raw error, kept typed so it can be classified.
#[derive(Debug)]
enum CopyErr {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
}

impl From<std::io::Error> for CopyErr {
    fn from(e: std::io::Error) -> Self {
        CopyErr::Io(e)
    }
}

impl std::fmt::Display for CopyErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CopyErr::Sqlite(e) => write!(f, "{e}"),
            CopyErr::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<CopyErr> for AppError {
    fn from(e: CopyErr) -> Self {
        match e {
            CopyErr::Sqlite(e) => e.into(),
            CopyErr::Io(e) => e.into(),
        }
    }
}

/// Disk full: ENOSPC (28), and Windows ERROR_HANDLE_DISK_FULL (39) and
/// ERROR_DISK_FULL (112). Locked: Windows access denied (5), sharing and
/// lock violations (32, 33), or a permission error anywhere.
fn classify(e: &CopyErr, needed: u64, dir: &Path) -> CopyFailure {
    let full = || CopyFailure::DiskFull {
        free_bytes: free_space(dir).map(|b| b as f64),
        needed_bytes: needed as f64,
    };
    match e {
        CopyErr::Io(io) => match (io.kind(), io.raw_os_error()) {
            (std::io::ErrorKind::StorageFull, _) => full(),
            (_, Some(28)) if cfg!(unix) => full(),
            (_, Some(39 | 112)) if cfg!(windows) => full(),
            (std::io::ErrorKind::PermissionDenied, _) => CopyFailure::Locked,
            (_, Some(5 | 32 | 33)) if cfg!(windows) => CopyFailure::Locked,
            _ => CopyFailure::Other {
                message: io.to_string(),
            },
        },
        CopyErr::Sqlite(sq) => match sq.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DiskFull) => full(),
            Some(
                rusqlite::ErrorCode::DatabaseBusy
                | rusqlite::ErrorCode::DatabaseLocked
                | rusqlite::ErrorCode::PermissionDenied
                | rusqlite::ErrorCode::ReadOnly,
            ) => CopyFailure::Locked,
            _ => CopyFailure::Other {
                message: sq.to_string(),
            },
        },
    }
}

/// Free space on the drive holding `dir`: the disk with the longest mount
/// point that `dir` is under.
fn free_space(dir: &Path) -> Option<u64> {
    let dir = dunce::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .filter(|d| dir.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// Bytes a copy of the db needs: the file plus its write-ahead log.
fn db_size(db_path: &Path) -> u64 {
    let wal = {
        let mut p = db_path.as_os_str().to_owned();
        p.push("-wal");
        PathBuf::from(p)
    };
    [db_path, wal.as_path()]
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum()
}

/// Before migrations run on an existing db (`from_version` > 0 and below
/// the latest), a copy of it as it is: `pre-migration-v<from>-<stamp>.db`,
/// keeping the newest 3. Not a daily copy, so restore never picks it on its
/// own; it's there to recover by hand if a migration ever goes wrong.
///
/// `VACUUM INTO` first; if that fails for any reason but corruption, a plain
/// file copy (after checkpointing the WAL into the main file), which often
/// works when VACUUM doesn't. Only if both fail is it an
/// `AppError::UpgradeCopyFailed`, classified from the last attempt.
///
/// Returns `None` if the db itself turns out to be corrupt while copying
/// (a readable header over damaged pages): there's nothing sound to copy,
/// and the caller quarantines it instead.
pub fn take_pre_migration(
    conn: &rusqlite::Connection,
    db_path: &Path,
    dir: &Path,
    from_version: i64,
) -> AppResult<Option<PathBuf>> {
    let needed = db_size(db_path);
    let failed = |e: CopyErr| AppError::UpgradeCopyFailed(classify(&e, needed, dir));
    std::fs::create_dir_all(dir).map_err(|e| failed(e.into()))?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3f");
    let name = format!("{PRE_MIGRATION}v{from_version}-{stamp}{SUFFIX}");
    let target = dir.join(&name);
    let tmp = dir.join(format!(".{name}.wfb-tmp"));
    let _ = std::fs::remove_file(&tmp);
    match vacuum_into(conn, &tmp) {
        Ok(true) => {}
        Ok(false) => {
            let _ = std::fs::remove_file(&tmp);
            return Ok(None);
        }
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            if let Err(e) = plain_copy(conn, db_path, &tmp) {
                let _ = std::fs::remove_file(&tmp);
                return Err(failed(e));
            }
        }
    }
    std::fs::rename(&tmp, &target).map_err(|e| failed(e.into()))?;
    let mut old: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with(PRE_MIGRATION) && n.ends_with(SUFFIX)
        })
        .map(|e| e.path())
        .collect();
    old.sort(); // the stamp sorts by time
    let excess = old.len().saturating_sub(KEEP_PRE_MIGRATION);
    for path in old.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
    Ok(Some(target))
}

/// `VACUUM INTO` a temp file, flushed to disk. `Ok(false)`: the source db
/// is corrupt (SQLite said so while reading it).
fn vacuum_into(conn: &rusqlite::Connection, tmp: &Path) -> Result<bool, CopyErr> {
    let tmp_str = tmp.to_str().ok_or_else(|| {
        CopyErr::Io(std::io::Error::other(format!(
            "db copy path isn't UTF-8: {}",
            tmp.display()
        )))
    })?;
    match conn.execute("VACUUM INTO ?1", [tmp_str]) {
        Ok(_) => {}
        Err(e) if is_corrupt(&e) => return Ok(false),
        Err(e) => return Err(CopyErr::Sqlite(e)),
    }
    flush(tmp)?;
    Ok(true)
}

/// The cheaper fallback: fold the WAL into the main file, then copy the file
/// as it is.
fn plain_copy(conn: &rusqlite::Connection, db_path: &Path, tmp: &Path) -> Result<(), CopyErr> {
    // Best effort: without it the copy misses what's still in the WAL, but
    // a failed checkpoint (busy) shouldn't stop the attempt.
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
    std::fs::copy(db_path, tmp)?;
    flush(tmp)?;
    Ok(())
}

/// Flushes a file to disk. Write access: on Windows, flushing
/// (FlushFileBuffers) a handle opened read-only fails with "Access is denied".
fn flush(path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)?
        .sync_all()
}

/// SQLite reported the database as corrupt or not a database.
pub fn is_corrupt(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)
    )
}

/// Puts the newest copy that opens cleanly in place at `db_path` (which the
/// caller has already moved aside). A copy that's itself damaged, or from a
/// newer build than this one (`max_version`), is skipped. Copies are copied,
/// never moved, so they stay available. Returns the date of the restored
/// copy, or `None` if there was no usable copy.
pub fn restore_newest(db_path: &Path, max_version: i64) -> Option<NaiveDate> {
    for (day, copy) in list(&dir_for(db_path)) {
        let tmp = db_path.with_extension("db.restoring");
        if std::fs::copy(&copy, &tmp).is_err() {
            continue;
        }
        if healthy(&tmp, max_version) && std::fs::rename(&tmp, db_path).is_ok() {
            return Some(day);
        }
        let _ = std::fs::remove_file(&tmp);
    }
    None
}

/// The file is a database SQLite can read end to end (`integrity_check`),
/// at a schema version this build can open.
fn healthy(path: &Path, max_version: i64) -> bool {
    let Ok(conn) = rusqlite::Connection::open(path) else {
        return false;
    };
    let intact = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .is_ok_and(|s| s == "ok");
    let version = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0));
    intact && version.is_ok_and(|v| v <= max_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn one_copy_a_day_keeping_the_newest_seven() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(&tmp.path().join("buddy.db")).unwrap();
        let dir = dir_for(&tmp.path().join("buddy.db"));

        // A temp file left by a crash on an earlier day is swept.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".buddy-2026-09-20.db.wfb-tmp"), b"partial").unwrap();

        let first = take_daily(&db, &dir, day("2026-10-01")).unwrap();
        assert!(first.is_some_and(|p| p.ends_with("buddy-2026-10-01.db")));
        assert!(
            take_daily(&db, &dir, day("2026-10-01")).unwrap().is_none(),
            "once a day"
        );

        for d in 2..=9 {
            take_daily(&db, &dir, day(&format!("2026-10-{d:02}"))).unwrap();
        }
        let kept: Vec<_> = list(&dir).into_iter().map(|(d, _)| d).collect();
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept[0], day("2026-10-09"));
        assert_eq!(kept[KEEP - 1], day("2026-10-03"), "the two oldest are gone");
        let stray = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().contains("wfb-tmp"));
        assert!(!stray, "no temp file left");
    }

    #[test]
    fn a_copy_holds_the_data() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        let db = Db::open(&path).unwrap();
        db.set_meta("probe", "in the copy").unwrap();
        let copy = take_daily(&db, &dir_for(&path), day("2026-10-04"))
            .unwrap()
            .unwrap();
        drop(db);

        let restored = Db::open(&copy).unwrap();
        assert_eq!(
            restored.get_meta("probe").unwrap().as_deref(),
            Some("in the copy")
        );
    }

    #[test]
    fn restore_skips_a_damaged_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        let dir = dir_for(&path);
        let db = Db::open(&path).unwrap();
        db.set_meta("probe", "from the 3rd").unwrap();
        take_daily(&db, &dir, day("2026-10-03")).unwrap();
        drop(db);
        // The newer copy is damaged.
        std::fs::write(dir.join("buddy-2026-10-04.db"), vec![0xAB; 8192]).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(restore_newest(&path, 99), Some(day("2026-10-03")));
        let db = Db::open(&path).unwrap();
        assert_eq!(
            db.get_meta("probe").unwrap().as_deref(),
            Some("from the 3rd")
        );
        assert_eq!(list(&dir).len(), 2, "copies are copied, not moved");
    }

    #[test]
    fn no_usable_copy_restores_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        assert_eq!(restore_newest(&path, 99), None);
        assert!(!path.exists());
    }

    /// #41 review: a copy from a newer build (higher schema version) is
    /// never restored; an older good copy is used instead.
    #[test]
    fn restore_skips_a_copy_from_a_newer_build() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        let dir = dir_for(&path);
        let db = Db::open(&path).unwrap();
        take_daily(&db, &dir, day("2026-10-03")).unwrap();
        take_daily(&db, &dir, day("2026-10-04")).unwrap();
        drop(db);
        let newer = dir.join("buddy-2026-10-04.db");
        rusqlite::Connection::open(&newer)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        std::fs::remove_file(&path).unwrap();

        let ours = rusqlite::Connection::open(dir.join("buddy-2026-10-03.db"))
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap();
        assert_eq!(restore_newest(&path, ours), Some(day("2026-10-03")));
    }

    /// V5b: copy failures become a plain reason for the startup screen.
    #[test]
    fn copy_failures_are_classified() {
        let dir = std::env::temp_dir();
        let io = |e: std::io::Error| classify(&CopyErr::Io(e), 40_000_000, &dir);

        match io(std::io::Error::from(std::io::ErrorKind::StorageFull)) {
            CopyFailure::DiskFull { needed_bytes, .. } => assert_eq!(needed_bytes, 40e6),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            io(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            CopyFailure::Locked
        );
        assert!(matches!(
            io(std::io::Error::other("something odd")),
            CopyFailure::Other { message } if message.contains("odd")
        ));
        #[cfg(windows)]
        {
            assert_eq!(
                io(std::io::Error::from_raw_os_error(32)),
                CopyFailure::Locked
            );
            assert!(matches!(
                io(std::io::Error::from_raw_os_error(112)),
                CopyFailure::DiskFull { .. }
            ));
        }
        // A real free-space reading for the temp dir's drive.
        match io(std::io::Error::from(std::io::ErrorKind::StorageFull)) {
            CopyFailure::DiskFull { free_bytes, .. } => assert!(free_bytes.is_some()),
            other => panic!("{other:?}"),
        }
    }

    /// V5b: the plain-copy fallback checkpoints first, so rows still in the
    /// WAL make it into the copy.
    #[test]
    fn the_plain_copy_includes_what_is_in_the_wal() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("buddy.db");
        let db = Db::open(&path).unwrap();
        db.set_meta("probe", "only in the wal").unwrap();
        let copy = tmp.path().join("copy.db");
        db.with_conn(|c| plain_copy(c, &path, &copy).map_err(AppError::from))
            .unwrap();
        drop(db);
        let value: String = rusqlite::Connection::open(&copy)
            .unwrap()
            .query_row("SELECT value FROM meta WHERE key = 'probe'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(value, "only in the wal");
    }
}
