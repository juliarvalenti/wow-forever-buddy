use tauri::State;

use crate::error::{AppError, AppResult};
use crate::ingest::{self, IngestProblem};
use crate::state::AppState;

/// Characters whose ForeverBuddy notes couldn't be read last time, for the
/// Dashboard's "Couldn't read Thrandor's notes · will retry". They're retried
/// when the file changes. Only the active flavor's; none before a game
/// folder is set.
#[tauri::command(async)]
#[specta::specta]
pub fn ingest_problems(state: State<'_, AppState>) -> AppResult<Vec<IngestProblem>> {
    match state.core.active_game() {
        Ok(game) => ingest::problems(&state.core.db, &game.flavor),
        Err(AppError::NoInstall) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}
