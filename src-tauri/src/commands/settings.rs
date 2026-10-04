use tauri::State;

use crate::config::settings::{Settings, SettingsPatch};
use crate::error::AppResult;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn settings_get(state: State<'_, AppState>) -> Settings {
    state.core.settings.get()
}

/// Changes only the fields present in `patch` and returns the new settings.
/// The game folder isn't part of it: that goes through the install
/// commands, which validate it.
#[tauri::command]
#[specta::specta]
pub fn settings_update(state: State<'_, AppState>, patch: SettingsPatch) -> AppResult<Settings> {
    state.core.settings.apply_patch(patch)
}
