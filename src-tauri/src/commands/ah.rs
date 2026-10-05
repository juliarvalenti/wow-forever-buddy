use tauri::State;

use crate::ah::{self, AhHistory, AhItem, AhStatus, GoodsWorth, Sellable};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// The active flavor, or `None` before a game folder is set (then every
/// list is empty, not an error).
fn flavor(state: &State<'_, AppState>) -> AppResult<Option<String>> {
    match state.core.active_game() {
        Ok(game) => Ok(Some(game.flavor)),
        Err(AppError::NoInstall) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Auctionator's days are the player's local days.
fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

/// The scan bar: whether there are prices, how many, and when Auctionator
/// last scanned.
#[tauri::command(async)]
#[specta::specta]
pub fn ah_status(state: State<'_, AppState>) -> AppResult<AhStatus> {
    match flavor(&state)? {
        Some(f) => ah::status(&state.core.db, &f),
        None => Ok(AhStatus {
            has_prices: false,
            market: None,
            items: 0,
            last_scan_at: None,
            newest_day: None,
        }),
    }
}

/// Priced items whose name contains `query` (or with that item id).
#[tauri::command(async)]
#[specta::specta]
pub fn ah_search(state: State<'_, AppState>, query: String) -> AppResult<Vec<AhItem>> {
    match flavor(&state)? {
        Some(f) => ah::search(&state.core.db, &f, &query, today()),
        None => Ok(Vec::new()),
    }
}

/// One item's price history over the last `days` (all of it with `None`).
#[tauri::command(async)]
#[specta::specta]
pub fn ah_history(
    state: State<'_, AppState>,
    item_id: u32,
    days: Option<u32>,
) -> AppResult<AhHistory> {
    let f = flavor(&state)?.ok_or(AppError::NoInstall)?;
    ah::history(&state.core.db, &f, item_id, days, today())
}

#[tauri::command(async)]
#[specta::specta]
pub fn ah_watchlist(state: State<'_, AppState>) -> AppResult<Vec<AhItem>> {
    match flavor(&state)? {
        Some(f) => ah::watchlist(&state.core.db, &f, today()),
        None => Ok(Vec::new()),
    }
}

/// Adds an item to the watchlist, or removes it. The app's own list: never
/// written to the game.
#[tauri::command(async)]
#[specta::specta]
pub fn ah_set_watched(state: State<'_, AppState>, item_id: u32, watched: bool) -> AppResult<()> {
    let f = flavor(&state)?.ok_or(AppError::NoInstall)?;
    ah::set_watched(&state.core.db, &f, item_id, watched)
}

/// What the alts carry that's worth at least `min_value` copper.
#[tauri::command(async)]
#[specta::specta]
pub fn ah_worth_selling(state: State<'_, AppState>, min_value: f64) -> AppResult<Vec<Sellable>> {
    match flavor(&state)? {
        Some(f) => ah::worth_selling(&state.core.db, &f, min_value, today()),
        None => Ok(Vec::new()),
    }
}

/// Net worth's goods (F5c): what the alts carry, valued at the last scan,
/// priced items only.
#[tauri::command(async)]
#[specta::specta]
pub fn ah_goods_worth(state: State<'_, AppState>) -> AppResult<GoodsWorth> {
    match flavor(&state)? {
        Some(f) => ah::goods_worth(&state.core.db, &f, today()),
        None => Ok(GoodsWorth {
            value: 0.0,
            items: 0,
            priced: 0,
            by_character: Vec::new(),
            top: Vec::new(),
            as_of: None,
        }),
    }
}

/// The last lowest buyout of each item that has one, as `(id, copper)`.
#[tauri::command(async)]
#[specta::specta]
pub fn ah_prices(state: State<'_, AppState>, item_ids: Vec<u32>) -> AppResult<Vec<(u32, f64)>> {
    match flavor(&state)? {
        Some(f) => ah::prices(&state.core.db, &f, &item_ids),
        None => Ok(Vec::new()),
    }
}
