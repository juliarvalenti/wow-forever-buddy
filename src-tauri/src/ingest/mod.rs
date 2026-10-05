//! Ingest (docs/specs/v0.2-addon.md §4): reading each character's
//! `ForeverBuddy.lua` into the db.
//!
//! When: a scan at app start (after replaying backups, below), after WoW
//! exits (once its writes settle), and every 10 minutes while it runs (a
//! `/reload` writes the file mid-session). A scan reads only files whose
//! size or mtime changed since `ingest_state` says it last looked, and only
//! once they've been still for 2 s. The file watcher the spec mentions is a
//! follow-up: these scans already cover every write, just less instantly.
//!
//! Gap-fill: the file keeps only the last 10 sessions, but every game-exit
//! backup holds that logout's copy. At start, every full backup snapshot
//! newer than `meta.last_replayed_snapshot` is replayed, oldest first,
//! through the same idempotent apply. That's also how a db restored from a
//! daily copy (V5) catches up: the copy carries its own marker.
//!
//! Ingest only ever reads the game folder.

pub mod apply;
pub mod file;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::AppResult;
use crate::fsx::read::safe_read;
use crate::state::AppCore;
use apply::Target;
use file::Status;

const FILE_NAME: &str = "ForeverBuddy.lua";
/// A file must be this still before it's read (core-fs §3).
const SETTLE: Duration = Duration::from_secs(2);
/// Meta key: the newest backup snapshot gap-fill has replayed.
pub const LAST_REPLAYED: &str = "last_replayed_snapshot";

/// Emitted after a scan or replay that changed characters' data.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct IngestCompleted {
    /// `characters.id` (u32: row ids, and specta refuses i64 in TypeScript).
    pub characters: Vec<u32>,
}

/// A character's file that couldn't be read last time, for the Dashboard's
/// "Couldn't read Thrandor's notes · will retry".
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct IngestProblem {
    /// The character folder, e.g. `Ellygie-Vargur`.
    pub character: String,
    /// `skipped (parse)`, `skipped (integrity)` or `skipped (mismatch)`.
    pub status: String,
    /// Names the field at fault, never a value from the file.
    pub error: Option<String>,
}

/// Where a `ForeverBuddy.lua` sits, relative to the flavor folder:
/// `WTF/Account/<account>/<group>/<character>/SavedVariables/ForeverBuddy.lua`,
/// where the group is Forever's opaque id or, on the older layout, the realm.
pub fn target_for(flavor: &str, rel: &str) -> Option<Target> {
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        [wtf, acct, account, group, character, sv, name]
            if wtf.eq_ignore_ascii_case("WTF")
                && acct.eq_ignore_ascii_case("Account")
                && sv.eq_ignore_ascii_case("SavedVariables")
                && name.eq_ignore_ascii_case(FILE_NAME) =>
        {
            Some(Target {
                flavor: flavor.to_string(),
                account: account.to_string(),
                group_dir: group.to_string(),
                char_dir: character.to_string(),
            })
        }
        _ => None,
    }
}

/// What reading one file came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Applied(i64),
    Skipped(file::Rejected),
}

/// Decodes, checks the folder, and applies one file's bytes in one
/// transaction.
pub fn ingest_bytes(db: &Db, target: &Target, bytes: &[u8]) -> AppResult<Outcome> {
    let addon = match file::decode(bytes) {
        Ok(f) => f,
        Err(rejected) => return Ok(Outcome::Skipped(rejected)),
    };
    if !file::matches_folder(&addon.character, &target.char_dir) {
        return Ok(Outcome::Skipped(file::Rejected {
            status: Status::Mismatch,
            error: "character.name doesn't match the character folder".into(),
        }));
    }
    let id = db.with_conn(|c| {
        let tx = c.transaction()?;
        let id = apply::apply(&tx, target, &addon)?;
        tx.commit()?;
        Ok(id)
    })?;
    Ok(Outcome::Applied(id))
}

