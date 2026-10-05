use tauri::State;

use crate::addons::{self, AddonsList};
use crate::error::AppResult;
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
