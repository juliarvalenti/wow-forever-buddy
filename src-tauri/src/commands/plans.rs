use tauri::State;

use crate::applog;
use crate::error::{AppError, AppResult};
use crate::plans::{self, Plan};
use crate::state::AppState;

/// The characters' active quest plans (P1). Empty before a game folder is
/// set. Plans become active only through an approved proposal (P2).
#[tauri::command(async)]
#[specta::specta]
pub fn plans_list(state: State<'_, AppState>) -> AppResult<Vec<Plan>> {
    match state.core.active_game() {
        Ok(game) => plans::active(&state.core.db, &game.flavor),
        Err(AppError::NoInstall) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// "Clear plan": the character has no plan any more, in the app now and in
/// the game at the next slot write.
#[tauri::command(async)]
#[specta::specta]
pub fn plan_clear(state: State<'_, AppState>, character_id: u32) -> AppResult<Vec<Plan>> {
    let core = &state.core;
    plans::clear_plan(&core.db, character_id)?;
    if let Err(e) = core.send_to_game() {
        applog::append(
            &core.paths.log_dir,
            &format!("sending to the game failed: {e}"),
        );
    }
    plans_list(state)
}
