use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::error::{AppError, AppResult};

/// Where the app keeps its own files (spec §6). Never inside the game folder.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AppPaths {
    /// settings.json
    pub config_dir: PathBuf,
    /// buddy.db and the default backups/ location (%LOCALAPPDATA% on Windows)
    pub local_data_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve<R: Runtime>(app: &AppHandle<R>) -> AppResult<Self> {
        let path = app.path();
        let resolve_err = |e: tauri::Error| AppError::Io(e.to_string());
        Ok(Self {
            config_dir: path.app_config_dir().map_err(resolve_err)?,
            local_data_dir: path.app_local_data_dir().map_err(resolve_err)?,
            log_dir: path.app_log_dir().map_err(resolve_err)?,
        })
    }
}

/// Everything the app does, minus Tauri. Integration tests build this directly
/// against temp dirs; later tickets add settings, db, install, game status, jobs.
#[derive(Debug)]
pub struct AppCore {
    pub paths: AppPaths,
}

impl AppCore {
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        for dir in [&paths.config_dir, &paths.local_data_dir, &paths.log_dir] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(Self { paths })
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
    fn core_creates_its_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths {
            config_dir: tmp.path().join("config"),
            local_data_dir: tmp.path().join("local"),
            log_dir: tmp.path().join("logs"),
        };
        let core = AppCore::new(paths).unwrap();
        assert!(core.paths.config_dir.is_dir());
        assert!(core.paths.local_data_dir.is_dir());
        assert!(core.paths.log_dir.is_dir());
    }
}