/// Every `ForeverBuddy.lua` under the flavor folder, as `(relative, absolute)`.
fn addon_files(flavor_dir: &Path) -> Vec<(String, PathBuf)> {
    let accounts = flavor_dir.join("WTF").join("Account");
    let mut out = Vec::new();
    let dirs = |p: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect()
    };
    for account in dirs(&accounts) {
        for group in dirs(&account) {
            for character in dirs(&group) {
                let file = character.join("SavedVariables").join(FILE_NAME);
                if !file.is_file() {
                    continue;
                }
                let rel = file
                    .strip_prefix(flavor_dir)
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                out.push((rel, file));
            }
        }
    }
    out.sort();
    out
}

fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64)
}

/// Reads every changed, settled `ForeverBuddy.lua` in the flavor folder.
/// Returns the characters whose data was applied.
pub fn scan(db: &Db, flavor: &str, flavor_dir: &Path) -> AppResult<Vec<i64>> {
    let mut applied = Vec::new();
    for (rel, abs) in addon_files(flavor_dir) {
        let Some(target) = target_for(flavor, &rel) else {
            continue;
        };
        let Ok(meta) = std::fs::metadata(&abs) else {
            continue;
        };
        let (size, mtime) = (meta.len() as i64, mtime_ns(&meta));
        // Still being written (WoW saves on logout and /reload): next scan.
        let settled = meta
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .is_some_and(|age| age >= SETTLE);
        if !settled {
            continue;
        }
        let seen: Option<(i64, i64)> = db.with_conn(|c| {
            Ok(c.query_row(
                "SELECT size, mtime_ns FROM ingest_state WHERE path = ?1",
                [&rel],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
        })?;
        // Unchanged since last time, whether it worked or not: a failed file
        // is retried when it changes (spec §4 step 1).
        if seen == Some((size, mtime)) {
            continue;
        }
        // Locked or changing right now: leave it for the next scan.
        let Ok(bytes) = safe_read(&abs) else {
            continue;
        };
        let (status, error) = match ingest_bytes(db, &target, &bytes)? {
            Outcome::Applied(id) => {
                applied.push(id);
                (Status::Ok, None)
            }
            Outcome::Skipped(r) => (r.status, Some(r.error)),
        };
        db.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO ingest_state (path, size, mtime_ns, ingested_at, status, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![rel, size, mtime, chrono::Utc::now().to_rfc3339(), status.as_str(), error],
            )?;
            Ok(())
        })?;
    }
    Ok(applied)
}

/// Gap-fill: replays every full backup snapshot of `flavor` newer than
/// `meta.last_replayed_snapshot`, oldest first, through the same apply.
/// Holds the job lock, so a prune can't remove a snapshot mid-read. A
/// missing or damaged copy of one file just skips that file.
pub fn replay_backups(core: &AppCore, flavor: &str) -> AppResult<Vec<i64>> {
    let _job = core.jobs.lock().expect("job lock poisoned");
    let store = core.backups()?;
    let last = core.db.get_meta(LAST_REPLAYED)?.unwrap_or_default();
    // Snapshot ids are ULIDs, which sort by time.
    let mut pending: Vec<String> = store
        .list()?
        .into_iter()
        .filter(|s| s.scope == crate::backup::manifest::Scope::Full && s.flavor == flavor)
        .map(|s| s.id)
        .filter(|id| *id > last)
        .collect();
    pending.sort();
    let mut applied = Vec::new();
    for id in pending {
        let manifest = store.manifest(&id)?;
        for f in &manifest.files {
            let Some(target) = target_for(flavor, &f.path) else {
                continue;
            };
            let Ok(bytes) = store.blobs().get(&f.blake3) else {
                continue;
            };
            if let Outcome::Applied(char_id) = ingest_bytes(&core.db, &target, &bytes)? {
                if !applied.contains(&char_id) {
                    applied.push(char_id);
                }
            }
        }
        core.db.set_meta(LAST_REPLAYED, &id)?;
    }
    Ok(applied)
}

