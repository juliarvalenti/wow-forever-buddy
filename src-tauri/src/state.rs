use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::backup::BackupService;
use crate::config::paths::AppPaths;
use crate::config::settings::SettingsStore;
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
        match self.settings.get().backup.location {
            Some(picked) => picked.join(STORE_FOLDER),
            None => self.paths.local_data_dir.join("backups"),
        }
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
