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
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Utc};

use crate::backup::manifest::{Scope, SnapshotSummary, Trigger};
use crate::backup::retention::POLICY;
use crate::backup::{SnapshotRequest, SnapshotScope};
use crate::error::AppResult;
use crate::state::AppCore;

pub const APP_START_MIN_AGE: Duration = Duration::from_secs(6 * 3600);
pub const EXIT_SETTLE: Duration = Duration::from_secs(5);
pub const EXIT_SETTLE_TIMEOUT: Duration = Duration::from_secs(120);
/// How often the scheduler checks whether a scheduled backup is due.
pub const SCHEDULE_TICK: Duration = Duration::from_secs(60);

/// Takes an automatic full snapshot and prunes. Returns `None` if it was
/// skipped: WoW is running, or nothing changed since the last full snapshot.
/// `on_created` runs for a new snapshot (the app emits `backup-created`).
pub fn run_auto(
    core: &AppCore,
    trigger: Trigger,
    on_created: &dyn Fn(&SnapshotSummary),
) -> AppResult<Option<SnapshotSummary>> {
    let _job = core.jobs.lock().expect("job lock poisoned");
    if core.game.is_running_now(&core.probe_target()) {
        return Ok(None);
    }
    let game = core.active_game()?;
    let store = core.backups()?;
    let include_addons = core.settings.get().backup.include_addons;
    let created = store.create(
        SnapshotRequest {
            game: &game.root,
            flavor: &game.flavor,
            trigger,
            label: None,
            scope: SnapshotScope::Full { include_addons },
            game_running: false,
        },
        &mut |_, _| {},
    )?;
    if let Some(summary) = &created {
        on_created(summary);
    }
    store.prune(Utc::now(), &POLICY)?;
    Ok(created)
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

/// A scheduled backup is due: a schedule is set and that long has passed.
pub fn schedule_due(core: &AppCore, now: DateTime<Utc>) -> AppResult<bool> {
    let hours = core.settings.get().backup.schedule_hours;
    Ok(hours > 0
        && older_than(
            last_full_snapshot(core)?,
            now,
            Duration::from_secs(u64::from(hours) * 3600),
        ))
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
