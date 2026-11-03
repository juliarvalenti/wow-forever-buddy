//! The Preview tab (PV1): the player's screen with our addon and their
//! settings, redrawn when a setting changes. See `crate::preview`.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::preview::PreviewStatus;
use crate::state::AppState;

/// The preview's state changed (a new frame, a draw started or failed, the
/// download moved on): the tab asks for the status, and the frame if new.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct PreviewChanged;

#[tauri::command(async)]
#[specta::specta]
pub fn preview_status(state: State<'_, AppState>) -> AppResult<PreviewStatus> {
    Ok(state.core.preview.status())
}

/// The last good frame as SVG, if there is one.
#[tauri::command(async)]
#[specta::specta]
pub fn preview_frame(state: State<'_, AppState>) -> AppResult<Option<String>> {
    Ok(state.core.preview.frame())
}

/// The tab opened: draw now, and redraw on every change from here on.
#[tauri::command(async)]
#[specta::specta]
pub fn preview_open(state: State<'_, AppState>) -> AppResult<()> {
    let core = &state.core;
    core.preview.open(core.preview_inputs()?);
    Ok(())
}

/// "Download preview data": WOWAPI's data files from GitHub, checked against
/// the hashes the app carries. Progress arrives as `PreviewChanged`.
#[tauri::command(async)]
#[specta::specta]
pub fn preview_download(state: State<'_, AppState>) -> AppResult<()> {
    state.core.preview.download();
    Ok(())
}

/// Settings > "Remove preview data".
#[tauri::command(async)]
#[specta::specta]
pub fn preview_remove_data(state: State<'_, AppState>) -> AppResult<()> {
    state
        .core
        .preview
        .remove_data()
        .map_err(|e| AppError::Io(e.to_string()))
}
