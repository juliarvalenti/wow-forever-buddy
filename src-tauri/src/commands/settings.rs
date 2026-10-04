use tauri::State;

use crate::config::settings::Settings;
use crate::error::AppResult;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn settings_get(state: State<'_, AppState>) -> Settings {
    state.core.settings.get()
}

/// Saves a full settings object (typically `settings_get()` with edits).
/// `schema_version` and `install` are backend-owned and ignored here; the game
/// folder is changed through the install commands, which validate it.
#[tauri::command]
#[specta::specta]
pub fn settings_update(state: State<'_, AppState>, settings: Settings) -> AppResult<Settings> {
    state.core.settings.update_from_user(settings)
}
