use tauri::State;

use crate::error::{AppError, AppResult};
use crate::icons::{self, IconCacheStatus, IconFill};
use crate::state::AppState;

/// Settings > Game data cache: how many icons are cached, their size, and
/// the build they came from.
#[tauri::command(async)]
#[specta::specta]
pub fn icons_cache_status(state: State<'_, AppState>) -> AppResult<IconCacheStatus> {
    let core = &state.core;
    Ok(core.icons.status(core.flavor_dir().as_deref()))
}

/// "Rebuild": empties the cache and reads every known item's icon again.
#[tauri::command(async)]
#[specta::specta]
pub fn icons_cache_rebuild(state: State<'_, AppState>) -> AppResult<IconFill> {
    let core = &state.core;
    let flavor = core.flavor_dir().ok_or(AppError::NoInstall)?;
    let ids = icons::known_ids(&core.db)?;
    Ok(core.icons.rebuild(flavor, ids)?)
}

/// "Clear": empties the cache. Icons are read again as they're shown.
#[tauri::command(async)]
#[specta::specta]
pub fn icons_cache_clear(state: State<'_, AppState>) -> AppResult<()> {
    Ok(state.core.icons.clear()?)
}
