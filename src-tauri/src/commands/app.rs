use serde::Serialize;
use tauri::State;

use crate::error::AppResult;
use crate::state::{AppPaths, AppState};

#[derive(Debug, Serialize, specta::Type)]
pub struct AppInfo {
    pub version: String,
    pub paths: AppPaths,
}

/// App version and data locations, for the Settings/about panel and bug reports.
#[tauri::command]
#[specta::specta]
pub fn app_info(state: State<'_, AppState>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        paths: state.core.paths.clone(),
    })
}
