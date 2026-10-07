use tauri::State;

use crate::addon;
use crate::applog;
use crate::bridge::{self, Slot};
use crate::error::{AppError, AppResult};
use crate::plans::{self, Plan};
use crate::state::AppState;

/// The characters' active quest plans (P1). Empty before a game folder is
/// set. Plans become active only through an approved proposal (P2).
#[tauri::command(async)]
#[specta::specta]
pub fn plans_list(state: State<'_, AppState>) -> AppResult<Vec<Plan>> {
    let core = &state.core;
    let game = match core.active_game() {
        Ok(game) => game,
        Err(AppError::NoInstall) => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let listed = addon::listed_slots(&game.root).contains(&Slot::Plan);
    let mut plans = plans::active(&core.db, &game.flavor)?;
    for p in &mut plans {
        p.delivery = bridge::delivery(
            &core.db,
            &game.flavor,
            Slot::Plan,
            p.character_id,
            &p.created_at,
            listed,
        )?;
    }
    Ok(plans)
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
