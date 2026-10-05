use tauri::State;

use crate::addons::{self, AddonsList, CharacterKey, ToggleResult};
use crate::error::{AppError, AppResult};
use crate::install;
use crate::state::AppState;

/// The Addons screen (F4): every addon in the active flavor's
/// `Interface/AddOns`, read-only, with each character's on/off. `null`
/// before a game folder is set.
#[tauri::command(async)]
#[specta::specta]
pub fn addons_list(state: State<'_, AppState>) -> AppResult<Option<AddonsList>> {
    Ok(install::current(&state.core.settings)?.and_then(|i| i.active_flavor().map(addons::list)))
}

/// Turns `addon` on or off for `characters` (F6) by rewriting their
/// AddOns.txt through the write gate: refused while WoW runs, with a safety
/// snapshot first. The addon and characters are checked against what the
/// list shows; nothing else can be named. Runs as the one backup/restore job.
#[tauri::command(async)]
#[specta::specta]
pub fn addons_set_enabled(
    state: State<'_, AppState>,
    addon: String,
    characters: Vec<CharacterKey>,
    enabled: bool,
) -> AppResult<ToggleResult> {
    let core = &state.core;
    let _job = core.jobs.lock().expect("job lock poisoned");
    let install = install::current(&core.settings)?.ok_or(AppError::NoInstall)?;
    let flavor = install.active_flavor().ok_or(AppError::NoInstall)?;
    let target = core.mutation_target()?;
    addons::set_enabled(
        &core.write_gate()?,
        &target,
        flavor,
        &addon,
        &characters,
        enabled,
    )
}

/// Undoes a toggle from its safety snapshot (`ToggleResult::snapshot_id`):
/// only AddOns.txt files are put back, through the write gate.
#[tauri::command(async)]
#[specta::specta]
pub fn addons_undo(state: State<'_, AppState>, snapshot_id: String) -> AppResult<()> {
    let core = &state.core;
    let _job = core.jobs.lock().expect("job lock poisoned");
    let store = core.backups()?;
    let manifest = store.manifest(&snapshot_id)?;
    addons::undo(
        &core.write_gate()?,
        &core.mutation_target()?,
        &manifest,
        |hash| store.blobs().get(hash),
    )
}
