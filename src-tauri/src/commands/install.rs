use std::path::PathBuf;

use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::error::AppResult;
use crate::install::{self, detect::DetectReport, layout::Install, InstallChanged};
use crate::state::AppState;

// These scan folders (and on Windows, every drive), so they run off the main
// thread via `command(async)`.

/// Every WoW install found (the saved one first, then registry and common
/// paths, each with its source), plus every place we looked.
#[tauri::command(async)]
#[specta::specta]
pub fn install_detect(state: State<'_, AppState>) -> AppResult<DetectReport> {
    Ok(install::detect_all(&state.core.settings))
}

/// The active install, re-validated. Fails with `InvalidInstall` if the saved
/// folder is gone, and is `null` if none has been chosen.
#[tauri::command(async)]
#[specta::specta]
pub fn install_get(state: State<'_, AppState>) -> AppResult<Option<Install>> {
    install::current(&state.core.settings)
}

/// Sets the install from a picked folder (root, flavor folder or WTF) and
/// optionally a flavor id. Validates, normalizes and saves.
#[tauri::command(async)]
#[specta::specta]
pub fn install_set(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
    flavor: Option<String>,
) -> AppResult<Install> {
    let install = install::set(&state.core.settings, &path, flavor.as_deref())?;
    let _ = InstallChanged {
        install: Some(install.clone()),
    }
    .emit(&app);
    Ok(install)
}
