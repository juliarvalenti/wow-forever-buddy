use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::config::paths::AppPaths;
use crate::error::{AppError, AppResult};
use crate::install;
use crate::startup::{Retry, StartupFailure, StartupSlot};
use crate::state::{AppCore, AppState};

#[derive(Debug, Serialize, specta::Type)]
pub struct AppInfo {
    pub version: String,
    pub paths: AppPaths,
}

/// App version and data locations, for the Settings/about panel and bug reports.
#[tauri::command]
#[specta::specta]
pub fn app_info(state: State<'_, AppState>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        paths: state.core.paths.clone(),
    })
}

/// The folders the UI can reveal. A fixed set: the frontend never passes a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FolderTarget {
    Backups,
    Logs,
    /// The active flavor folder, e.g. `<root>/_classic_beta_`.
    Game,
}

pub fn folder_path(core: &AppCore, which: FolderTarget) -> AppResult<PathBuf> {
    match which {
        FolderTarget::Backups => Ok(core.backups_dir()),
        FolderTarget::Logs => Ok(core.paths.log_dir.clone()),
        FolderTarget::Game => install::current(&core.settings)?
            .and_then(|i| i.active_flavor().map(|f| f.dir.clone()))
            .ok_or(AppError::NoInstall),
    }
}

/// Opens one of the app's folders in Explorer/Finder. The opening happens in
/// Rust, so the webview needs no opener permissions at all.
#[tauri::command(async)]
#[specta::specta]
pub fn app_open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    which: FolderTarget,
) -> AppResult<()> {
    let dir = folder_path(&state.core, which)?;
    if which != FolderTarget::Game {
        std::fs::create_dir_all(&dir)?;
    }
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Io(e.to_string()))
}

/// Why the app couldn't start, or `null` when it started fine. The UI asks
/// this first: in the failure case no other command has state to work with.
#[tauri::command]
#[specta::specta]
pub fn startup_failure(app: AppHandle) -> Option<StartupFailure> {
    app.try_state::<StartupSlot>().and_then(|s| s.get())
}

/// "Open data folder" on the startup error screen: the folder holding the
/// file at fault. Works without `AppState`.
#[tauri::command(async)]
#[specta::specta]
pub fn startup_open_data_folder(app: AppHandle) -> AppResult<()> {
    let failure = app
        .try_state::<StartupSlot>()
        .and_then(|s| s.get())
        .ok_or_else(|| AppError::NotFound("the app started normally".into()))?;
    app.opener()
        .open_path(failure.folder().to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Io(e.to_string()))
}

/// The startup screen's "Try again", and its "Update without a safety copy"
/// (`skip_safety_copy`). Re-runs startup; returns `null` if the app started
/// (the UI then reloads into it), or the new failure. Skipping the copy is
/// refused unless the current failure is that the copy failed, and it
/// applies to this attempt only: nothing is saved.
#[tauri::command(async)]
#[specta::specta]
pub fn startup_retry(app: AppHandle, skip_safety_copy: bool) -> AppResult<Option<StartupFailure>> {
    let slot = app
        .try_state::<StartupSlot>()
        .ok_or_else(|| AppError::NotFound("the app started normally".into()))?;
    let outcome = slot.retry(skip_safety_copy, AppCore::new_with)?;
    match outcome {
        Retry::Started(core) => {
            crate::start(&app, core);
            Ok(None)
        }
        Retry::Failed(failure) => Ok(Some(failure)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_copy;

    #[test]
    fn folder_targets_resolve_to_app_or_game_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let core = AppCore::new(AppPaths::under(tmp.path())).unwrap();

        assert_eq!(
            folder_path(&core, FolderTarget::Backups).unwrap(),
            core.paths.local_data_dir.join("backups")
        );
        assert_eq!(
            folder_path(&core, FolderTarget::Logs).unwrap(),
            core.paths.log_dir
        );
        assert!(matches!(
            folder_path(&core, FolderTarget::Game),
            Err(AppError::NoInstall)
        ));

        let (_w, root) = fixture_copy();
        let install = install::set(&core.settings, &root, None).unwrap();
        assert_eq!(
            folder_path(&core, FolderTarget::Game).unwrap(),
            install.active_flavor().unwrap().dir
        );

        let custom = tmp.path().join("elsewhere");
        core.settings
            .update(|s| s.backup.location = Some(custom.clone()))
            .unwrap();
        assert_eq!(
            folder_path(&core, FolderTarget::Backups).unwrap(),
            custom.join(crate::state::STORE_FOLDER),
            "a picked location holds the store in its own subfolder"
        );
    }

    #[test]
    fn unknown_targets_are_rejected() {
        for bad in ["config", "../", "C:\\Windows", "Backups"] {
            assert!(serde_json::from_value::<FolderTarget>(serde_json::json!(bad)).is_err());
        }
    }
}
