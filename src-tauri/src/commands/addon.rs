use tauri::State;

use crate::addon::{self, AddonStatus};
use crate::error::AppResult;
use crate::state::AppState;

/// Whether ForeverBuddy is installed in the active game folder, its version
/// against the one this app carries, and which characters have it on.
#[tauri::command(async)]
#[specta::specta]
pub fn addon_status(state: State<'_, AppState>) -> AppResult<AddonStatus> {
    addon::status(&state.core.active_game()?.root)
}

/// Installs or updates the addon through the write gate: refused while WoW
/// runs, with a safety snapshot first. Runs as the one backup/restore job.
#[tauri::command(async)]
#[specta::specta]
pub fn addon_install(state: State<'_, AppState>) -> AppResult<AddonStatus> {
    let core = &state.core;
    let _job = core.jobs.lock().expect("job lock poisoned");
    let target = core.mutation_target()?;
    addon::install(&core.write_gate()?, &target)?;
    addon::status(&target.game)
}

/// Removes the addon's files (and its folder, if nothing else is in it).
/// The SavedVariables stay: they're the player's data.
#[tauri::command(async)]
#[specta::specta]
pub fn addon_remove(state: State<'_, AppState>) -> AppResult<AddonStatus> {
    let core = &state.core;
    let _job = core.jobs.lock().expect("job lock poisoned");
    let target = core.mutation_target()?;
    addon::remove(&core.write_gate()?, &target)?;
    addon::status(&target.game)
}
