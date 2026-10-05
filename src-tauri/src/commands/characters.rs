use tauri::State;

use crate::characters::{self, CharacterSheet, CharactersOverview, SearchResults};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// The Characters screen: totals and one card per character of the active
/// game flavor. Empty (not an error) before a game folder is set.
#[tauri::command(async)]
#[specta::specta]
pub fn characters_overview(state: State<'_, AppState>) -> AppResult<CharactersOverview> {
    match state.core.active_game() {
        Ok(game) => characters::overview(&state.core.db, &game.flavor),
        Err(AppError::NoInstall) => Ok(CharactersOverview {
            gold: 0.0,
            items: 0,
            characters: Vec::new(),
        }),
        Err(e) => Err(e),
    }
}

/// The Characters search box: every satchel, bank and mailbox of the active
/// flavor's characters, searched by item name words and `ilvl>60`-style
/// filters. Nothing before a game folder is set.
#[tauri::command(async)]
#[specta::specta]
pub fn characters_search(state: State<'_, AppState>, query: String) -> AppResult<SearchResults> {
    // Typed text, so long only by mistake; a cap keeps the matching cheap.
    let query: String = query.chars().take(200).collect();
    match state.core.active_game() {
        Ok(game) => characters::search(&state.core.db, &game.flavor, &query),
        Err(AppError::NoInstall) => Ok(SearchResults {
            hits: Vec::new(),
            total: 0,
            characters: Vec::new(),
            more: false,
        }),
        Err(e) => Err(e),
    }
}

/// One character's sheet: gear, satchels, bank, mail, professions, 30-day gold.
#[tauri::command(async)]
#[specta::specta]
pub fn character_detail(state: State<'_, AppState>, id: u32) -> AppResult<CharacterSheet> {
    characters::sheet(&state.core.db, id)
}
