use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use crate::backup::export::{export_zip, ExportReport};
use crate::backup::journal;
use crate::backup::manifest::{SnapshotSummary, Trigger};
use crate::backup::retention::POLICY;
use crate::backup::tree::SnapshotDetail;
use crate::backup::{clean_label, PruneReport, SnapshotRequest, SnapshotScope, StorageInfo};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::triggers::{self, AutoBackupFailure};

/// Emitted while a backup runs.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct BackupProgress {
    pub done: u32,
    pub total: u32,
}

/// Emitted when a snapshot has been written.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct BackupCreated(pub SnapshotSummary);

/// Emitted when an automatic backup fails (R1). The same failure stays
/// available from `backup_auto_status` until an automatic backup succeeds.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct BackupFailed(pub AutoBackupFailure);

/// The latest automatic backup failure, if no automatic backup has
/// succeeded since. The UI asks on start, since a failure can happen before
/// it's listening.
#[tauri::command(async)]
#[specta::specta]
pub fn backup_auto_status(state: State<'_, AppState>) -> AppResult<Option<AutoBackupFailure>> {
    triggers::last_failure(&state.core)
}

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
            .backups()?
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

// These run off the main thread (`async`): `backups()` may open the store,
// which touches the backup location (an offline network share can take tens
// of seconds) and can reindex every manifest after a location change.
// Delete, pin and label edit one manifest under the store's own short lock
// and never wait on a running backup.

/// Every snapshot, newest first.
#[tauri::command(async)]
#[specta::specta]
pub fn backup_list(state: State<'_, AppState>) -> AppResult<Vec<SnapshotSummary>> {
    state.core.backups()?.list()
}

/// One snapshot, grouped by account, character and category for the restore panel.
#[tauri::command(async)]
#[specta::specta]
pub fn backup_get(state: State<'_, AppState>, id: String) -> AppResult<SnapshotDetail> {
    state.core.backups()?.detail(&id)
}

/// Refused for a snapshot an interrupted restore still needs, and while the
/// restore journal is unreadable (then nobody can tell which those are).
/// Runs as a job, so it can't race a restore that's starting from the same
/// snapshot before its journal is written; it waits behind a running backup.
#[tauri::command(async)]
#[specta::specta]
pub fn backup_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let core = &state.core;
    let _job = core.jobs.lock().expect("job lock poisoned");
    let held = journal::held_snapshots(&core.paths.local_data_dir)?;
    core.backups()?.delete(&id, &held)
}

#[tauri::command(async)]
#[specta::specta]
pub fn backup_set_pinned(
    state: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> AppResult<SnapshotSummary> {
    state.core.backups()?.set_pinned(&id, pinned)
}

#[tauri::command(async)]
#[specta::specta]
pub fn backup_set_label(
    state: State<'_, AppState>,
    id: String,
    label: Option<String>,
) -> AppResult<SnapshotSummary> {
    state.core.backups()?.set_label(&id, label)
}

/// Emitted while an export runs.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct ExportProgress {
    pub done: u32,
    pub total: u32,
}

/// "Export as .zip". `dest` comes from the save dialog; it may not be inside
/// the game folder (or a linked folder's target) or the backup store. Runs
/// as the one backup/restore job, so pruning can't remove the snapshot
/// mid-export.
#[tauri::command]
#[specta::specta]
pub async fn backup_export_zip(
    app: AppHandle,
    id: String,
    dest: std::path::PathBuf,
) -> AppResult<ExportReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let core = &app.state::<AppState>().core;
        let _job = core.jobs.lock().expect("job lock poisoned");
        let mut forbidden = vec![core.backups_dir()];
        if let Some(install) = core.settings.get().install {
            forbidden.push(install.root);
            forbidden.extend(install.links.into_iter().map(|l| l.target));
        }
        let mut last_sent = 0;
        export_zip(
            &*core.backups()?,
            &id,
            &dest,
            &forbidden,
            &mut |done, total| {
                if done == total || done >= last_sent + (total / 50).max(1) {
                    last_sent = done;
                    let _ = ExportProgress { done, total }.emit(&app);
                }
            },
        )
    })
    .await
    .map_err(|e| AppError::Io(format!("export task failed: {e}")))?
}

/// The storage meter and the retention sentence for the Backups header.
#[tauri::command(async)]
#[specta::specta]
pub fn backup_storage(state: State<'_, AppState>) -> AppResult<StorageInfo> {
    Ok(state.core.backups()?.storage(&POLICY))
}

/// Settings' "Prune now". Waits for any running backup or restore, because
/// garbage collection must never overlap one. Keeps whatever an interrupted
/// restore still needs, and refuses while its journal is unreadable.
#[tauri::command(async)]
#[specta::specta]
pub fn backup_prune_now(state: State<'_, AppState>) -> AppResult<PruneReport> {
    let core = &state.core;
    let _job = core.jobs.lock().expect("job lock poisoned");
    let held = journal::held_snapshots(&core.paths.local_data_dir)?;
    core.backups()?
        .prune(chrono::Utc::now(), &POLICY, crate::backup::Gc::Now, &held)
}