/// Files that couldn't be read last time, for the Dashboard.
pub fn problems(db: &Db) -> AppResult<Vec<IngestProblem>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT path, status, error FROM ingest_state WHERE status != 'ok' ORDER BY path",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let path: String = r.get(0)?;
                Ok(IngestProblem {
                    character: path.split('/').nth(4).unwrap_or_default().to_string(),
                    status: r.get(1)?,
                    error: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// What the ingest worker is asked to do.
pub enum Job {
    /// App start: replay backups, then scan.
    Start,
    /// A scan now (the 10-minute tick while WoW runs).
    Scan,
    /// WoW just exited: wait for its writes to settle, then scan.
    AfterExit,
}

/// Runs one job against the active game. `settle` waits for WoW's exit
/// writes. Returns the characters whose data changed; failures are logged
/// (a file that can't be read is recorded in `ingest_state` instead).
pub fn run_job(core: &AppCore, job: &Job, settle: impl Fn(&Path)) -> Vec<i64> {
    let Ok(game) = core.active_game() else {
        return Vec::new();
    };
    let note = |what: &str, e: crate::error::AppError| {
        crate::applog::append(&core.paths.log_dir, &format!("ingest {what} failed: {e}"));
    };
    let mut changed = Vec::new();
    if matches!(job, Job::Start) {
        match replay_backups(core, &game.flavor) {
            Ok(ids) => changed.extend(ids),
            Err(e) => note("replay", e),
        }
    }
    if matches!(job, Job::AfterExit) {
        settle(&game.root.base.join("WTF"));
    }
    match scan(&core.db, &game.flavor, &game.root.base) {
        Ok(ids) => changed.extend(ids),
        Err(e) => note("scan", e),
    }
    changed.sort_unstable();
    changed.dedup();
    changed
}

/// One thread runs ingest jobs in order, like the T14 session worker.
pub fn spawn_worker(run: impl Fn(Job) + Send + 'static) -> Sender<Job> {
    let (tx, rx) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name("ingest".into())
        .spawn(move || {
            for job in rx {
                run(job);
            }
        })
        .expect("spawn ingest thread");
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/addon")
                .join(name),
        )
        .unwrap()
    }

    fn target(char_dir: &str) -> Target {
        Target {
            flavor: "_classic_beta_".into(),
            account: "ACCOUNT1".into(),
            group_dir: "70".into(),
            char_dir: char_dir.into(),
        }
    }

    fn count(db: &Db, sql: &str) -> i64 {
        db.with_conn(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    }

    #[test]
    fn paths_for_both_layouts() {
        let t = target_for(
            "_classic_beta_",
            "WTF/Account/ACCOUNT1/70/Ellygie-Vargur/SavedVariables/ForeverBuddy.lua",
        )
        .unwrap();
        assert_eq!(
            (t.group_dir.as_str(), t.char_dir.as_str()),
            ("70", "Ellygie-Vargur")
        );
        let legacy = target_for(
            "_classic_era_",
            "wtf/account/A/Classic Beta PvP 2/Ellygie/savedvariables/foreverbuddy.lua",
        )
        .unwrap();
        assert_eq!(legacy.group_dir, "Classic Beta PvP 2");
        assert!(target_for("x", "WTF/Account/A/SavedVariables/ForeverBuddy.lua").is_none());
        assert!(target_for("x", "WTF/Account/A/70/C/SavedVariables/Other.lua").is_none());
    }

    #[test]
    fn the_adventure_fixture_lands_in_the_db() {
        let db = Db::open_in_memory().unwrap();
        let out = ingest_bytes(&db, &target("Thrandor-Vargur"), &fixture("adventure.lua")).unwrap();
        assert!(matches!(out, Outcome::Applied(_)));
        assert_eq!(count(&db, "SELECT count(*) FROM characters"), 1);
        let (name, surname, realm): (String, String, String) = db
            .with_conn(|c| {
                Ok(
                    c.query_row("SELECT name, surname, realm FROM characters", [], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })?,
                )
            })
            .unwrap();
        assert_eq!((name.as_str(), surname.as_str()), ("Thrandor", "Vargur"));
        assert_eq!(realm, "Classic Beta PvP 2");
        assert_eq!(count(&db, "SELECT count(*) FROM adventures"), 1);
        assert_eq!(count(&db, "SELECT count(*) FROM adventure_events"), 14);
        assert_eq!(count(&db, "SELECT end_money FROM adventures"), 25545);
        assert_eq!(count(&db, "SELECT end_level FROM adventures"), 13);
        assert_eq!(
            count(&db, "SELECT count(*) FROM gold_points"),
            4,
            "three money events and the snapshot"
        );
        // V2's snapshot sections.
        assert_eq!(count(&db, "SELECT money FROM char_snapshots"), 25545);
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM char_items WHERE location = 'bag'"
            ),
            2
        );
        assert_eq!(
            count(
                &db,
                "SELECT item_id FROM char_items WHERE location = 'equipped' AND slot = 16"
            ),
            25
        );
        assert_eq!(count(&db, "SELECT count(*) FROM professions"), 3);
    }

    #[test]
    fn applying_twice_changes_nothing() {
        let db = Db::open_in_memory().unwrap();
        let t = target("Thrandor-Vargur");
        let bytes = fixture("adventure.lua");
        ingest_bytes(&db, &t, &bytes).unwrap();
        let before: Vec<i64> = [
            "SELECT count(*) FROM characters",
            "SELECT count(*) FROM adventures",
            "SELECT count(*) FROM adventure_events",
            "SELECT count(*) FROM gold_points",
        ]
        .iter()
        .map(|q| count(&db, q))
        .collect();
        ingest_bytes(&db, &t, &bytes).unwrap();
        let after: Vec<i64> = [
            "SELECT count(*) FROM characters",
            "SELECT count(*) FROM adventures",
            "SELECT count(*) FROM adventure_events",
            "SELECT count(*) FROM gold_points",
        ]
        .iter()
        .map(|q| count(&db, q))
        .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn the_folder_wins() {
        let db = Db::open_in_memory().unwrap();
        let out = ingest_bytes(&db, &target("Someone-Else"), &fixture("adventure.lua")).unwrap();
        match out {
            Outcome::Skipped(r) => assert_eq!(r.status, Status::Mismatch),
            other => panic!("{other:?}"),
        }
        assert_eq!(count(&db, "SELECT count(*) FROM characters"), 0);
    }

    /// An older copy (a backup replay) after a newer one: newer gear and the
    /// longer session survive, and the user's note is kept.
    #[test]
    fn older_data_never_overwrites_newer() {
        let db = Db::open_in_memory().unwrap();
        let t = target("Ellygie-Vargur");
        let file = |at: i64, helm: i64, events: usize| {
            let evs: String = (0..events)
                .map(|i| format!("{{ kind = \"zone\", t = {}, zone = \"Z{i}\" }},", 100 + i))
                .collect();
            format!(
                r#"ForeverBuddyDB = {{
  _meta = {{ schema = 1, written = {at}, counts = {{ sessions = 1, events = {events}, items = 0, bag_items = 0 }} }},
  character = {{ name = "Ellygie", surname = "Vargur", level = {lvl} }},
  snapshot = {{ at = {at}, money = {at}, level = {lvl}, equipped = {{ [1] = "|Hitem:{helm}:|h[x]|h" }} }},
  sessions = {{ {{ id = 100, login = 100, events = {{ {evs} }} }} }},
}}
"#,
                lvl = at / 100
            )
        };
        ingest_bytes(&db, &t, file(2000, 2222, 5).as_bytes()).unwrap();
        db.with_conn(|c| {
            c.execute("UPDATE adventures SET note = 'my note'", [])?;
            Ok(())
        })
        .unwrap();
        ingest_bytes(&db, &t, file(1000, 1111, 2).as_bytes()).unwrap();

        assert_eq!(
            count(
                &db,
                "SELECT item_id FROM char_items WHERE location = 'equipped'"
            ),
            2222
        );
        assert_eq!(
            count(&db, "SELECT level FROM characters"),
            20,
            "display stays newest"
        );
        assert_eq!(count(&db, "SELECT count(*) FROM adventure_events"), 5);
        assert_eq!(
            count(&db, "SELECT count(*) FROM char_snapshots"),
            2,
            "both kept as history"
        );
        let note: String = db
            .with_conn(|c| Ok(c.query_row("SELECT note FROM adventures", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(note, "my note");
    }

    fn write(dir: &Path, rel: &str, bytes: &[u8]) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        // Old enough to count as settled.
        let old = SystemTime::now() - Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }

    #[test]
    fn a_scan_reads_changed_files_once_and_records_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let flavor = tmp.path();
        let db = Db::open_in_memory().unwrap();
        let good = "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua";
        let bad = "WTF/Account/ACCOUNT1/70/Ellygie/SavedVariables/ForeverBuddy.lua";
        write(flavor, good, &fixture("adventure.lua"));
        write(flavor, bad, b"ForeverBuddyDB = { sender = \"SecretSender\"");

        let ids = scan(&db, "_classic_beta_", flavor).unwrap();
        assert_eq!(ids.len(), 1);
        let problems = problems(&db).unwrap();
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].character, "Ellygie");
        assert_eq!(problems[0].status, "skipped (parse)");
        assert!(!problems[0]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("SecretSender"));

        assert!(
            scan(&db, "_classic_beta_", flavor).unwrap().is_empty(),
            "unchanged: not re-read"
        );
    }

    /// Gap-fill: a full backup's copy of the file is replayed once, then the
    /// marker stops it being replayed again.
    #[test]
    fn backups_are_replayed_once() {
        use crate::backup::manifest::Trigger;
        use crate::backup::{SnapshotRequest, SnapshotScope};
        use crate::config::paths::AppPaths;
        use crate::game::process::fake::FakeProbe;
        use crate::secrets::MemoryStore;
        use std::sync::Arc;

        let (dir, root) = crate::test_support::fixture_copy();
        let core = AppCore::with_parts(
            AppPaths::under(&dir.path().join("app")),
            Arc::new(MemoryStore::default()),
            Arc::new(FakeProbe::default()),
        )
        .unwrap();
        crate::install::set(&core.settings, &root, Some("_classic_beta_")).unwrap();
        let flavor_dir = root.join("_classic_beta_");
        write(
            &flavor_dir,
            "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
            &fixture("adventure.lua"),
        );
        let game = core.active_game().unwrap();
        core.backups()
            .unwrap()
            .create(
                SnapshotRequest {
                    game: &game.root,
                    flavor: &game.flavor,
                    trigger: Trigger::Manual,
                    label: None,
                    scope: SnapshotScope::Full {
                        include_addons: false,
                    },
                    game_running: false,
                },
                &mut |_, _| {},
            )
            .unwrap();

        let ids = replay_backups(&core, "_classic_beta_").unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(count(&core.db, "SELECT count(*) FROM adventures"), 1);
        assert!(core.db.get_meta(LAST_REPLAYED).unwrap().is_some());
        assert!(
            replay_backups(&core, "_classic_beta_").unwrap().is_empty(),
            "marker: once"
        );
        // Another flavor's backups aren't replayed into this one.
        core.db.set_meta(LAST_REPLAYED, "").unwrap();
        assert!(replay_backups(&core, "_retail_").unwrap().is_empty());
    }

    #[test]
    fn a_file_still_being_written_waits_for_the_next_scan() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        let rel = "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua";
        let p = tmp.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, fixture("adventure.lua")).unwrap(); // mtime: now
        assert!(scan(&db, "_classic_beta_", tmp.path()).unwrap().is_empty());
        assert_eq!(count(&db, "SELECT count(*) FROM ingest_state"), 0);
    }
}
