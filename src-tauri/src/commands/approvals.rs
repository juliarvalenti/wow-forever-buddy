use tauri::State;

use crate::agent;
use crate::error::AppResult;
use crate::proposals::draft::UiDraft;
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
    let game = core.active_game()?;
    let access = core.settings.get().agent_access;
    // A macro proposal is checked against the player's macros (MC1).
    proposals::ingest_in(
        &core.db,
        &dir,
        &game.flavor,
        Some(&game.root.base),
        access,
        now(),
    )
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

/// ED1: the UI drafts waiting for the Your UI tab, newest first. Its "Use
/// this" and "Throw away" are `approvals_decide`'s Approve and Decline.
#[tauri::command(async)]
#[specta::specta]
pub fn ui_drafts_list(state: State<'_, AppState>) -> AppResult<Vec<UiDraft>> {
    let core = &state.core;
    let _ = ingest_inbox(core);
    proposals::drafts(&core.db, &core.active_game()?.flavor)
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

/// UI1: a proposed UI document drawn on the player's screen at `moment`,
/// as a PNG `data:` URL for its Approvals card (the same render as
/// render_ui). Needs the preview data the Your UI tab downloads.
#[tauri::command(async)]
#[specta::specta]
pub fn ui_doc_picture(
    state: State<'_, AppState>,
    id: u32,
    moment: crate::preview::Moment,
) -> AppResult<String> {
    use base64::Engine;
    let core = &state.core;
    let doc = proposals::uidoc::doc(&core.db, &core.active_game()?.flavor, id)?
        .ok_or_else(|| crate::error::AppError::NotFound("that UI".into()))?;
    let scratch = tempfile::tempdir().map_err(|e| crate::error::AppError::Io(e.to_string()))?;
    let data = crate::preview::ui::data_dir(&core.paths.local_data_dir);
    let outcome = crate::preview::ui::render(&crate::preview::ui::Request {
        data: &data,
        scratch: scratch.path(),
        doc: &doc,
        moment,
        class: None,
        scale: 0.5,
    })
    .map_err(crate::error::AppError::NotFound)?;
    match outcome {
        crate::preview::ui::Outcome::Drawn(r) => Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(r.png)
        )),
        crate::preview::ui::Outcome::Refused(p) => Err(crate::error::AppError::InvalidSettings(
            crate::preview::ui::problem_lines(&p),
        )),
    }
}

/// UI1: the UI document in use, if any (for "Go back to the standard HUD").
#[tauri::command(async)]
#[specta::specta]
pub fn ui_doc_active(state: State<'_, AppState>) -> AppResult<Option<proposals::uidoc::ActiveUi>> {
    let core = &state.core;
    proposals::uidoc::active_ui(&core.db, &core.active_game()?.flavor)
}

/// UI1: "Go back to the standard HUD": no document in use for anyone, sent
/// to the game.
#[tauri::command(async)]
#[specta::specta]
pub fn ui_doc_go_back(state: State<'_, AppState>) -> AppResult<()> {
    let core = &state.core;
    proposals::uidoc::go_back(&core.db, &core.active_game()?.flavor, None)?;
    let _ = core.send_to_game();
    Ok(())
}

/// P4b: "Keep the new settings or go back?", asked once after the game has
/// applied them. Going back sends the old settings to the game.
#[tauri::command(async)]
#[specta::specta]
pub fn approvals_keep_settings(state: State<'_, AppState>, keep: bool) -> AppResult<()> {
    let core = &state.core;
    crate::addon_settings::decide(&core.db, &core.active_game()?.flavor, keep, now())?;
    if !keep {
        let _ = core.send_to_game();
    }
    Ok(())
}
