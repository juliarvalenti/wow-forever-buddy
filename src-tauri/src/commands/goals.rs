use tauri::State;

use crate::error::AppResult;
use crate::goals::{self, Goal, NewGoal};
use crate::state::AppState;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// G1: open goals, and those reached in the last 3 days.
#[tauri::command(async)]
#[specta::specta]
pub fn goals_list(state: State<'_, AppState>) -> AppResult<Vec<Goal>> {
    let core = &state.core;
    goals::list(&core.db, &core.active_game()?.flavor, now())
}

/// A goal set in the app. The briefing slot carries goals, so it's resent.
#[tauri::command(async)]
#[specta::specta]
pub fn goals_add(state: State<'_, AppState>, goal: NewGoal) -> AppResult<u32> {
    let core = &state.core;
    let id = goals::add(&core.db, &core.active_game()?.flavor, &goal, "app", now())?;
    let _ = core.send_to_game();
    Ok(id)
}

#[tauri::command(async)]
#[specta::specta]
pub fn goals_delete(state: State<'_, AppState>, id: u32) -> AppResult<()> {
    let core = &state.core;
    goals::delete(&core.db, &core.active_game()?.flavor, id, now())?;
    let _ = core.send_to_game();
    Ok(())
}
