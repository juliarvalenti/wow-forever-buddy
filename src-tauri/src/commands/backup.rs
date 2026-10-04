use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::backup::manifest::{SnapshotSummary, Trigger};
use crate::backup::tree::SnapshotDetail;
use crate::backup::{clean_label, SnapshotRequest, SnapshotScope};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Emitted while a backup runs.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct BackupProgress {
    pub done: u32,
    pub total: u32,
}

/// Emitted when a snapshot has been written.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct BackupCreated(pub SnapshotSummary);

/// "Back up now": a full manual snapshot. Allowed while WoW runs, and then
/// flagged as taken mid-session. Waits if another backup or restore is running.
#[tauri::command]
#[specta::specta]
pub async fn backup_create(app: AppHandle, label: Option<String>) -> AppResult<SnapshotSummary> {
    let label = clean_label(label)?;
    tauri::async_runtime::spawn_blocking(move || {
        let core = &app.state::<AppState>().core;
        let _job = core.jobs.lock().expect("job lock poisoned");
        let game = core.active_game()?;
        let include_addons = core.settings.get().backup.include_addons;

        let mut last_sent = 0;
        let summary = core
            .backups
            .create(
                SnapshotRequest {
                    game: &game.root,
                    flavor: &game.flavor,
                    trigger: Trigger::Manual,
                    label,
                    scope: SnapshotScope::Full { include_addons },
                    game_running: core.game.status().running,
                },
                &mut |done, total| {
                    // About fifty updates per backup is plenty for a progress bar.
                    if done == total || done >= last_sent + (total / 50).max(1) {
                        last_sent = done;
                        let _ = BackupProgress { done, total }.emit(&app);
                    }
                },
            )?
            .expect("manual snapshots are never skipped");
        let _ = BackupCreated(summary.clone()).emit(&app);
        Ok(summary)
    })
    .await
    .map_err(|e| AppError::Io(format!("backup task failed: {e}")))?
}

/// Every snapshot, newest first.
#[tauri::command]
#[specta::specta]
pub fn backup_list(state: State<'_, AppState>) -> AppResult<Vec<SnapshotSummary>> {
    state.core.backups.list()
}

/// One snapshot, grouped by account, character and category for the restore panel.
#[tauri::command]
#[specta::specta]
pub fn backup_get(state: State<'_, AppState>, id: String) -> AppResult<SnapshotDetail> {
    state.core.backups.detail(&id)
}

#[tauri::command]
#[specta::specta]
pub fn backup_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let _job = state.core.jobs.lock().expect("job lock poisoned");
    state.core.backups.delete(&id)
}

#[tauri::command]
#[specta::specta]
pub fn backup_set_pinned(
    state: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> AppResult<SnapshotSummary> {
    let _job = state.core.jobs.lock().expect("job lock poisoned");
    state.core.backups.set_pinned(&id, pinned)
}

#[tauri::command]
#[specta::specta]
pub fn backup_set_label(
    state: State<'_, AppState>,
    id: String,
    label: Option<String>,
) -> AppResult<SnapshotSummary> {
    let _job = state.core.jobs.lock().expect("job lock poisoned");
    state.core.backups.set_label(&id, label)
}
