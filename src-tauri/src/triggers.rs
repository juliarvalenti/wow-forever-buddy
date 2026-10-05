//! Automatic backups (spec §5 "Triggers").
//!
//! - **App start:** on launch, if the last full snapshot is more than 6 h old.
//! - **Game exit:** when WoW stops, once the WTF folder has been quiet for 5 s
//!   (WoW writes SavedVariables while it shuts down).
//! - **Scheduled:** every `schedule_hours` while the app is open (0 = off).
//!
//! None of them run while WoW is running: they wait for the game-exit backup
//! instead, so they never capture a half-written file from a `/reload`.
//! Each one runs as a job (`AppCore::jobs`), then applies retention.

use std::path::Path;
use std::sync::MutexGuard;
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::applog;
use crate::backup::manifest::{Scope, SnapshotSummary, Trigger};
use crate::backup::retention::POLICY;
use crate::backup::{journal, Gc, SnapshotRequest, SnapshotScope};
use crate::error::{AppError, AppResult};
use crate::game::process::GameCheck;
use crate::state::AppCore;

pub const APP_START_MIN_AGE: Duration = Duration::from_secs(6 * 3600);
pub const EXIT_SETTLE: Duration = Duration::from_secs(5);
pub const EXIT_SETTLE_TIMEOUT: Duration = Duration::from_secs(120);
/// How often the scheduler checks whether a scheduled backup is due.
pub const SCHEDULE_TICK: Duration = Duration::from_secs(60);

/// Meta key: when an automatic backup last ran (written or skipped), so a
/// skipped run doesn't stay "due" and re-run every tick.
const LAST_AUTO_ATTEMPT: &str = "last_auto_attempt";
/// Meta key: the latest automatic backup failure (JSON), until one succeeds.
const LAST_AUTO_FAILURE: &str = "last_auto_failure";

/// An automatic backup that failed, or that left files out, for the Backups
/// screen (R1): nobody is watching when one runs, so it's kept until a later
/// automatic backup captures everything, and the UI asks for it on start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct AutoBackupFailure {
    /// RFC 3339, UTC.
    pub at: String,
    pub trigger: Trigger,
    /// The error, as shown to the user.
    pub error: String,
    /// 0: the backup failed. Otherwise it was taken, but this many files
    /// couldn't be read and were left out.
    #[serde(default)]
    pub skipped: u32,
}

/// The automatic backup failure still standing, if any.
pub fn last_failure(core: &AppCore) -> AppResult<Option<AutoBackupFailure>> {
    Ok(core
        .db
        .get_meta(LAST_AUTO_FAILURE)?
        .and_then(|json| serde_json::from_str(&json).ok()))
}

/// Takes an automatic full snapshot, then prunes if one was written.
/// Returns `None` if it was skipped: WoW is running, or nothing changed
/// since the last full snapshot. `on_created` runs for a new snapshot (the
/// app emits `backup-created`). A failure is logged and kept for the UI
/// (`last_failure`) before it's returned.
pub fn run_auto(
    core: &AppCore,
    trigger: Trigger,
    on_created: &dyn Fn(&SnapshotSummary),
) -> AppResult<Option<SnapshotSummary>> {
    let job = core.jobs.lock().expect("job lock poisoned");
    run_auto_locked(core, &job, trigger, on_created)
}

