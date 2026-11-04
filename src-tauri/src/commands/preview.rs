//! The Preview tab (PV1): the player's screen with our addon and their
//! settings, redrawn when a setting changes. See `crate::preview`.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::preview::{Moment, PreviewStatus};
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

/// The last good frame, if there is one, as a `data:` URL for an `<img>`.
#[tauri::command(async)]
#[specta::specta]
pub fn preview_frame(state: State<'_, AppState>) -> AppResult<Option<String>> {
    Ok(state
        .core
        .preview
        .frame()
        .map(|svg| crate::preview::frame_url(&svg)))
}

/// The tab opened, or picked a moment: draw now (that moment, or the one it
/// last picked), and redraw on every change from here on.
#[tauri::command(async)]
#[specta::specta]
pub fn preview_open(state: State<'_, AppState>, moment: Option<Moment>) -> AppResult<()> {
    let core = &state.core;
    core.preview.open(core.preview_inputs()?, moment);
    Ok(())
}

/// The tab picked a UI draft to see, or what's in game (none): draw it, and
/// keep showing it through setting changes until the next pick.
#[tauri::command(async)]
#[specta::specta]
pub fn preview_draft(state: State<'_, AppState>, draft: Option<u32>) -> AppResult<()> {
    let core = &state.core;
    core.preview.set_draft(draft);
    core.preview.open(core.preview_inputs()?, None);
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
