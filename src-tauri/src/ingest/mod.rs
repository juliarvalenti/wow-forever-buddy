//! Ingest (docs/specs/v0.2-addon.md §4): reading each character's
//! `ForeverBuddy.lua` into the db.
//!
//! When: a scan at app start (after replaying backups, below), after WoW
//! exits (once its writes settle), and a few seconds after each logout or
//! `/reload` while it runs ([`Watcher`]). A scan reads only files whose size
//! or mtime changed since `ingest_state` says it last looked, and only once
//! they've been still for 2 s.
//!
//! Gap-fill: the file keeps only the last 10 sessions, but every game-exit
//! backup holds that logout's copy. At start, every full backup snapshot
//! newer than that flavor's `meta.last_replayed_snapshot:<flavor>` is
//! replayed, oldest first, through the same idempotent apply. That's also
//! how a db restored from a daily copy (V5) catches up: the copy carries its
//! own marker.
//!
//! Older settings folders (W1b) are never read as characters, live or
//! replayed.
//!
//! Ingest only ever reads the game folder.

pub mod apply;
pub mod file;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::AppResult;
use crate::fsx::read::safe_read;
use crate::install::wtf::older_groups;
use crate::state::AppCore;
use apply::Target;
use file::Status;

const FILE_NAME: &str = "ForeverBuddy.lua";
/// A file must be this still before it's read (core-fs §3).
pub const SETTLE: Duration = Duration::from_secs(2);
/// Meta key: the newest backup snapshot gap-fill has replayed, per flavor
/// (`last_replayed_snapshot:<flavor>`): replay is per flavor, so one marker
/// shared across flavors would skip the other flavor's older backups.
pub fn last_replayed_key(flavor: &str) -> String {
    format!("last_replayed_snapshot:{flavor}")
}

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
/// Older settings folders (W1b, `install::wtf::older_groups`) are left out:
/// they're never a character.
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
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    for account in dirs(&accounts) {
        let groups = dirs(&account);
        let names: Vec<String> = groups.iter().filter_map(|g| name(g)).collect();
        let older = older_groups(names.iter().map(String::as_str));
        for group in groups {
            if name(&group).is_some_and(|g| older.contains(g.as_str())) {
                continue;
            }
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

/// `ingest_state.path`: the file relative to the game root
/// (`<flavor>/WTF/...`), so two flavors' copies of one character folder
/// keep separate rows.
fn state_key(flavor: &str, rel: &str) -> String {
    format!("{flavor}/{rel}")
}

/// Reads every changed, settled `ForeverBuddy.lua` in the flavor folder.
/// Returns the characters whose data was applied.
pub fn scan(db: &Db, flavor: &str, flavor_dir: &Path) -> AppResult<Vec<i64>> {
    let mut applied = Vec::new();
    for (rel, abs) in addon_files(flavor_dir) {
        let Some(target) = target_for(flavor, &rel) else {
            continue;
        };
        // Forgotten (O2): not read again until Remember again. Its
        // ingest_state row went with it, so that reads it fresh.
        if crate::tidy::is_forgotten(db, &target)? {
            continue;
        }
        let key = state_key(flavor, &rel);
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
                [&key],
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
                params![key, size, mtime, chrono::Utc::now().to_rfc3339(), status.as_str(), error],
            )?;
            Ok(())
        })?;
    }
    Ok(applied)
}

/// The older settings folders (W1b) among snapshot paths
/// (`WTF/Account/<account>/<group>/...`), as `(account, group)`: the same
/// rule a live scan applies to the folders on disk.
fn older_in<'a>(paths: impl Iterator<Item = &'a str>) -> HashSet<(String, String)> {
    let mut groups: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for p in paths {
        if let [_, _, account, group, _, ..] = p.split('/').collect::<Vec<_>>()[..] {
            groups.entry(account).or_default().insert(group);
        }
    }
    groups
        .into_iter()
        .flat_map(|(account, gs)| {
            older_groups(gs)
                .into_iter()
                .map(move |g| (account.to_string(), g.to_string()))
        })
        .collect()
}

