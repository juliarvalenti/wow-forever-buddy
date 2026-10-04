use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::backup::journal::{self, Journal};
use crate::backup::restore::{
    self, with_restorer, RestoreMode, RestorePlan, RestoreReport, RestoreSelection, Restorer,
    VerifyReport,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Emitted while a restore runs.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct RestoreProgress {
    pub done: u32,
    pub total: u32,
}

/// Emitted when a restore (or a recovery) has finished.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct RestoreCompleted(pub RestoreReport);

/// What to do about a restore that was interrupted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum JournalAction {
    /// Put back what was there before the restore started (recommended).
    RollBack,
    /// Run the restore again to completion.
    Finish,
}

/// Runs `work` as the one backup/restore job, off the main thread, emitting
/// progress and the completion event.
async fn restore_job(
    app: AppHandle,
    work: impl FnOnce(&Restorer<'_>, &mut dyn FnMut(u32, u32) -> AppResult<()>) -> AppResult<RestoreReport>
        + Send
        + 'static,
) -> AppResult<RestoreReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let core = &app.state::<AppState>().core;
        let _job = core.jobs.lock().expect("job lock poisoned");
        let mut last_sent = 0;
        let report = with_restorer(core, |r| {
            work(r, &mut |done, total| {
                if done == total || done >= last_sent + (total / 50).max(1) {
                    last_sent = done;
                    let _ = RestoreProgress { done, total }.emit(&app);
                }
                Ok(())
            })
        })?;
        let _ = RestoreCompleted(report.clone()).emit(&app);
        Ok(report)
    })
    .await
    .map_err(|e| AppError::Io(format!("restore task failed: {e}")))?
}

/// What restoring `selection` from snapshot `id` would do: files to write
/// (per folder), files to delete, unchanged and read-only files, and a
/// one-line summary. Changes nothing.
#[tauri::command(async)]
#[specta::specta]
pub fn backup_restore_preview(
    state: State<'_, AppState>,
    id: String,
    selection: RestoreSelection,
    mode: Option<RestoreMode>,
) -> AppResult<RestorePlan> {
    let core = &state.core;
    let game = core.active_game()?;
    let manifest = core.backups()?.manifest(&id)?;
    restore::plan(&manifest, &game.root, &selection, mode.unwrap_or_default())
}

/// Restores `selection` from snapshot `id`. Fails with `GameRunning` while
/// WoW runs (the UI waits for it to close, then the user confirms again),
/// `ReadOnly` or `BackupCorrupt` before changing anything. Waits for any
/// running backup or restore.
#[tauri::command]
#[specta::specta]
pub async fn backup_restore(
    app: AppHandle,
    id: String,
    selection: RestoreSelection,
    mode: Option<RestoreMode>,
) -> AppResult<RestoreReport> {
    let mode = mode.unwrap_or_default();
    restore_job(app, move |r, progress| {
        r.run(&id, &selection, mode, progress)
    })
    .await
}

/// Checks every stored copy in a snapshot against its checksum.
#[tauri::command(async)]
#[specta::specta]
pub fn backup_verify(state: State<'_, AppState>, id: String) -> AppResult<VerifyReport> {
    let backups = state.core.backups()?;
    let manifest = backups.manifest(&id)?;
    Ok(restore::verify(&backups, &manifest))
}

/// The interrupted restore, if the app stopped in the middle of one.
#[tauri::command(async)]
#[specta::specta]
pub fn restore_journal_status(state: State<'_, AppState>) -> AppResult<Option<Journal>> {
    journal::read(&state.core.paths.local_data_dir)
}

/// Rolls back or finishes an interrupted restore.
#[tauri::command]
#[specta::specta]
pub async fn restore_journal_resolve(
    app: AppHandle,
    action: JournalAction,
) -> AppResult<RestoreReport> {
    restore_job(app, move |r, progress| {
        let journal = journal::read(r.journal_dir)?
            .ok_or_else(|| AppError::NotFound("no interrupted restore".into()))?;
        match action {
            JournalAction::RollBack => r.roll_back(&journal, progress),
            JournalAction::Finish => r.finish(&journal, progress),
        }
    })
    .await
}
