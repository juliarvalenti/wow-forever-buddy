use tauri::State;

use crate::adventures::{self, Adventure};
use crate::error::AppResult;
use crate::state::AppState;

/// One adventure's recap, or the newest one when `id` is `None`. `None`
/// back means there are no adventures yet (or none with that id).
#[tauri::command(async)]
#[specta::specta]
pub fn adventure_get(state: State<'_, AppState>, id: Option<u32>) -> AppResult<Option<Adventure>> {
    let core = &state.core;
    let flavor = core.active_game()?.flavor;
    let id = match id {
        Some(id) => Some(id),
        None => adventures::latest(&core.db, &flavor)?,
    };
    match id {
        Some(id) => adventures::adventure(&core.db, &flavor, id),
        None => Ok(None),
    }
}

/// Saves the user's note on an adventure (blank clears it). Only the app's
/// own database changes; nothing is written to the game folder.
#[tauri::command(async)]
#[specta::specta]
pub fn adventure_set_note(state: State<'_, AppState>, id: u32, note: String) -> AppResult<()> {
    let core = &state.core;
    let flavor = core.active_game()?.flavor;
    adventures::set_note(&core.db, &flavor, id, &note)
}