/// Gap-fill: replays every full backup snapshot of `flavor` newer than that
/// flavor's marker, oldest first, through the same apply. Holds the job
/// lock, so a prune can't remove a snapshot mid-read. A missing or damaged
/// copy of one file skips that file; a manifest that can't be read skips
/// that snapshot (logged), so it can't stall every later replay.
pub fn replay_backups(core: &AppCore, flavor: &str) -> AppResult<Vec<i64>> {
    let _job = core.jobs.lock().expect("job lock poisoned");
    let store = core.backups()?;
    let key = last_replayed_key(flavor);
    let last = core.db.get_meta(&key)?.unwrap_or_default();
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
        let manifest = match store.manifest(&id) {
            Ok(m) => m,
            Err(e) => {
                crate::applog::append(
                    &core.paths.log_dir,
                    &format!("ingest replay skipped snapshot {id}: {e}"),
                );
                core.db.set_meta(&key, &id)?;
                continue;
            }
        };
        let older = older_in(manifest.files.iter().map(|f| f.path.as_str()));
        for f in &manifest.files {
            let Some(target) = target_for(flavor, &f.path) else {
                continue;
            };
            if older.contains(&(target.account.clone(), target.group_dir.clone())) {
                continue;
            }
            // Forgotten (O2): an old backup doesn't bring it back.
            if crate::tidy::is_forgotten(&core.db, &target)? {
                continue;
            }
            let Ok(bytes) = store.blobs().get(&f.blake3) else {
                continue;
            };
            if let Outcome::Applied(char_id) = ingest_bytes(&core.db, &target, &bytes)? {
                if !applied.contains(&char_id) {
                    applied.push(char_id);
                }
            }
        }
        core.db.set_meta(&key, &id)?;
    }
    Ok(applied)
}

