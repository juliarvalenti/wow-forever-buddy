use crate::config::paths::AppPaths;
use crate::config::settings::SettingsStore;
use crate::error::AppResult;

/// Everything the app does, minus Tauri. Integration tests build this directly
/// against temp dirs; later tickets add db, install, game status, jobs.
pub struct AppCore {
    pub paths: AppPaths,
    pub settings: SettingsStore,
}

impl AppCore {
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        for dir in [&paths.config_dir, &paths.local_data_dir, &paths.log_dir] {
            std::fs::create_dir_all(dir)?;
        }
        let settings = SettingsStore::load(paths.settings_file())?;
        Ok(Self { paths, settings })
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
    }
}