/// `run_auto` for a caller that already holds `AppCore::jobs` (the guard is
/// the proof).
pub fn run_auto_locked(
    core: &AppCore,
    _job: &MutexGuard<'_, ()>,
    trigger: Trigger,
    on_created: &dyn Fn(&SnapshotSummary),
) -> AppResult<Option<SnapshotSummary>> {
    if let Err(e) = core
        .db
        .set_meta(LAST_AUTO_ATTEMPT, &Utc::now().to_rfc3339())
    {
        record_failure(core, trigger, &e, 0);
        return Err(e);
    }
    // Running: wait for the game-exit backup. Unknown (the process list
    // failed): back up anyway, flagged like a mid-session manual backup,
    // so a broken probe can't silently stop every automatic backup.
    let game_running = match core.game.check_now(&core.probe_target()) {
        GameCheck::Running => return Ok(None),
        GameCheck::Unknown => true,
        GameCheck::NotRunning => false,
    };
    let created = match snapshot(core, trigger, game_running) {
        Ok(created) => created,
        // Not set up yet (first run): nothing to back up, nothing wrong.
        Err(AppError::NoInstall) => return Err(AppError::NoInstall),
        Err(e) => {
            record_failure(core, trigger, &e, 0);
            return Err(e);
        }
    };
    match &created {
        // A new snapshot settles it: clear, or warn while it left files out.
        Some(summary) => {
            let skipped = core
                .backups()
                .and_then(|store| store.manifest(&summary.id))
                .map(|m| m.skipped)
                .unwrap_or_default();
            match skipped.first() {
                None => clear_failure(core)?,
                Some(first) => {
                    let why = format!(
                        "Couldn't read {} (first: {}: {}).",
                        plural_files(skipped.len()),
                        first.path,
                        first.reason
                    );
                    record_failure(core, trigger, &why, skipped.len() as u32);
                }
            }
        }
        // Identical to the latest full snapshot: a standing failure is
        // over, but a "left files out" warning still describes that snapshot.
        None => {
            if last_failure(core)?.is_some_and(|f| f.skipped == 0) {
                clear_failure(core)?;
            }
        }
    }
    if let Some(summary) = &created {
        on_created(summary);
        // Spec §5: prune after each *new* snapshot; GC itself is hourly.
        // Never while an unreadable restore journal hides which snapshots a
        // roll back needs: skip until the user resolves it. The backup itself
        // succeeded, so a pruning error is only logged.
        if let Ok(held) = journal::held_snapshots(&core.paths.local_data_dir) {
            let pruned = core
                .backups()
                .and_then(|store| store.prune(Utc::now(), &POLICY, Gc::Throttled, &held));
            if let Err(e) = pruned {
                applog::append(&core.paths.log_dir, &format!("prune failed: {e}"));
            }
        }
    }
    Ok(created)
}

fn snapshot(
    core: &AppCore,
    trigger: Trigger,
    game_running: bool,
) -> AppResult<Option<SnapshotSummary>> {
    let game = core.active_game()?;
    let include_addons = core.settings.get().backup.include_addons;
    core.backups()?.create(
        SnapshotRequest {
            game: &game.root,
            flavor: &game.flavor,
            trigger,
            label: None,
            scope: SnapshotScope::Full { include_addons },
            game_running,
        },
        &mut |_, _| {},
    )
}

fn plural_files(n: usize) -> String {
    if n == 1 {
        "1 file".into()
    } else {
        format!("{n} files")
    }
}

/// Logs it and keeps it for the UI. `skipped` > 0: the backup was taken but
/// left that many files out.
fn record_failure(core: &AppCore, trigger: Trigger, e: &dyn std::fmt::Display, skipped: u32) {
    let failure = AutoBackupFailure {
        at: Utc::now().to_rfc3339(),
        trigger,
        error: e.to_string(),
        skipped,
    };
    let what = if skipped == 0 {
        "failed"
    } else {
        "left files out"
    };
    applog::append(
        &core.paths.log_dir,
        &format!("automatic backup ({}) {what}: {e}", trigger.as_str()),
    );
    let json = serde_json::to_string(&failure).expect("failure serializes");
    let _ = core.db.set_meta(LAST_AUTO_FAILURE, &json);
}

fn clear_failure(core: &AppCore) -> AppResult<()> {
    core.db.with_conn(|c| {
        c.execute("DELETE FROM meta WHERE key = ?1", [LAST_AUTO_FAILURE])?;
        Ok(())
    })
}

/// The game-exit backup. Takes the job lock *before* waiting for WTF to
/// settle, so a restore clicked the moment WoW closes queues behind this
/// backup instead of running before WoW's exit writes are captured (spec §5).
pub fn game_exit_backup(
    core: &AppCore,
    settle: Duration,
    timeout: Duration,
    on_created: &dyn Fn(&SnapshotSummary),
) -> AppResult<Option<SnapshotSummary>> {
    let job = core.jobs.lock().expect("job lock poisoned");
    if let Ok(game) = core.active_game() {
        wait_until_settled(&game.root.base.join("WTF"), settle, timeout);
    }
    run_auto_locked(core, &job, Trigger::GameExit, on_created)
}

