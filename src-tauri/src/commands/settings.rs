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
/// commands, which validate it. Nor is the backup location, which moves the
/// backups (`backup_move_location`).
/// Runs off the main thread: validation resolves the backup location, which
/// can stall on an offline network share.
#[tauri::command(async)]
#[specta::specta]
pub fn settings_update(state: State<'_, AppState>, patch: SettingsPatch) -> AppResult<Settings> {
    state.core.update_settings(patch)
}
