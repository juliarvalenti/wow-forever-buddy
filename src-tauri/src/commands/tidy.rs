use std::path::PathBuf;

use chrono::Utc;
use tauri::State;

use crate::applog;
use crate::error::{AppError, AppResult};
use crate::state::{AppCore, AppState};
use crate::tidy::{self, DataSize, ForgetPreview, Tidy};

/// The active flavor and its folder, checked like every WTF read.
fn game(core: &AppCore) -> AppResult<(String, PathBuf)> {
    let game = core.active_game()?;
    let dir = core.flavor_dir().ok_or(AppError::NoInstall)?;
    Ok((game.flavor, dir))
}

/// Hidden totals and tooltips change with these: out to the game (now, or
/// once WoW closes).
fn send(core: &AppCore) {
    if let Err(e) = core.send_to_game() {
        applog::append(
            &core.paths.log_dir,
            &format!("sending to the game failed: {e}"),
        );
    }
}

/// O2: gone, hidden and forgotten characters, for Characters and Settings ›
/// Data. Without a game folder, nothing is gone.
#[tauri::command(async)]
#[specta::specta]
pub fn tidy_get(state: State<'_, AppState>) -> AppResult<Tidy> {
    let core = &state.core;
    match game(core) {
        Ok((flavor, dir)) => tidy::status(&core.db, &flavor, Some(&dir)),
        Err(AppError::NoInstall) => Ok(Tidy {
            gone: Vec::new(),
            hidden: Vec::new(),
            forgotten: Vec::new(),
        }),
        Err(e) => Err(e),
    }
}

/// "Forever Buddy's own data": sizes and counts (Settings › Data only).
#[tauri::command(async)]
#[specta::specta]
pub fn tidy_data(state: State<'_, AppState>) -> AppResult<DataSize> {
    tidy::data(&state.core.db, &state.core.paths.db_file())
}

#[tauri::command(async)]
#[specta::specta]
pub fn tidy_hide(state: State<'_, AppState>, character_id: u32, hidden: bool) -> AppResult<Tidy> {
    let core = &state.core;
    tidy::set_hidden(&core.db, character_id, hidden, Utc::now().timestamp())?;
    send(core);
    tidy_get(state)
}

/// What Forget would remove, for its dialog. Refused for a character that's
/// still in WTF.
#[tauri::command(async)]
#[specta::specta]
pub fn tidy_forget_preview(
    state: State<'_, AppState>,
    character_id: u32,
) -> AppResult<ForgetPreview> {
    let core = &state.core;
    let (flavor, dir) = game(core)?;
    tidy::preview(&core.db, &flavor, &dir, character_id)
}

#[tauri::command(async)]
#[specta::specta]
pub fn tidy_forget(state: State<'_, AppState>, character_id: u32) -> AppResult<Tidy> {
    let core = &state.core;
    let (flavor, dir) = game(core)?;
    let name = tidy::forget(
        &core.db,
        &flavor,
        &dir,
        character_id,
        Utc::now().timestamp(),
    )?;
    applog::append(
        &core.paths.log_dir,
        &format!("forgot {name}'s history (character {character_id})"),
    );
    send(core);
    tidy_get(state)
}

/// Remember again: the folder can be read again from WTF or a newer backup.
#[tauri::command(async)]
#[specta::specta]
pub fn tidy_remember(state: State<'_, AppState>, forgotten_id: u32) -> AppResult<Tidy> {
    tidy::remember(&state.core.db, forgotten_id)?;
    tidy_get(state)
}

/// Compact (VACUUM). Refused while a backup, restore or ingest holds the job
/// lock, rather than waiting behind it.
#[tauri::command(async)]
#[specta::specta]
pub fn tidy_compact(state: State<'_, AppState>) -> AppResult<DataSize> {
    let core = &state.core;
    {
        let _job = core.jobs.try_lock().map_err(|_| AppError::Busy)?;
        tidy::compact(&core.db)?;
    }
    tidy_data(state)
}
