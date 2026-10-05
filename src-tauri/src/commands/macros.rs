use tauri::State;

use crate::error::AppResult;
use crate::install;
use crate::macros::{self, MacrosList};
use crate::state::AppState;

/// The Macros screen (F7): every account's and character's macros in the
/// active flavor, read-only. `null` before a game folder is set.
#[tauri::command(async)]
#[specta::specta]
pub fn macros_list(state: State<'_, AppState>) -> AppResult<Option<MacrosList>> {
    Ok(install::current(&state.core.settings)?.and_then(|i| i.active_flavor().map(macros::list)))
}