/// When the newest full snapshot (any trigger) was taken, if there is one.
pub fn last_full_snapshot(core: &AppCore) -> AppResult<Option<DateTime<Utc>>> {
    Ok(core
        .backups()?
        .list()?
        .into_iter()
        .filter(|s| s.scope == Scope::Full)
        .filter_map(|s| DateTime::parse_from_rfc3339(&s.created_at).ok())
        .map(|d| d.with_timezone(&Utc))
        .max())
}

fn older_than(last: Option<DateTime<Utc>>, now: DateTime<Utc>, age: Duration) -> bool {
    last.is_none_or(|t| now.signed_duration_since(t).to_std().unwrap_or_default() >= age)
}

/// The app-start backup is due: enabled, and the last full snapshot is old.
pub fn app_start_due(core: &AppCore, now: DateTime<Utc>) -> AppResult<bool> {
    Ok(core.settings.get().backup.on_app_start
        && older_than(last_full_snapshot(core)?, now, APP_START_MIN_AGE))
}

/// A scheduled backup is due: a schedule is set, and that long has passed
/// since both the last full snapshot and the last automatic attempt (an
/// attempt skipped as unchanged counts, so it doesn't re-run every tick).
pub fn schedule_due(core: &AppCore, now: DateTime<Utc>) -> AppResult<bool> {
    let hours = core.settings.get().backup.schedule_hours;
    let last_attempt = core
        .db
        .get_meta(LAST_AUTO_ATTEMPT)?
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&Utc));
    let last = last_full_snapshot(core)?.max(last_attempt);
    Ok(hours > 0 && older_than(last, now, Duration::from_secs(u64::from(hours) * 3600)))
}

/// Waits until nothing under `dir` has changed for `settle`, checking every
/// 250 ms. Returns false if it's still changing after `timeout` (the
/// game-exit backup then runs anyway: WoW is closed, and safe_read retries).
pub fn wait_until_settled(dir: &Path, settle: Duration, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    let mut last = newest_mtime(dir);
    let mut quiet_since = Instant::now();
    loop {
        if quiet_since.elapsed() >= settle {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250).min(settle));
        let now = newest_mtime(dir);
        if now != last {
            last = now;
            quiet_since = Instant::now();
        }
    }
}

