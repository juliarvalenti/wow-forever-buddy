use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::backup::journal::{self, RecoveryStatus};
use crate::backup::manifest::Trigger;
use crate::backup::restore::{
    self, with_restorer, Recovery, RestoreMode, RestorePlan, RestoreReport, RestoreSelection,
    Restorer, VerifyReport,
};
use crate::error::{AppError, AppResult};
use crate::fsx::relpath::RelPath;
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
    /// Forget it without changing files (e.g. the journal is unreadable).
    /// Its pre-restore snapshot stays in the Safety list.
    Discard,
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

/// Restores `selection` from snapshot `id`. `confirmed_deletes` is the
/// preview's `delete` list the user confirmed. Fails before changing anything
/// with `GameRunning` while WoW runs (the UI waits for it to close, then the
/// user confirms again), `ReadOnly`, `BackupCorrupt`, `RestorePending`, or
/// `DeletionsChanged` if it would remove a file not in `confirmed_deletes`.
/// Waits for any running backup or restore.
#[tauri::command]
#[specta::specta]
pub async fn backup_restore(
    app: AppHandle,
    id: String,
    selection: RestoreSelection,
    mode: Option<RestoreMode>,
    confirmed_deletes: Vec<RelPath>,
) -> AppResult<RestoreReport> {
    let mode = mode.unwrap_or_default();
    restore_job(app, move |r, progress| {
        r.run(&id, &selection, mode, &confirmed_deletes, progress)
    })
    .await
}

/// Checks every stored copy in a snapshot against its checksum. Runs as a
/// job, so a concurrent prune's GC can't remove blobs mid-check and make
/// them look missing.
#[tauri::command(async)]
#[specta::specta]
pub fn backup_verify(state: State<'_, AppState>, id: String) -> AppResult<VerifyReport> {
    let _job = state.core.jobs.lock().expect("job lock poisoned");
    let backups = state.core.backups()?;
    let manifest = backups.manifest(&id)?;
    Ok(restore::verify(&backups, &manifest))
}

/// Whether a restore was interrupted, for the startup recovery dialog and the
/// "restores locked" banner. `unreadable` carries the newest pre-restore
/// safety snapshot for "Open the safety copy", since the journal can't say.
#[tauri::command(async)]
#[specta::specta]
pub fn restore_journal_status(state: State<'_, AppState>) -> RecoveryStatus {
    let core = &state.core;
    journal::status(&core.paths.local_data_dir, || {
        core.backups()
            .and_then(|b| b.list())
            .ok()?
            .into_iter()
            .find(|s| s.trigger == Trigger::PreRestore)
            .map(|s| s.id)
    })
}

fn pending(journal_dir: &std::path::Path) -> AppResult<journal::Journal> {
    journal::read(journal_dir)?.ok_or_else(|| AppError::NotFound("no interrupted restore".into()))
}

impl JournalAction {
    fn recovery(self) -> Option<Recovery> {
        match self {
            JournalAction::RollBack => Some(Recovery::RollBack),
            JournalAction::Finish => Some(Recovery::Finish),
            JournalAction::Discard => None,
        }
    }
}

/// What rolling back or finishing the interrupted restore would do, for the
/// recovery dialog's confirm step. Changes nothing. Discard has no plan.
#[tauri::command(async)]
#[specta::specta]
pub fn restore_journal_preview(
    state: State<'_, AppState>,
    action: JournalAction,
) -> AppResult<RestorePlan> {
    let how = action
        .recovery()
        .ok_or_else(|| AppError::NotFound("discard changes no files".into()))?;
    with_restorer(&state.core, |r| {
        r.recovery_plan(&pending(r.journal_dir)?, how)
    })
}

/// Rolls back, finishes or discards an interrupted restore. Until one of
/// these succeeds, `backup_restore` is refused with `RestorePending`.
/// `confirmed_deletes` is `restore_journal_preview`'s `delete` list the user
/// confirmed; roll back and finish are refused with `DeletionsChanged` if
/// they would remove anything else. Discard ignores it.
#[tauri::command]
#[specta::specta]
pub async fn restore_journal_resolve(
    app: AppHandle,
    action: JournalAction,
    confirmed_deletes: Vec<RelPath>,
) -> AppResult<RestoreReport> {
    restore_job(app, move |r, progress| {
        match action.recovery() {
            Some(how) => r.recover(&pending(r.journal_dir)?, how, &confirmed_deletes, progress),
            None => {
                // Discard works even when the journal can't be read.
                let source = pending(r.journal_dir)
                    .map(|j| j.source_snapshot)
                    .unwrap_or_default();
                r.discard()?;
                Ok(RestoreReport {
                    snapshot_id: source,
                    pre_restore_snapshot: None,
                    written: 0,
                    deleted: 0,
                    summary: "interrupted restore discarded".into(),
                })
            }
        }
    })
    .await
}