/// `flavor`'s files that couldn't be read last time, for the Dashboard.
pub fn problems(db: &Db, flavor: &str) -> AppResult<Vec<IngestProblem>> {
    let prefix = state_key(flavor, "");
    db.with_conn(|c| {
        // substr, not LIKE: flavor folders have '_', LIKE's wildcard.
        let mut stmt = c.prepare(
            "SELECT path, status, error FROM ingest_state
             WHERE status != 'ok' AND substr(path, 1, length(?1)) = ?1 ORDER BY path",
        )?;
        let rows = stmt
            .query_map([&prefix], |r| {
                let path: String = r.get(0)?;
                Ok(IngestProblem {
                    // <flavor>/WTF/Account/<account>/<group>/<character>/...
                    character: path.split('/').nth(5).unwrap_or_default().to_string(),
                    status: r.get(1)?,
                    error: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// How often the watcher looks while WoW runs.
pub const WATCH_EVERY: Duration = Duration::from_secs(2);

/// Notices a logout or `/reload` a few seconds after it lands, by polling
/// the addon files' size and mtime while WoW runs. That's a stat of one file
/// per character, so a poll costs next to nothing; polling also sees writes
/// through a WTF folder linked elsewhere (a Dropbox junction), where native
/// change notifications are unreliable, and can't miss one to a dropped
/// event. It asks for a scan once the files changed since the last scan it
/// asked for and have been still for `SETTLE`, so a save in progress waits
/// instead of being read half-written and then never retried.
#[derive(Default)]
pub struct Watcher {
    /// What the files looked like at the last scan this asked for.
    last: Option<Vec<(String, u64, i64)>>,
}

impl Watcher {
    /// True when a scan is due.
    pub fn poll(&mut self, flavor_dir: &Path) -> bool {
        let mut newest: Option<SystemTime> = None;
        // Auctionator's account-wide prices too (F5): a logout writes both.
        let auctionator = crate::ah::files(flavor_dir)
            .into_iter()
            .map(|(_, rel, abs)| (rel, abs));
        let now: Vec<(String, u64, i64)> = addon_files(flavor_dir)
            .into_iter()
            .chain(auctionator)
            .filter_map(|(rel, abs)| {
                let meta = std::fs::metadata(&abs).ok()?;
                newest = newest.max(meta.modified().ok());
                Some((rel, meta.len(), mtime_ns(&meta)))
            })
            .collect();
        if self.last.as_ref() == Some(&now) {
            return false;
        }
        // The same test as `scan`'s (a future mtime isn't settled), so a
        // file the scan would skip is never recorded here as handled.
        let settled = newest.is_none_or(|m| {
            SystemTime::now()
                .duration_since(m)
                .is_ok_and(|age| age >= SETTLE)
        });
        if settled {
            self.last = Some(now);
        }
        settled
    }
}

/// What the ingest worker is asked to do.
pub enum Job {
    /// App start: replay backups, then scan.
    Start,
    /// A scan now (the watcher saw a file change while WoW runs).
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

    /// Bridge receipts land per character and slot; a file without them
    /// (an older one, replayed) leaves them as they were.
    #[test]
    fn bridge_receipts_are_kept() {
        let db = Db::open_in_memory().unwrap();
        let t = target("Thrandor-Vargur");
        ingest_bytes(&db, &t, &fixture("bridge.lua")).unwrap();
        let q = "SELECT count(*) FROM bridge_receipts WHERE stamp = 1790960000";
        assert_eq!(count(&db, q), 2);
        ingest_bytes(&db, &t, &fixture("first_login.lua")).unwrap();
        assert_eq!(count(&db, q), 2);
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

    /// Every section of the real addon's snapshot lands in its table, read
    /// from the harness-generated fixture (tools/addon-test), so a change in
    /// what the addon writes breaks this test.
    #[test]
    fn the_snapshot_fixture_lands_in_every_table() {
        let db = Db::open_in_memory().unwrap();
        ingest_bytes(&db, &target("Thrandor-Vargur"), &fixture("snapshot.lua")).unwrap();
        let rows = |location: &str| {
            count(
                &db,
                &format!("SELECT count(*) FROM char_items WHERE location = '{location}'"),
            )
        };
        assert_eq!(
            (rows("equipped"), rows("bag"), rows("bank"), rows("mail")),
            (1, 2, 1, 1)
        );
        assert_eq!(
            count(
                &db,
                "SELECT item_id FROM char_items WHERE location = 'bank'"
            ),
            14047
        );
        assert_eq!(count(&db, "SELECT money FROM char_mail"), 500);
        assert_eq!(
            count(
                &db,
                "SELECT map FROM char_snapshots WHERE rest_state = 'Rested'"
            ),
            1429
        );
        assert_eq!(count(&db, "SELECT level FROM char_snapshots"), 12);
        assert_eq!(
            count(&db, "SELECT line FROM professions WHERE name = 'Herbalism'"),
            182
        );
        assert_eq!(
            count(
                &db,
                "SELECT raid FROM lockouts WHERE name = 'The Deadmines'"
            ),
            0
        );
        assert_eq!(
            count(&db, "SELECT raid FROM lockouts WHERE name = 'Molten Core'"),
            1
        );
        assert_eq!(
            count(&db, "SELECT spec FROM professions WHERE name = 'Tailoring'"),
            2
        );
        assert_eq!(count(&db, "SELECT count(*) FROM items"), 4);
    }

    /// A later file with no bank or mail (no visit since) keeps the stored
    /// bank and mail rather than emptying them; a carried-forward copy of
    /// the same visit changes nothing. All three files are the addon's own.
    #[test]
    fn an_absent_bank_keeps_the_stored_bank() {
        let db = Db::open_in_memory().unwrap();
        let t = target("Thrandor-Vargur");
        let kept = |db: &Db| {
            (
                count(
                    db,
                    "SELECT count(*) FROM char_items WHERE location = 'bank'",
                ),
                count(
                    db,
                    "SELECT count(*) FROM char_items WHERE location = 'mail'",
                ),
                count(db, "SELECT count(*) FROM char_mail"),
            )
        };
        ingest_bytes(&db, &t, &fixture("snapshot.lua")).unwrap();
        assert_eq!(kept(&db), (1, 1, 1));
        ingest_bytes(&db, &t, &fixture("second_login.lua")).unwrap();
        assert_eq!(
            kept(&db),
            (1, 1, 1),
            "no bank in the file: stored bank kept"
        );
        ingest_bytes(&db, &t, &fixture("carry_forward.lua")).unwrap();
        assert_eq!(kept(&db), (1, 1, 1), "carried-forward visit");
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
        assert!(
            problems(&db, "_classic_").unwrap().is_empty(),
            "another flavor's"
        );
        let problems = problems(&db, "_classic_beta_").unwrap();
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

    /// Two flavors with the same account/group/character folder keep
    /// separate `ingest_state` rows: reading one doesn't mark the other read.
    #[test]
    fn each_flavor_tracks_its_own_files() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        let rel = "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua";
        for flavor in ["_classic_", "_classic_beta_"] {
            write(&tmp.path().join(flavor), rel, &fixture("adventure.lua"));
        }
        // Same bytes, same mtime: a shared row would skip the second flavor.
        for flavor in ["_classic_", "_classic_beta_"] {
            assert_eq!(
                scan(&db, flavor, &tmp.path().join(flavor)).unwrap().len(),
                1,
                "{flavor}"
            );
        }
        assert_eq!(count(&db, "SELECT count(*) FROM ingest_state"), 2);
    }

    /// Older settings folders (W1b) are never read as characters: the realm
    /// folder next to a group id is skipped, while an account with only
    /// realm folders keeps them.
    #[test]
    fn older_settings_folders_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let file = "SavedVariables/ForeverBuddy.lua";
        write(
            tmp.path(),
            &format!("WTF/Account/A/70/Thrandor-Vargur/{file}"),
            b"x\n",
        );
        write(
            tmp.path(),
            &format!("WTF/Account/A/Classic Beta PvP 2/Thrandor/{file}"),
            b"x\n",
        );
        write(
            tmp.path(),
            &format!("WTF/Account/B/Whitemane/Brannic/{file}"),
            b"x\n",
        );
        let found: Vec<String> = addon_files(tmp.path())
            .into_iter()
            .map(|(rel, _)| rel)
            .collect();
        assert_eq!(
            found,
            [
                format!("WTF/Account/A/70/Thrandor-Vargur/{file}"),
                format!("WTF/Account/B/Whitemane/Brannic/{file}"),
            ]
        );
        // The same rule over a snapshot's paths, for replay.
        let older = older_in(
            [
                "WTF/Account/A/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
                "WTF/Account/A/Classic Beta PvP 2/Thrandor/SavedVariables/ForeverBuddy.lua",
                "WTF/Account/B/Whitemane/Brannic/SavedVariables/ForeverBuddy.lua",
            ]
            .into_iter(),
        );
        assert!(older.contains(&("A".into(), "Classic Beta PvP 2".into())));
        assert!(!older.contains(&("A".into(), "70".into())));
        assert!(!older.iter().any(|(a, _)| a == "B"));
    }

    /// The watcher asks for a scan once per change, and only after the files
    /// have been still for `SETTLE`: a save in progress waits.
    #[test]
    fn the_watcher_asks_once_per_settled_change() {
        let tmp = tempfile::tempdir().unwrap();
        let rel = "WTF/Account/A/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua";
        write(tmp.path(), rel, b"one\n");
        let mut w = Watcher::default();
        assert!(w.poll(tmp.path()), "first look: scan");
        assert!(!w.poll(tmp.path()), "nothing changed");

        // A /reload writes it: too fresh to read yet, so no scan, and the
        // change isn't forgotten.
        std::fs::write(tmp.path().join(rel), b"two, longer\n").unwrap();
        assert!(!w.poll(tmp.path()), "still being written");
        assert!(!w.poll(tmp.path()), "still too fresh");
        // Once it's been still long enough: one scan.
        write(tmp.path(), rel, b"two, longer\n");
        assert!(w.poll(tmp.path()), "settled: scan");
        assert!(!w.poll(tmp.path()), "once");
    }

    /// An app core over a copy of the fixture game folder.
    fn backup_core() -> (tempfile::TempDir, std::path::PathBuf, AppCore) {
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
        (dir, root, core)
    }

    /// Writes the adventure file into `flavor` and takes a full backup of it.
    fn full_backup(core: &AppCore, root: &Path, flavor: &str) -> String {
        use crate::backup::manifest::Trigger;
        use crate::backup::{SnapshotRequest, SnapshotScope};

        write(
            &root.join(flavor),
            "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
            &fixture("adventure.lua"),
        );
        crate::install::set(&core.settings, root, Some(flavor)).unwrap();
        let game = core.active_game().unwrap();
        // ULIDs only sort by time across milliseconds.
        std::thread::sleep(std::time::Duration::from_millis(2));
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
            .unwrap()
            .expect("a new snapshot")
            .id
    }

    /// Gap-fill: a full backup's copy of the file is replayed once, then the
    /// marker stops it being replayed again.
    #[test]
    fn backups_are_replayed_once() {
        let (_dir, root, core) = backup_core();
        full_backup(&core, &root, "_classic_beta_");

        let ids = replay_backups(&core, "_classic_beta_").unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(count(&core.db, "SELECT count(*) FROM adventures"), 1);
        assert!(core
            .db
            .get_meta(&last_replayed_key("_classic_beta_"))
            .unwrap()
            .is_some());
        assert!(
            replay_backups(&core, "_classic_beta_").unwrap().is_empty(),
            "marker: once"
        );
        // Another flavor's backups aren't replayed into this one.
        assert!(replay_backups(&core, "_retail_").unwrap().is_empty());
    }

    /// O2: a forgotten character isn't brought back by a replay, even one
    /// that starts over (a db restored from an older daily copy).
    #[test]
    fn a_forgotten_character_stays_forgotten_through_a_replay() {
        let (_dir, root, core) = backup_core();
        full_backup(&core, &root, "_classic_beta_");
        let ids = replay_backups(&core, "_classic_beta_").unwrap();
        let flavor_dir = root.join("_classic_beta_");
        std::fs::remove_dir_all(flavor_dir.join("WTF/Account/ACCOUNT1/70/Thrandor-Vargur"))
            .unwrap();
        crate::tidy::forget(&core.db, "_classic_beta_", &flavor_dir, ids[0] as u32, 1).unwrap();

        core.db
            .set_meta(&last_replayed_key("_classic_beta_"), "")
            .unwrap();
        assert!(replay_backups(&core, "_classic_beta_").unwrap().is_empty());
        assert_eq!(count(&core.db, "SELECT count(*) FROM characters"), 0);
        assert_eq!(count(&core.db, "SELECT count(*) FROM adventures"), 0);
    }

    /// The marker is per flavor: replaying one flavor's newer backups
    /// doesn't skip another flavor's older ones.
    #[test]
    fn each_flavor_keeps_its_own_replay_marker() {
        let (_dir, root, core) = backup_core();
        let older = full_backup(&core, &root, "_retail_");
        let newer = full_backup(&core, &root, "_classic_beta_");
        assert!(older < newer);

        assert_eq!(replay_backups(&core, "_classic_beta_").unwrap().len(), 1);
        assert_eq!(replay_backups(&core, "_retail_").unwrap().len(), 1);
        assert_eq!(
            count(&core.db, "SELECT count(DISTINCT flavor) FROM characters"),
            2
        );
    }

    /// A snapshot whose manifest can't be read is skipped, not a stop for
    /// every later replay.
    #[test]
    fn an_unreadable_manifest_is_skipped() {
        let (_dir, root, core) = backup_core();
        let bad = full_backup(&core, &root, "_classic_beta_");
        full_backup(&core, &root, "_classic_beta_");
        let manifest = core
            .backups_dir()
            .join("snapshots")
            .join(format!("{bad}.json"));
        std::fs::write(&manifest, b"{ not json").unwrap();

        assert_eq!(replay_backups(&core, "_classic_beta_").unwrap().len(), 1);
        assert_eq!(count(&core.db, "SELECT count(*) FROM adventures"), 1);
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
