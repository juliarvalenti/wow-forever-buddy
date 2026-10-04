use crate::config::paths::AppPaths;
use crate::config::settings::SettingsStore;
use crate::db::Db;
use crate::error::AppResult;
use crate::fsx::atomic::sweep_temp_files;

/// Everything the app does, minus Tauri. Integration tests build this directly
/// against temp dirs; later tickets add install, game status, jobs.
pub struct AppCore {
    pub paths: AppPaths,
    pub settings: SettingsStore,
    #[allow(dead_code)] // first read by the backup store (T7)
    pub db: Db,
}

impl AppCore {
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        for dir in [&paths.config_dir, &paths.local_data_dir, &paths.log_dir] {
            std::fs::create_dir_all(dir)?;
        }
        // Leftovers from a crash mid-write (spec §8 startup step 3). The game
        // folder is swept once the install is resolved.
        sweep_temp_files(&paths.config_dir)?;
        sweep_temp_files(&paths.local_data_dir)?;

        let settings = SettingsStore::load(paths.settings_file())?;
        let db = Db::open(&paths.db_file())?;
        Ok(Self {
            paths,
            settings,
            db,
        })
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
        let core = AppCore::new(AppPaths::under(tmp.path())).unwrap();
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
