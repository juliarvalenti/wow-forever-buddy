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

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn db_file(&self) -> PathBuf {
        self.local_data_dir.join("buddy.db")
    }

    /// All three dirs under one root. For tests.
    #[cfg(test)]
    pub fn under(root: &std::path::Path) -> Self {
        Self {
            config_dir: root.join("config"),
            local_data_dir: root.join("local"),
            log_dir: root.join("logs"),
        }
    }
}