/// Newest modification time of any file under `dir` (and how many files),
/// so additions, deletions and edits all count as change.
fn newest_mtime(dir: &Path) -> (Option<SystemTime>, usize) {
    let mut newest = None;
    let mut count = 0;
    for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if entry.file_type().is_file() {
            count += 1;
            if let Some(m) = entry.metadata().ok().and_then(|m| m.modified().ok()) {
                newest = newest.max(Some(m));
            }
        }
    }
    (newest, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::paths::AppPaths;
    use crate::game::process::fake::FakeProbe;
    use crate::secrets::MemoryStore;
    use crate::test_support::fixture_copy;
    use std::sync::Arc;

    struct Setup {
        _dir: tempfile::TempDir,
        flavor: std::path::PathBuf,
        probe: Arc<FakeProbe>,
        core: AppCore,
    }

    fn setup() -> Setup {
        let (dir, root) = fixture_copy();
        let probe = Arc::new(FakeProbe::default());
        let core = AppCore::with_parts(
            AppPaths::under(&dir.path().join("app")),
            Arc::new(MemoryStore::default()),
            probe.clone(),
        )
        .unwrap();
        crate::install::set(&core.settings, &root, Some("_classic_beta_")).unwrap();
        Setup {
            flavor: root.join("_classic_beta_"),
            _dir: dir,
            probe,
            core,
        }
    }

    #[test]
    fn auto_backup_runs_skips_identical_and_reports() {
        let s = setup();
        let seen = std::sync::Mutex::new(Vec::new());
        let record = |summary: &SnapshotSummary| seen.lock().unwrap().push(summary.id.clone());

        let first = run_auto(&s.core, Trigger::GameExit, &record)
            .unwrap()
            .unwrap();
        assert_eq!(first.trigger, Trigger::GameExit);
        assert!(
            run_auto(&s.core, Trigger::Scheduled, &record)
                .unwrap()
                .is_none(),
            "unchanged"
        );
        assert_eq!(*seen.lock().unwrap(), [first.id]);
    }

    #[test]
    fn auto_backup_never_runs_while_wow_runs() {
        let s = setup();
        s.probe.set_running(true);
        assert!(run_auto(&s.core, Trigger::Scheduled, &|_| {})
            .unwrap()
            .is_none());
        assert!(s.core.backups().unwrap().list().unwrap().is_empty());
    }

    /// R1: a failed automatic backup is logged and kept for the UI until an
    /// automatic backup succeeds; "not set up yet" isn't a failure.
    #[test]
    fn failures_are_logged_and_kept_until_a_success() {
        let s = setup();
        assert_eq!(last_failure(&s.core).unwrap(), None);

        // The game folder goes away (say the drive was unplugged).
        let parked = s.flavor.with_file_name("parked");
        std::fs::rename(&s.flavor, &parked).unwrap();
        let result = run_auto(&s.core, Trigger::Scheduled, &|_| {});
        std::fs::rename(&parked, &s.flavor).unwrap();
        assert!(result.is_err());

        let failure = last_failure(&s.core).unwrap().expect("kept");
        assert_eq!(failure.trigger, Trigger::Scheduled);
        assert!(!failure.error.is_empty());
        let log = std::fs::read_to_string(s.core.paths.log_dir.join("buddy.log")).unwrap();
        assert!(log.contains("automatic backup (scheduled) failed"));

        run_auto(&s.core, Trigger::Scheduled, &|_| {}).unwrap();
        assert_eq!(last_failure(&s.core).unwrap(), None, "cleared by a success");
    }

    /// #27 review: an automatic backup that left files out keeps a standing
    /// warning (not a failure) until one captures everything.
    #[cfg(unix)]
    #[test]
    fn skipped_files_keep_a_warning_until_a_complete_backup() {
        use std::os::unix::fs::PermissionsExt;

        let s = setup();
        let config = s.flavor.join("WTF/Config.wtf");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&config).is_ok() {
            return; // running as root
        }
        let made = run_auto(&s.core, Trigger::GameExit, &|_| {});
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(made.unwrap().is_some(), "taken anyway");
        let warning = last_failure(&s.core).unwrap().expect("warning kept");
        assert_eq!(warning.skipped, 1);
        assert!(
            warning.error.contains("WTF/Config.wtf"),
            "{}",
            warning.error
        );

        // Readable again: the next backup is complete and clears it.
        assert!(run_auto(&s.core, Trigger::Scheduled, &|_| {})
            .unwrap()
            .is_some());
        assert_eq!(last_failure(&s.core).unwrap(), None);
    }

    /// R2: when the process list fails ("unknown"), automatic backups still
    /// run, flagged as taken while the game may be running; writes stay
    /// blocked.
    #[test]
    fn unknown_game_state_still_backs_up_flagged() {
        let s = setup();
        s.probe.set_blind(true);
        let created = run_auto(&s.core, Trigger::Scheduled, &|_| {})
            .unwrap()
            .expect("backed up");
        assert!(created.game_running);
        assert!(s.core.game.is_running_now(&s.core.probe_target()));
    }

    #[test]
    fn app_start_and_schedule_are_due_by_age_and_settings() {
        let s = setup();
        let now = Utc::now();
        assert!(app_start_due(&s.core, now).unwrap(), "no snapshot yet");
        assert!(schedule_due(&s.core, now).unwrap());

        run_auto(&s.core, Trigger::Manual, &|_| {}).unwrap();
        assert!(!app_start_due(&s.core, now).unwrap(), "just backed up");
        assert!(app_start_due(&s.core, now + chrono::Duration::hours(7)).unwrap());
        assert!(!schedule_due(&s.core, now + chrono::Duration::hours(23)).unwrap());
        assert!(schedule_due(&s.core, now + chrono::Duration::hours(25)).unwrap());

        s.core
            .settings
            .update(|st| {
                st.backup.on_app_start = false;
                st.backup.schedule_hours = 0;
            })
            .unwrap();
        let later = now + chrono::Duration::days(30);
        assert!(!app_start_due(&s.core, later).unwrap());
        assert!(!schedule_due(&s.core, later).unwrap(), "0 means off");
    }

    /// Review item 2: a run skipped as unchanged counts as an attempt (so
    /// the scheduler doesn't re-run it every minute) and doesn't prune.
    #[test]
    fn skipped_runs_count_as_attempts_and_skip_pruning() {
        let s = setup();
        run_auto(&s.core, Trigger::GameExit, &|_| {})
            .unwrap()
            .unwrap();
        let gc_after_first = s.core.db.get_meta("last_gc").unwrap();
        assert!(
            gc_after_first.is_some(),
            "a new snapshot prunes (GC was due)"
        );

        std::thread::sleep(Duration::from_millis(5));
        assert!(run_auto(&s.core, Trigger::Scheduled, &|_| {})
            .unwrap()
            .is_none());
        assert_eq!(
            s.core.db.get_meta("last_gc").unwrap(),
            gc_after_first,
            "a skipped run doesn't prune or GC"
        );

        // The full snapshot is old, but an attempt was just made: not due.
        s.core
            .settings
            .update(|st| st.backup.schedule_hours = 1)
            .unwrap();
        let in_two_hours = Utc::now() + chrono::Duration::hours(2);
        s.core
            .db
            .set_meta(
                LAST_AUTO_ATTEMPT,
                &(in_two_hours - chrono::Duration::minutes(30)).to_rfc3339(),
            )
            .unwrap();
        assert!(!schedule_due(&s.core, in_two_hours).unwrap());
        assert!(schedule_due(&s.core, in_two_hours + chrono::Duration::hours(1)).unwrap());
    }

    /// Review item 1: with an unreadable restore journal the backup still
    /// runs, but nothing is pruned.
    #[test]
    fn an_unreadable_journal_skips_pruning() {
        let s = setup();
        let dir = &s.core.paths.local_data_dir;
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(journal::FILE_NAME), b"{ damaged").unwrap();
        assert!(run_auto(&s.core, Trigger::GameExit, &|_| {})
            .unwrap()
            .is_some());
        assert_eq!(s.core.db.get_meta("last_gc").unwrap(), None, "no prune ran");
    }

    /// Review item 3: the game-exit backup holds the job lock while it waits
    /// for WTF to settle, so a restore can't slip in ahead of it.
    #[test]
    fn game_exit_backup_holds_the_job_lock_while_settling() {
        let s = Arc::new(setup());
        let worker = {
            let s = s.clone();
            std::thread::spawn(move || {
                game_exit_backup(
                    &s.core,
                    Duration::from_millis(600),
                    Duration::from_secs(5),
                    &|_| {},
                )
            })
        };
        std::thread::sleep(Duration::from_millis(150));
        assert!(
            s.core.jobs.try_lock().is_err(),
            "a restore would have to wait during the settle"
        );
        let created = worker.join().unwrap().unwrap().unwrap();
        assert_eq!(created.trigger, Trigger::GameExit);
        assert!(s.core.jobs.try_lock().is_ok(), "released afterwards");
    }

    #[test]
    fn waits_for_the_folder_to_go_quiet() {
        let s = setup();
        let wtf = s.flavor.join("WTF");
        let writer_dir = wtf.clone();
        // WoW keeps writing for ~600 ms after exit.
        let writer = std::thread::spawn(move || {
            for i in 0..4 {
                std::thread::sleep(Duration::from_millis(150));
                std::fs::write(writer_dir.join(format!("late-{i}.lua")), b"x").unwrap();
            }
        });
        let started = Instant::now();
        assert!(wait_until_settled(
            &wtf,
            Duration::from_millis(500),
            Duration::from_secs(10)
        ));
        assert!(
            started.elapsed() >= Duration::from_millis(1000),
            "waited for the writes plus quiet"
        );
        writer.join().unwrap();

        // Never quiet within the timeout: gives up (returns false).
        let busy = wtf.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        let churn = std::thread::spawn(move || {
            let mut i = 0;
            while !stop2.load(std::sync::atomic::Ordering::SeqCst) {
                std::fs::write(busy.join("churn.lua"), format!("{i}")).unwrap();
                i += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        assert!(!wait_until_settled(
            &wtf,
            Duration::from_secs(5),
            Duration::from_millis(800)
        ));
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        churn.join().unwrap();
    }
}
