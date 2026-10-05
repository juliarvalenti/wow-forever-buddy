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
    db.with_conn(|c| vacuum_into(c, &tmp))?;
    std::fs::rename(&tmp, &target)?;
    for (_, old) in list(dir).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(target))
}

/// Before migrations run on an existing db (`from_version` > 0 and below
/// the latest), a copy of it as it is: `pre-migration-v<from>-<stamp>.db`,
/// keeping the newest 3. Not a daily copy, so restore never picks it on its
/// own; it's there to recover by hand if a migration ever goes wrong.
pub fn take_pre_migration(
    conn: &rusqlite::Connection,
    dir: &Path,
    from_version: i64,
) -> AppResult<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3f");
    let name = format!("{PRE_MIGRATION}v{from_version}-{stamp}{SUFFIX}");
    let target = dir.join(&name);
    let tmp = dir.join(format!(".{name}.wfb-tmp"));
    let _ = std::fs::remove_file(&tmp);
    vacuum_into(conn, &tmp)?;
    std::fs::rename(&tmp, &target)?;
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
    Ok(target)
}

/// `VACUUM INTO` a temp file, flushed to disk.
fn vacuum_into(conn: &rusqlite::Connection, tmp: &Path) -> AppResult<()> {
    let tmp_str = tmp
        .to_str()
        .ok_or_else(|| AppError::Io(format!("db copy path isn't UTF-8: {}", tmp.display())))?;
    conn.execute("VACUUM INTO ?1", [tmp_str])?;
    std::fs::File::open(tmp)?.sync_all()?;
    Ok(())
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
}
