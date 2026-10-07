use serde::Serialize;
use tauri::State;

use crate::addon;
use crate::ah;
use crate::applog;
use crate::bridge::{self, Delivery, Slot};
use crate::error::{AppError, AppResult};
use crate::lists::{self, List, NewItem, SeenItem};
use crate::notes;
use crate::state::AppState;

/// The Lists screen (B2): every list, and where they are on the way to the
/// game.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ListsView {
    pub lists: Vec<List>,
    pub delivery: Delivery,
    /// The login notes' way to the game (B1's Briefing slot), for the
    /// screen's second "Sent to the game" row.
    pub briefing: Delivery,
    /// When the AH was last scanned (RFC 3339), for the prices' age.
    pub scan_at: Option<String>,
}

#[tauri::command(async)]
#[specta::specta]
pub fn lists_get(state: State<'_, AppState>) -> AppResult<ListsView> {
    let core = &state.core;
    let game = match core.active_game() {
        Ok(game) => game,
        Err(AppError::NoInstall) => {
            return Ok(ListsView {
                lists: Vec::new(),
                delivery: Delivery::Waiting,
                briefing: Delivery::Waiting,
                scan_at: None,
            })
        }
        Err(e) => return Err(e),
    };
    let listed = addon::listed_slots(&game.root);
    let changed = lists::changed_at(&core.db, &game.flavor)?.unwrap_or_default();
    let notes_changed = notes::changed_at(&core.db, &game.flavor)?;
    Ok(ListsView {
        lists: lists::lists(&core.db, &game.flavor)?,
        delivery: bridge::delivery(
            &core.db,
            &game.flavor,
            Slot::Lists,
            None,
            &changed,
            listed.contains(&Slot::Lists),
        )?,
        briefing: bridge::delivery(
            &core.db,
            &game.flavor,
            Slot::Briefing,
            None,
            &notes_changed,
            listed.contains(&Slot::Briefing),
        )?,
        scan_at: ah::status(&core.db, &game.flavor)?.last_scan_at,
    })
}

/// After a change: out to the game (now, or once WoW closes), then the
/// fresh view.
fn changed(state: State<'_, AppState>) -> AppResult<ListsView> {
    let core = &state.core;
    if let Err(e) = core.send_to_game() {
        applog::append(
            &core.paths.log_dir,
            &format!("sending to the game failed: {e}"),
        );
    }
    lists_get(state)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_create(
    state: State<'_, AppState>,
    name: String,
    for_character: Option<u32>,
) -> AppResult<ListsView> {
    let game = state.core.active_game()?;
    lists::create_list(&state.core.db, &game.flavor, &name, for_character, "app")?;
    changed(state)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_update(
    state: State<'_, AppState>,
    id: u32,
    name: String,
    for_character: Option<u32>,
) -> AppResult<ListsView> {
    lists::update_list(&state.core.db, id, &name, for_character)?;
    changed(state)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_delete(state: State<'_, AppState>, id: u32) -> AppResult<ListsView> {
    lists::delete_list(&state.core.db, id)?;
    changed(state)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_item_add(
    state: State<'_, AppState>,
    list_id: u32,
    item: NewItem,
    need: u32,
) -> AppResult<ListsView> {
    lists::add_item(&state.core.db, list_id, &item, need)?;
    changed(state)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_item_need(state: State<'_, AppState>, id: u32, need: u32) -> AppResult<ListsView> {
    lists::set_need(&state.core.db, id, need)?;
    changed(state)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_item_remove(state: State<'_, AppState>, id: u32) -> AppResult<ListsView> {
    lists::remove_item(&state.core.db, id)?;
    changed(state)
}

/// "+ Add an item…": items your characters have seen.
#[tauri::command(async)]
#[specta::specta]
pub fn items_seen_search(state: State<'_, AppState>, query: String) -> AppResult<Vec<SeenItem>> {
    lists::search_seen(&state.core.db, &query)
}
