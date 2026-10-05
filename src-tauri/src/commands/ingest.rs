use tauri::State;

use crate::error::AppResult;
use crate::ingest::{self, IngestProblem};
use crate::state::AppState;

/// Characters whose ForeverBuddy notes couldn't be read last time, for the
/// Dashboard's "Couldn't read Thrandor's notes · will retry". They're retried
/// when the file changes.
#[tauri::command(async)]
#[specta::specta]
pub fn ingest_problems(state: State<'_, AppState>) -> AppResult<Vec<IngestProblem>> {
    ingest::problems(&state.core.db)
}
