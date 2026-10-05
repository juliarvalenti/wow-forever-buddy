use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::backup::relocate::{self, MoveReport};
use crate::backup::BackupService;
use crate::config::paths::AppPaths;
use crate::config::settings::{Settings, SettingsPatch, SettingsStore};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::fsx::atomic::{sweep_temp_files, sweep_temp_files_shallow};
use crate::fsx::relpath::GameRoot;
use crate::game::gate::{MutationTarget, WriteGate};
use crate::game::process::{GameWatcher, ProbeTarget, ProcessProbe, SysinfoProbe};
use crate::secrets::{KeyringStore, SecretStore};

/// The store's own folder inside a backup location the user picked.
pub const STORE_FOLDER: &str = "WoW Forever Buddy backups";

/// Everything the app does, minus Tauri. Integration tests build this directly
/// against temp dirs; later tickets add the active install (T5).
pub struct AppCore {
    pub paths: AppPaths,
    pub settings: SettingsStore,
    pub db: Db,
    pub secrets: Arc<dyn SecretStore>,
    pub game: Arc<GameWatcher>,
    /// Opened on demand (see `backups()`), so a backup drive that isn't
    /// plugged in never stops the app from starting.
    backups: Mutex<Option<Arc<BackupService>>>,
    /// Backup and restore work runs one job at a time (spec §5): hold this for
    /// the whole operation. A second caller waits its turn.
    pub jobs: Mutex<()>,
}

/// The game folder the app works with right now.
pub struct ActiveGame {
    pub root: GameRoot,
    /// Flavor folder name, e.g. "_classic_beta_".
    pub flavor: String,
}

