use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::config::paths::AppPaths;
use crate::config::settings::{InstallChoice, SettingsStore};
use crate::db::Db;
use crate::error::AppResult;
use crate::fsx::atomic::sweep_temp_files;
use crate::install::layout::ActiveInstall;

/// Everything the app does, minus Tauri. Integration tests build this directly
/// against temp dirs; later tickets add game status and jobs.
pub struct AppCore {
    pub paths: AppPaths,
    pub settings: SettingsStore,
    #[allow(dead_code)] // first read by the backup store (T7)
    pub db: Db,
    install: RwLock<Option<ActiveInstall>>,
}

impl AppCore {
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        for dir in [&paths.config_dir, &paths.local_data_dir, &paths.log_dir] {
            std::fs::create_dir_all(dir)?;
        }
        // Leftovers from a crash mid-write (spec §8 startup step 3).
        sweep_temp_files(&paths.config_dir)?;
        sweep_temp_files(&paths.local_data_dir)?;

        let settings = SettingsStore::load(paths.settings_file())?;
        let db = Db::open(&paths.db_file())?;

        // Re-validate the saved install. If it's missing (unplugged drive),
        // the choice stays in settings and the UI shows "game folder not found".
        let install = settings
            .get()
            .install
            .as_ref()
            .and_then(crate::install::resolve_saved);
        if let Some(active) = &install {
            sweep_game_temp_files(&active.flavor.dir);
        }

        Ok(Self {
            paths,
            settings,
            db,
            install: RwLock::new(install),
        })
    }

    pub fn install(&self) -> Option<ActiveInstall> {
        self.install.read().expect("install lock poisoned").clone()
    }

    /// Makes `active` the install the app works with, and saves the choice.
    pub fn set_install(&self, active: ActiveInstall) -> AppResult<()> {
        let choice = InstallChoice {
            root: active.root.clone(),
            flavor: active.flavor.id.clone(),
        };
        self.settings.update(|s| s.install = Some(choice))?;
        sweep_game_temp_files(&active.flavor.dir);
        *self.install.write().expect("install lock poisoned") = Some(active);
        Ok(())
    }

    /// Where backups live: the configured location, or `<local data>/backups`.
    pub fn backups_dir(&self) -> PathBuf {
        self.settings
            .get()
            .backup
            .location
            .unwrap_or_else(|| self.paths.local_data_dir.join("backups"))
    }
}

/// Our temp files are only ever written under WTF/, so that's all we sweep.
/// Best effort: a failure here must not block startup.
fn sweep_game_temp_files(flavor_dir: &Path) {
    let _ = sweep_temp_files(&flavor_dir.join("WTF"));
}

/// Managed Tauri state. A thin wrapper so commands depend on `AppCore`, not on Tauri.
pub struct AppState {
    pub core: AppCore,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_copy;

    #[test]
    fn core_creates_its_dirs_and_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::new(AppPaths::under(tmp.path())).unwrap();
        assert!(core.paths.config_dir.is_dir());
        assert!(core.paths.local_data_dir.is_dir());
        assert!(core.paths.log_dir.is_dir());
        assert!(core.paths.settings_file().is_file());
        assert!(core.paths.db_file().is_file());
        assert_eq!(core.install(), None);
        assert_eq!(
            core.backups_dir(),
            core.paths.local_data_dir.join("backups")
        );
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

    #[test]
    fn install_choice_persists_and_is_revalidated() {
        let (dir, root) = fixture_copy();
        let paths = AppPaths::under(&dir.path().join("app"));
        let leftover = root.join("_classic_beta_/WTF/.Config.wtf.wfb-tmp-1");
        std::fs::write(&leftover, b"partial").unwrap();

        let core = AppCore::new(paths.clone()).unwrap();
        core.set_install(crate::install::resolve(&root, None).unwrap())
            .unwrap();
        assert!(!leftover.exists(), "game temp files swept on set");
        drop(core);

        let core = AppCore::new(paths.clone()).unwrap();
        let active = core.install().expect("saved install restored");
        assert_eq!(active.flavor.id, "_classic_beta_");
        drop(core);

        // The drive goes away: no active install, but the choice is kept.
        std::fs::rename(&root, dir.path().join("moved")).unwrap();
        let core = AppCore::new(paths).unwrap();
        assert_eq!(core.install(), None);
        assert!(core.settings.get().install.is_some());
    }
}
