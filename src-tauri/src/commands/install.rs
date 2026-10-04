use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tauri_specta::Event;

use crate::error::{AppError, AppResult};
use crate::install::detect::{detect, system_sources, InstallCandidate};
use crate::install::layout::ActiveInstall;
use crate::state::AppState;

/// Emitted whenever the active install changes.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct InstallChanged(pub Option<ActiveInstall>);

/// Looks for WoW installs on this machine (registry, common paths), for onboarding.
#[tauri::command]
#[specta::specta]
pub async fn install_detect() -> AppResult<Vec<InstallCandidate>> {
    tauri::async_runtime::spawn_blocking(|| detect(&system_sources()))
        .await
        .map_err(|e| AppError::Io(format!("detection failed: {e}")))
}

/// The install the app is working with, or `None` if none is set or the saved
/// folder can't be found right now.
#[tauri::command]
#[specta::specta]
pub fn install_get(state: State<'_, AppState>) -> Option<ActiveInstall> {
    state.core.install()
}

/// Validates and saves the game folder. `path` may be the WoW root, a flavor
/// folder or a WTF folder (from the folder picker or a detected candidate);
/// `flavor` picks the game folder when the root has several.
#[tauri::command]
#[specta::specta]
pub fn install_set(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
    flavor: Option<String>,
) -> AppResult<ActiveInstall> {
    if !path.is_absolute() {
        return Err(AppError::InvalidInstall(format!(
            "not an absolute path: {}",
            path.display()
        )));
    }
    let active = crate::install::resolve(&path, flavor.as_deref())?;
    state.core.set_install(active.clone())?;
    InstallChanged(Some(active.clone()))
        .emit(&app)
        .map_err(|e| AppError::Io(format!("emit install changed: {e}")))?;
    Ok(active)
}

/// The folders the UI may reveal in Explorer/Finder. A fixed set, so the
/// frontend never gets to open arbitrary paths.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
pub enum FolderTarget {
    Backups,
    Logs,
    AppData,
    GameFolder,
}

#[tauri::command]
#[specta::specta]
pub fn app_open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    which: FolderTarget,
) -> AppResult<()> {
    let core = &state.core;
    let path = match which {
        FolderTarget::Backups => core.backups_dir(),
        FolderTarget::Logs => core.paths.log_dir.clone(),
        FolderTarget::AppData => core.paths.local_data_dir.clone(),
        FolderTarget::GameFolder => core.install().ok_or(AppError::NoInstall)?.flavor.dir,
    };
    std::fs::create_dir_all(&path)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Io(format!("open {}: {e}", path.display())))
}