impl AppCore {
    /// The real app: secrets go to the OS credential store.
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        Self::with_secrets(paths, Arc::new(KeyringStore::new()))
    }

    /// Tests pass an in-memory store so they never touch the OS keyring.
    pub fn with_secrets(paths: AppPaths, secrets: Arc<dyn SecretStore>) -> AppResult<Self> {
        Self::with_parts(paths, secrets, Arc::new(SysinfoProbe::new()))
    }

    /// The one real constructor: the secret store and the process probe are
    /// the seams tests replace.
    pub fn with_parts(
        paths: AppPaths,
        secrets: Arc<dyn SecretStore>,
        probe: Arc<dyn ProcessProbe>,
    ) -> AppResult<Self> {
        for dir in [&paths.config_dir, &paths.local_data_dir, &paths.log_dir] {
            std::fs::create_dir_all(dir)?;
        }
        // Leftovers from a crash mid-write (spec §8 startup step 3). The game
        // folder is swept once the install is resolved. Local data is swept
        // shallowly: the backup store under it can be large, and the backup
        // code cleans up its own in-progress files (T7).
        sweep_temp_files(&paths.config_dir)?;
        sweep_temp_files_shallow(&paths.local_data_dir)?;

        let settings = SettingsStore::load(paths.settings_file())?;
        let db = Db::open(&paths.db_file())?;
        let core = Self {
            paths,
            settings,
            db,
            secrets,
            game: Arc::new(GameWatcher::new(probe)),
            backups: Mutex::new(None),
            jobs: Mutex::new(()),
        };
        // Open the store now if we can; if the drive is missing, commands
        // report it and the app still starts.
        let _ = core.backups();
        Ok(core)
    }

    /// Where backups go: the configured location, or `<local data>/backups`.
    /// A location the user picked (say `D:\Backups`) holds the store in its own
    /// subfolder, never at the top: the store's GC deletes files it doesn't
    /// recognize as referenced, so it must only ever see a folder it owns.
    pub fn backups_dir(&self) -> PathBuf {
        self.store_dir_for(self.settings.get().backup.location.as_deref())
    }

    fn store_dir_for(&self, location: Option<&std::path::Path>) -> PathBuf {
        match location {
            Some(picked) => picked.join(STORE_FOLDER),
            None => self.paths.local_data_dir.join("backups"),
        }
    }

    /// The UI's settings patch. The backup location isn't part of it: that
    /// moves the backups, so it goes through `move_backups`, which runs as a
    /// job and waits out an interrupted restore. Re-pointing the setting
    /// alone would strand the existing backups and a restore's snapshots.
    pub fn update_settings(&self, patch: SettingsPatch) -> AppResult<Settings> {
        if patch.backup.as_ref().is_some_and(|b| b.location.is_some()) {
            return Err(AppError::InvalidSettings(
                "the backup location changes by moving the backups (backup_move_location)".into(),
            ));
        }
        self.settings.apply_patch(patch)
    }

    /// Moves the backup store to `location` (None = the default) and points
    /// the setting there (F1), as a job so no backup or restore runs
    /// meanwhile. Refused while an interrupted restore waits, and for a
    /// location the setting would refuse anyway. See `backup::relocate`:
    /// the old store stays in use until the copy is complete and opens.
    pub fn move_backups(
        &self,
        location: Option<PathBuf>,
        progress: crate::backup::Progress<'_>,
    ) -> AppResult<MoveReport> {
        let _job = self.jobs.lock().expect("job lock poisoned");
        if crate::backup::journal::read(&self.paths.local_data_dir)?.is_some() {
            return Err(AppError::RestorePending);
        }
        self.settings
            .check(|s| s.backup.location = location.clone())?;

        let src = self.backups_dir();
        let dst = self.store_dir_for(location.as_deref());
        let report = |files, bytes, left_behind| MoveReport {
            dir: dst.display().to_string(),
            files,
            bytes: bytes as f64,
            left_behind,
        };
        if relocate::same_dir(&src, &dst) {
            self.settings.update(|s| s.backup.location = location)?;
            return Ok(report(0, 0, None));
        }

        // Nothing to carry over if the store was never created (or its
        // drive is gone): then this only points the setting elsewhere.
        let (files, bytes) = if src.exists() {
            relocate::copy_store(&src, &dst, progress)?
        } else {
            (0, 0)
        };
        let service = match BackupService::open(&dst, self.db.clone()) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dst);
                return Err(AppError::Io(format!(
                    "the copied backups didn't open, so nothing was moved: {e}"
                )));
            }
        };
        if let Err(e) = self.settings.update(|s| s.backup.location = location) {
            let _ = std::fs::remove_dir_all(&dst);
            return Err(e);
        }
        *self.backups.lock().expect("backups lock poisoned") = Some(service);

        // The new store is in use; the old copy goes. The old folder is the
        // app's own (the default, or a picked folder's STORE_FOLDER), never
        // the picked folder itself.
        let left_behind = match src.exists().then(|| std::fs::remove_dir_all(&src)) {
            Some(Err(e)) => {
                crate::applog::append(
                    &self.paths.log_dir,
                    &format!(
                        "moved backups, but the old folder stays: {} ({e})",
                        src.display()
                    ),
                );
                Some(src.display().to_string())
            }
            _ => None,
        };
        Ok(report(files, bytes, left_behind))
    }

    /// The backup store at the currently configured location, opened (or
    /// re-opened after the location changed in Settings) on demand. Fails
    /// with a clear message if the location isn't available, e.g. an
    /// unplugged USB drive.
    pub fn backups(&self) -> AppResult<Arc<BackupService>> {
        let dir = self.backups_dir();
        let mut slot = self.backups.lock().expect("backups lock poisoned");
        if let Some(service) = slot.as_ref().filter(|s| s.dir() == dir) {
            return Ok(service.clone());
        }
        let service = BackupService::open(&dir, self.db.clone())
            .map_err(|e| AppError::Io(format!("backups unavailable: {} ({e})", dir.display())))?;
        let service = Arc::new(service);
        *slot = Some(service.clone());
        Ok(service)
    }

    /// Today's copy of the db (`db::copies`), if it hasn't been taken yet.
    /// Called at startup and hourly; a failure is logged, never fatal.
    pub fn take_daily_db_copy(&self, today: chrono::NaiveDate) {
        let dir = crate::db::copies::dir_for(&self.paths.db_file());
        if let Err(e) = crate::db::copies::take_daily(&self.db, &dir, today) {
            crate::applog::append(&self.paths.log_dir, &format!("daily db copy failed: {e}"));
        }
    }

    /// Sets the game folder (`install::set`) as a job: it sweeps temp files
    /// in the WTF folder, which would break a restore that's mid-write, and
    /// switching folders under a running backup or restore is wrong anyway.
    pub fn set_install(
        &self,
        path: &std::path::Path,
        flavor: Option<&str>,
    ) -> AppResult<crate::install::layout::Install> {
        let _job = self.jobs.lock().expect("job lock poisoned");
        crate::install::set(&self.settings, path, flavor)
    }

    /// Startup's install resolution (`install::resolve_on_startup`), which
    /// may also sweep temp files, as a job for the same reason.
    pub fn resolve_install_on_startup(&self) -> Option<crate::install::layout::Install> {
        let _job = self.jobs.lock().expect("job lock poisoned");
        crate::install::resolve_on_startup(&self.settings)
    }

    /// The configured game folder, validated now. Until T5 lands this reads
    /// the install choice from settings; T5 switches it to the active install.
    pub fn active_game(&self) -> AppResult<ActiveGame> {
        let choice = self.settings.get().install.ok_or(AppError::NoInstall)?;
        // Re-validates the saved install, and refuses if a linked folder (a
        // WTF junction into Dropbox, say) was added, removed or re-pointed
        // since the user confirmed it.
        crate::install::current(&self.settings)?.ok_or(AppError::NoInstall)?;
        let flavor_dir: PathBuf = choice.root.join(&choice.flavor);
        if !flavor_dir.join("WTF").is_dir() {
            return Err(AppError::InvalidInstall(format!(
                "{} has no WTF folder",
                flavor_dir.display()
            )));
        }
        Ok(ActiveGame {
            // From the recorded links, so paths are checked against where the
            // links pointed at confirmation, not wherever they point now.
            root: GameRoot::from_saved(&choice)?,
            flavor: choice.flavor,
        })
    }

    /// The write gate for game-file changes, with backups as its safety net.
    pub fn write_gate(&self) -> AppResult<WriteGate> {
        Ok(WriteGate::new(
            self.game.clone(),
            self.backups()?,
            self.db.clone(),
        ))
    }

    /// What a change to the active game folder writes into.
    pub fn mutation_target(&self) -> AppResult<MutationTarget> {
        Ok(MutationTarget {
            game: self.active_game()?.root,
            probe: self.probe_target(),
        })
    }

    /// How to recognize our running game, from the current settings.
    pub fn probe_target(&self) -> ProbeTarget {
        let settings = self.settings.get();
        ProbeTarget {
            root: settings.install.map(|i| i.root),
            extra_names: settings.process_names_extra,
        }
    }
}

