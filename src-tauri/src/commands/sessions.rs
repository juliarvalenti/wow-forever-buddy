use tauri::State;

use crate::error::AppResult;
use crate::sessions::{self, PlaySession, WtfCharacter};
use crate::state::AppState;

/// Play sessions of the active flavor from the last `days` days, newest
/// first. The one with no `ended_at` is in progress. Follow
/// `sessions-changed` for updates.
#[tauri::command(async)]
#[specta::specta]
pub fn sessions_list(state: State<'_, AppState>, days: u32) -> AppResult<Vec<PlaySession>> {
    let core = &state.core;
    let game = core.active_game()?;
    let since = chrono::Utc::now() - chrono::Duration::days(i64::from(days));
    sessions::list(&core.db, &game.flavor, &since.to_rfc3339())
}

/// The characters in the active flavor's WTF folder, most recently played
/// first. Names only: class, level and gold need the addon (v0.2).
#[tauri::command(async)]
#[specta::specta]
pub fn characters_list(state: State<'_, AppState>) -> AppResult<Vec<WtfCharacter>> {
    let game = state.core.active_game()?;
    Ok(sessions::wtf_characters(&game.root.base.join("WTF")))
}
