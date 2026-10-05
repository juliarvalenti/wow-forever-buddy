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

/// Where the copies of the db at `db_path` live: a sibling folder.
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
pub fn take_daily(db: &Db, dir: &Path, today: NaiveDate) -> AppResult<Option<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let target = dir.join(file_name(today));
    if target.exists() {
        return Ok(None);
    }
    let tmp = dir.join(format!(".{}.wfb-tmp", file_name(today)));
    let _ = std::fs::remove_file(&tmp);
    let tmp_str = tmp
        .to_str()
        .ok_or_else(|| AppError::Io(format!("db copy path isn't UTF-8: {}", tmp.display())))?
        .to_string();
    db.with_conn(|c| {
        c.execute("VACUUM INTO ?1", [&tmp_str])?;
        Ok(())
    })?;
    std::fs::File::open(&tmp)?.sync_all()?;
    std::fs::rename(&tmp, &target)?;
    for (_, old) in list(dir).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(target))
}

/// Puts the newest copy that opens cleanly in place at `db_path` (which the
/// caller has already moved aside). A copy that's itself damaged is skipped.
/// Copies are copied, never moved, so they stay available. Returns the date
/// of the restored copy, or `None` if there was no usable copy.
pub fn restore_newest(db_path: &Path) -> Option<NaiveDate> {
    for (day, copy) in list(&dir_for(db_path)) {
        let tmp = db_path.with_extension("db.restoring");
        if std::fs::copy(&copy, &tmp).is_err() {
            continue;
        }
        if healthy(&tmp) && std::fs::rename(&tmp, db_path).is_ok() {
            return Some(day);
        }
        let _ = std::fs::remove_file(&tmp);
    }
    None
}

/// The file is a database SQLite can read end to end.
fn healthy(path: &Path) -> bool {
    let Ok(conn) = rusqlite::Connection::open(path) else {
        return false;
    };
    conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
        .is_ok_and(|s| s == "ok")
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

        assert_eq!(restore_newest(&path), Some(day("2026-10-03")));
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
        assert_eq!(restore_newest(&path), None);
        assert!(!path.exists());
    }
}
