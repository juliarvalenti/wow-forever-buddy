use tauri::State;

use crate::error::AppResult;
use crate::notes::{self, LoginNote, NewNote};
use crate::state::AppState;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// After a change: the Briefing slot is rewritten now if WoW is closed, or
/// after it exits (the ingest that follows sends it). A failure here isn't
/// the note's failure; the app's "Sent to the game" panel reports it.
fn resend(state: &AppState) {
    let _ = state.core.send_to_game();
}

/// Login notes for the active flavor's characters: waiting ones, and the
/// ones shown in the last week.
#[tauri::command(async)]
#[specta::specta]
pub fn notes_list(state: State<'_, AppState>) -> AppResult<Vec<LoginNote>> {
    let core = &state.core;
    notes::list(&core.db, &core.active_game()?.flavor, now())
}

#[tauri::command(async)]
#[specta::specta]
pub fn notes_add(state: State<'_, AppState>, note: NewNote) -> AppResult<u32> {
    let core = &state.core;
    let id = notes::add(&core.db, &core.active_game()?.flavor, &note, "you", now())?;
    resend(&state);
    Ok(id)
}

#[tauri::command(async)]
#[specta::specta]
pub fn notes_delete(state: State<'_, AppState>, id: u32) -> AppResult<()> {
    let core = &state.core;
    notes::delete(&core.db, &core.active_game()?.flavor, id, now())?;
    resend(&state);
    Ok(())
}
