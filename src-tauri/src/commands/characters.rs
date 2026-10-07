use tauri::State;

use crate::characters::{self, AltLockout, CharacterSheet, CharactersOverview, SearchResults};
use crate::error::{AppError, AppResult};
use crate::quests::{self, QuestLog};
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

/// One character's quest log (Q1b): completed count and recent accepts and
/// turn-ins. The sheet shows it only when it has something in it.
#[tauri::command(async)]
#[specta::specta]
pub fn character_quests(state: State<'_, AppState>, id: u32) -> AppResult<QuestLog> {
    quests::log(&state.core.db, id)
}

/// Whether any character has quest data yet; until then the sheet's Quests
/// tab stays hidden. False before a game folder is set.
#[tauri::command(async)]
#[specta::specta]
pub fn quests_available(state: State<'_, AppState>) -> AppResult<bool> {
    match state.core.active_game() {
        Ok(game) => quests::any(&state.core.db, &game.flavor),
        Err(AppError::NoInstall) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Raid and dungeon saves across the active flavor's characters that haven't
/// reset yet, soonest first (the Dashboard's "Lockouts this week"). Empty
/// before a game folder is set.
#[tauri::command(async)]
#[specta::specta]
pub fn lockouts_list(state: State<'_, AppState>) -> AppResult<Vec<AltLockout>> {
    match state.core.active_game() {
        Ok(game) => {
            characters::lockouts(&state.core.db, &game.flavor, chrono::Utc::now().timestamp())
        }
        Err(AppError::NoInstall) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Marks or unmarks a character as a bank alt (its card's "Bank" tag).
#[tauri::command(async)]
#[specta::specta]
pub fn character_set_bank_alt(
    state: State<'_, AppState>,
    id: u32,
    bank_alt: bool,
) -> AppResult<()> {
    characters::set_bank_alt(&state.core.db, id, bank_alt)
}
