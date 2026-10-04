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

/// Everything the app does, minus Tauri. Integration tests build this directly
/// against temp dirs; later tickets add the active install (T5).
pub struct AppCore {
    pub paths: AppPaths,
    pub settings: SettingsStore,
    pub db: Db,
    pub secrets: Arc<dyn SecretStore>,
    pub game: Arc<GameWatcher>,
    pub backups: Arc<BackupService>,
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
        let backups_dir = settings
            .get()
            .backup
            .location
            .unwrap_or_else(|| paths.local_data_dir.join("backups"));
        let backups = Arc::new(BackupService::open(&backups_dir, db.clone())?);
        Ok(Self {
            paths,
            settings,
            db,
            secrets,
            game: Arc::new(GameWatcher::new(probe)),
            backups,
            jobs: Mutex::new(()),
        })
    }

    /// The configured game folder, validated now. Until T5 lands this reads
    /// the install choice from settings; T5 switches it to the active install.
    pub fn active_game(&self) -> AppResult<ActiveGame> {
        let install = self.settings.get().install.ok_or(AppError::NoInstall)?;
        let flavor_dir: PathBuf = install.root.join(&install.flavor);
        if !flavor_dir.join("WTF").is_dir() {
            return Err(AppError::InvalidInstall(format!(
                "{} has no WTF folder",
                flavor_dir.display()
            )));
        }
        Ok(ActiveGame {
            root: GameRoot::new(&flavor_dir)?,
            flavor: install.flavor,
        })
    }

    /// The write gate for game-file changes, with backups as its safety net.
    #[allow(dead_code)] // first caller is restore (T9)
    pub fn write_gate(&self) -> WriteGate {
        WriteGate::new(self.game.clone(), self.backups.clone(), self.db.clone())
    }

    /// What a change to the active game folder writes into.
    #[allow(dead_code)] // first caller is restore (T9)
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
}
