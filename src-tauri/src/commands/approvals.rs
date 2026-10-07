use tauri::State;

use crate::agent;
use crate::error::AppResult;
use crate::proposals::{self, Approvals, Decision};
use crate::state::{AppCore, AppState};

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Picks up the agent inbox (spec §4): on app start, every 10 seconds while
/// open (`lib.rs`), and whenever Approvals asks. Returns how many proposals
/// were stored, staged or refused.
pub fn ingest_inbox(core: &AppCore) -> AppResult<usize> {
    let dir = agent::Paths::new(&core.paths.config_dir, &core.paths.local_data_dir).dir;
    if proposals::inbox::waiting(&dir).is_empty() {
        return Ok(0);
    }
    let flavor = core.active_game()?.flavor;
    let access = core.settings.get().agent_access;
    proposals::ingest(&core.db, &dir, &flavor, access, now())
}

/// Approvals (IMPLEMENTING §17): what's waiting, and the last 30 days decided.
#[tauri::command(async)]
#[specta::specta]
pub fn approvals_list(state: State<'_, AppState>) -> AppResult<Approvals> {
    let core = &state.core;
    let _ = ingest_inbox(core);
    proposals::list(&core.db, &core.active_game()?.flavor, now())
}

/// The sidebar's count: waiting proposals, or 0 while agent access is off.
#[tauri::command(async)]
#[specta::specta]
pub fn approvals_waiting(state: State<'_, AppState>) -> AppResult<u32> {
    let core = &state.core;
    if !core.settings.get().agent_access {
        return Ok(0);
    }
    let _ = ingest_inbox(core);
    Ok(proposals::waiting_count(&core.db, &core.active_game()?.flavor)? as u32)
}

/// Approve, Decline, or (for a conflicting note) Use proposed. An applied
/// note is sent to the game like one written in the app.
#[tauri::command(async)]
#[specta::specta]
pub fn approvals_decide(state: State<'_, AppState>, id: u32, decision: Decision) -> AppResult<()> {
    let core = &state.core;
    proposals::decide(&core.db, &core.active_game()?.flavor, id, decision, now())?;
    if decision != Decision::Decline {
        let _ = core.send_to_game();
    }
    Ok(())
}