/// Managed Tauri state. A thin wrapper so commands depend on `AppCore`, not on Tauri.
pub struct AppState {
    pub core: AppCore,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_creates_its_dirs_and_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::with_secrets(
            AppPaths::under(tmp.path()),
            Arc::new(crate::secrets::MemoryStore::default()),
        )
        .unwrap();
        assert!(core.paths.config_dir.is_dir());
        assert!(core.paths.local_data_dir.is_dir());
        assert!(core.paths.log_dir.is_dir());
        assert!(core.paths.settings_file().is_file());
        assert!(core.paths.db_file().is_file());
    }

    #[test]
    fn startup_sweeps_leftover_temp_files() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(tmp.path());
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        let leftover = paths.config_dir.join(".settings.json.wfb-tmp-x1");
        std::fs::write(&leftover, b"partial").unwrap();

        AppCore::new(paths).unwrap();
        assert!(!leftover.exists());
    }

    fn set_location(core: &AppCore, dir: &std::path::Path) {
        core.settings
            .update(|s| s.backup.location = Some(dir.to_path_buf()))
            .unwrap();
    }

    /// Review must-fix 3: an unplugged backup drive never stops the app
    /// from starting; backup commands report it, and it recovers when the
    /// drive comes back.
    #[test]
    fn missing_backup_drive_does_not_block_startup() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(&tmp.path().join("app"));
        // A file where the drive's folder should be makes the location unusable.
        let unplugged = tmp.path().join("E-drive");
        std::fs::write(&unplugged, b"not a folder").unwrap();
        {
            let core = AppCore::new(paths.clone()).unwrap();
            set_location(&core, &unplugged.join("WoWBackups"));
        }

        let core = AppCore::new(paths).expect("starts without the backup drive");
        let err = core.backups().err().expect("store is unavailable");
        assert!(
            matches!(err, AppError::Io(ref m) if m.contains("backups unavailable")),
            "{err:?}"
        );

        // The drive comes back.
        std::fs::remove_file(&unplugged).unwrap();
        std::fs::create_dir(&unplugged).unwrap();
        assert!(core.backups().is_ok());
    }

    #[test]
    fn changing_the_location_reopens_the_store() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::new(AppPaths::under(&tmp.path().join("app"))).unwrap();
        let first = core.backups().unwrap();
        assert_eq!(first.dir(), core.paths.local_data_dir.join("backups"));

        let elsewhere = tmp.path().join("Elsewhere");
        set_location(&core, &elsewhere);
        let second = core.backups().unwrap();
        assert_eq!(
            second.dir(),
            elsewhere.join(STORE_FOLDER),
            "H1: in its own subfolder of the picked location"
        );
        assert!(
            Arc::ptr_eq(&second, &core.backups().unwrap()),
            "reused while unchanged"
        );
    }

    /// A core with the fixture game and one full snapshot.
    fn core_with_a_snapshot() -> (tempfile::TempDir, AppCore, String) {
        let (dir, root) = crate::test_support::fixture_copy();
        let core = AppCore::new(AppPaths::under(&dir.path().join("app"))).unwrap();
        crate::install::set(&core.settings, &root, None).unwrap();
        let game = core.active_game().unwrap();
        let summary = core
            .backups()
            .unwrap()
            .create(
                crate::backup::SnapshotRequest {
                    game: &game.root,
                    flavor: &game.flavor,
                    trigger: crate::backup::manifest::Trigger::Manual,
                    label: None,
                    scope: crate::backup::SnapshotScope::Full {
                        include_addons: false,
                    },
                    game_running: false,
                },
                &mut |_, _| {},
            )
            .unwrap()
            .unwrap();
        (dir, core, summary.id)
    }

    /// Every snapshot listed, and every blob it needs readable.
    fn assert_whole(core: &AppCore, id: &str) {
        let store = core.backups().unwrap();
        let ids: Vec<_> = store.list().unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![id.to_string()]);
        for hash in store.referenced_blobs().unwrap() {
            assert!(store.blobs().check(&hash).is_none(), "blob {hash} readable");
        }
    }

    fn mv(core: &AppCore, location: Option<PathBuf>) -> AppResult<MoveReport> {
        core.move_backups(location, &mut |_, _| {})
    }

    /// F1: moving the store carries every snapshot over, switches to it,
    /// and removes the old copy; moving back to the default works too.
    #[test]
    fn moving_the_store_carries_the_backups() {
        let (dir, core, id) = core_with_a_snapshot();
        let default = core.backups_dir();
        let elsewhere = dir.path().join("Elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();

        let report = mv(&core, Some(elsewhere.clone())).unwrap();
        assert!(
            report.files > 0 && report.left_behind.is_none(),
            "{report:?}"
        );
        assert_eq!(core.backups_dir(), elsewhere.join(STORE_FOLDER));
        assert_eq!(core.settings.get().backup.location, Some(elsewhere.clone()));
        assert!(!default.exists(), "the old copy is gone");
        assert_whole(&core, &id);

        mv(&core, None).unwrap();
        assert_eq!(core.backups_dir(), default);
        assert!(!elsewhere.join(STORE_FOLDER).exists());
        assert!(elsewhere.is_dir(), "the picked folder itself stays");
        assert_whole(&core, &id);
    }

    /// The UI's patch can't re-point the backups without moving them.
    #[test]
    fn a_settings_patch_cannot_change_the_backup_location() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::new(AppPaths::under(&tmp.path().join("app"))).unwrap();
        let patch = |v: serde_json::Value| serde_json::from_value::<SettingsPatch>(v).unwrap();

        let elsewhere = tmp.path().join("Elsewhere");
        for location in [serde_json::json!(elsewhere), serde_json::Value::Null] {
            let err = core
                .update_settings(patch(
                    serde_json::json!({ "backup": { "location": location } }),
                ))
                .unwrap_err();
            assert!(matches!(err, AppError::InvalidSettings(_)), "{err:?}");
        }
        assert_eq!(core.settings.get().backup.location, None);

        let s = core
            .update_settings(patch(
                serde_json::json!({ "backup": { "on_app_start": false } }),
            ))
            .unwrap();
        assert!(!s.backup.on_app_start, "everything else still goes through");
    }

    /// Refusals change nothing: a folder with someone's files in it, one
    /// inside the game folder, and any move while a restore waits.
    #[test]
    fn a_refused_move_changes_nothing() {
        let (dir, core, id) = core_with_a_snapshot();
        let before = core.backups_dir();

        let used = dir.path().join("Used");
        std::fs::create_dir_all(used.join(STORE_FOLDER)).unwrap();
        std::fs::write(used.join(STORE_FOLDER).join("notes.txt"), b"mine").unwrap();
        assert!(mv(&core, Some(used)).is_err());

        let game_root = core.settings.get().install.unwrap().root;
        let inside = game_root.join("Backups");
        assert!(matches!(
            mv(&core, Some(inside.clone())),
            Err(AppError::InvalidSettings(_))
        ));
        assert!(!inside.exists(), "refused before copying anything");

        std::fs::write(
            core.paths
                .local_data_dir
                .join(crate::backup::journal::FILE_NAME),
            b"{ interrupted",
        )
        .unwrap();
        assert!(mv(&core, Some(dir.path().join("Free"))).is_err());
        assert!(!dir.path().join("Free").exists());

        assert_eq!(core.backups_dir(), before);
        assert_eq!(core.settings.get().backup.location, None);
        assert_whole(&core, &id);
    }

    /// Review note: the game root comes from the links recorded when the
    /// install was confirmed, and a link re-pointed later is refused.
    #[test]
    fn active_game_uses_recorded_links_and_refuses_repointed_ones() {
        let (dir, root) = crate::test_support::fixture_copy();
        let flavor = root.join("_classic_beta_");
        let synced = dir.path().join("Dropbox").join("WTF");
        std::fs::create_dir_all(synced.parent().unwrap()).unwrap();
        std::fs::rename(flavor.join("WTF"), &synced).unwrap();
        crate::test_support::link_dir(&synced, &flavor.join("WTF"));

        let core = AppCore::new(AppPaths::under(&dir.path().join("app"))).unwrap();
        assert!(matches!(core.active_game(), Err(AppError::NoInstall)));
        crate::install::set(&core.settings, &root, None).unwrap();

        let game = core.active_game().unwrap();
        assert_eq!(game.flavor, "_classic_beta_");
        assert_eq!(game.root.links.len(), 1, "the WTF link was recorded");

        // Someone points WTF somewhere else.
        let other = dir.path().join("Other");
        std::fs::create_dir_all(&other).unwrap();
        crate::test_support::unlink_dir(&flavor.join("WTF"));
        crate::test_support::link_dir(&other, &flavor.join("WTF"));
        assert!(core.active_game().is_err(), "re-pointed link refused");
    }

    /// H1: changing the game folder (which sweeps temp files in WTF) waits
    /// for a running backup or restore instead of breaking it.
    #[test]
    fn set_install_waits_for_the_job_lock() {
        let (dir, root) = crate::test_support::fixture_copy();
        let core = Arc::new(AppCore::new(AppPaths::under(&dir.path().join("app"))).unwrap());
        let job = core.jobs.lock().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = {
            let core = core.clone();
            std::thread::spawn(move || {
                let result = core.set_install(&root, None);
                tx.send(()).unwrap();
                result
            })
        };
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(300))
                .is_err(),
            "blocked while a job runs"
        );
        drop(job);
        rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        worker.join().unwrap().unwrap();
    }
}
