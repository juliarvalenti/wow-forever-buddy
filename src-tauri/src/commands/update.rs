//! U2: the update banner's commands (IMPLEMENTING §23). The updater plugin
//! itself has no webview permissions: everything goes through these.

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, AppResult};
use crate::update::{self, UpdateStatus, Updates};

/// Where the banner is. The UI calls this on mount, then follows `update-status`.
#[tauri::command]
#[specta::specta]
pub fn update_status(updates: State<'_, Updates>) -> UpdateStatus {
    updates.status()
}

/// Settings › About's "Check now". Does nothing while "Check for updates" is off.
#[tauri::command]
#[specta::specta]
pub async fn update_check(app: AppHandle, updates: State<'_, Updates>) -> AppResult<UpdateStatus> {
    Ok(update::check(&app, &updates).await)
}

/// "Update now" and "Try again" on the installer build: download and verify.
#[tauri::command]
#[specta::specta]
pub async fn update_download(
    app: AppHandle,
    updates: State<'_, Updates>,
) -> AppResult<UpdateStatus> {
    Ok(update::download(&app, &updates).await)
}

/// "Restart now": installs the downloaded update and reopens the app, once
/// any backup, restore or write running now has finished.
#[tauri::command]
#[specta::specta]
pub fn update_restart(app: AppHandle) {
    std::thread::spawn(move || {
        use tauri::Manager;
        update::install(&app, &app.state::<Updates>(), true);
    });
}

/// "What's new" and "Download from GitHub": the latest release's page, a
/// fixed URL.
#[tauri::command(async)]
#[specta::specta]
pub fn update_open_release_page(app: AppHandle) -> AppResult<()> {
    app.opener()
        .open_url(update::RELEASES_PAGE, None::<&str>)
        .map_err(|e| AppError::Io(e.to_string()))
}
