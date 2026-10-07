use tauri::State;

use crate::addon;
use crate::applog;
use crate::bridge::{self, Slot};
use crate::cleanup::{self, Cleanup, Mark};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// The sheet's Bag cleanup panel (B3) for one character, with the Cleanup
/// slot's way to the game.
#[tauri::command(async)]
#[specta::specta]
pub fn cleanup_get(state: State<'_, AppState>, character_id: u32) -> AppResult<Cleanup> {
    let core = &state.core;
    let mut view = cleanup::view(&core.db, character_id)?;
    match core.active_game() {
        Ok(game) => {
            let listed = addon::listed_slots(&game.root).contains(&Slot::Cleanup);
            let changed = cleanup::changed_at(&core.db, &game.flavor)?;
            view.delivery = bridge::delivery(
                &core.db,
                &game.flavor,
                Slot::Cleanup,
                Some(character_id),
                &changed,
                listed,
            )?;
        }
        Err(AppError::NoInstall) => {}
        Err(e) => return Err(e),
    }
    Ok(view)
}

/// After a change: out to the game (now, or once WoW closes), then the
/// fresh panel.
fn changed(state: State<'_, AppState>, character_id: u32) -> AppResult<Cleanup> {
    let core = &state.core;
    if let Err(e) = core.send_to_game() {
        applog::append(
            &core.paths.log_dir,
            &format!("sending to the game failed: {e}"),
        );
    }
    cleanup_get(state, character_id)
}

#[tauri::command(async)]
#[specta::specta]
pub fn cleanup_mark(
    state: State<'_, AppState>,
    character_id: u32,
    item_id: u32,
    mark: Mark,
) -> AppResult<Cleanup> {
    cleanup::mark(&state.core.db, character_id, item_id, mark)?;
    changed(state, character_id)
}

#[tauri::command(async)]
#[specta::specta]
pub fn cleanup_clear(
    state: State<'_, AppState>,
    character_id: u32,
    item_id: u32,
) -> AppResult<Cleanup> {
    cleanup::clear(&state.core.db, character_id, item_id)?;
    changed(state, character_id)
}

/// B3b: mark a suggestion with its reason, or (`item_id` None) "Mark all".
#[tauri::command(async)]
#[specta::specta]
pub fn cleanup_accept(
    state: State<'_, AppState>,
    character_id: u32,
    item_id: Option<u32>,
) -> AppResult<Cleanup> {
    cleanup::accept(&state.core.db, character_id, item_id)?;
    changed(state, character_id)
}

/// B3b: dismiss a suggestion; it doesn't come back for that item.
#[tauri::command(async)]
#[specta::specta]
pub fn cleanup_dismiss(
    state: State<'_, AppState>,
    character_id: u32,
    item_id: u32,
) -> AppResult<Cleanup> {
    cleanup::dismiss(&state.core.db, character_id, item_id)?;
    cleanup_get(state, character_id)
}
