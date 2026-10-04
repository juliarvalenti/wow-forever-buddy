mod backup;
mod commands;
mod config;
mod db;
mod error;
mod fsx;
mod game;
mod install;
mod secrets;
mod state;
pub mod sv;
#[cfg(test)]
mod test_support;

use tauri::Manager;
use tauri_specta::Event;

use crate::config::paths::AppPaths;
use crate::install::InstallChanged;
use crate::state::{AppCore, AppState};

/// Where the generated TypeScript bindings live. Absolute, so a debug build
/// launched from any working directory still writes into the repo.
const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/bindings.ts");

/// The one place commands and events are registered. Used by `run()` and by
/// the `export_bindings` test, so the TS bindings can never drift from Rust.
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            commands::app::app_info,
            commands::backup::backup_create,
            commands::backup::backup_delete,
            commands::backup::backup_get,
            commands::backup::backup_list,
            commands::backup::backup_set_label,
            commands::backup::backup_set_pinned,
            commands::restore::backup_restore_preview,
            commands::restore::backup_restore,
            commands::restore::backup_verify,
            commands::restore::restore_journal_status,
            commands::restore::restore_journal_resolve,
            commands::game::game_status,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::secrets::secrets_status,
            commands::secrets::secrets_set,
            commands::secrets::secrets_delete,
            commands::install::install_detect,
            commands::install::install_get,
            commands::install::install_set,
            commands::app::app_open_folder,
        ])
        .events(tauri_specta::collect_events![
            InstallChanged,
            commands::backup::BackupCreated,
            commands::backup::BackupProgress,
            commands::restore::RestoreProgress,
            commands::restore::RestoreCompleted,
            commands::game::GameStatusChanged
        ])
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
}

fn export_bindings(builder: &tauri_specta::Builder<tauri::Wry>) {
    builder
        .export(specta_typescript::Typescript::default(), BINDINGS_PATH)
        .expect("failed to export TypeScript bindings");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    export_bindings(&builder);

    tauri::Builder::default()
        // Must be the first plugin. A second launch focuses the running app
        // instead of starting another one that would race on settings,
        // backups and game-file writes (and sweep its temp files).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let paths = AppPaths::resolve(app.handle())?;
            let core = AppCore::new(paths)?;
            let game = core.game.clone();
            app.manage(AppState { core });

            // Startup step 4 (spec §8) runs in the background; the window
            // shows right away and hears about the result via the event.
            let handle = app.handle().clone();
            let install_handle = handle.clone();
            std::thread::spawn(move || {
                let state = install_handle.state::<AppState>();
                if let Some(install) = install::resolve_on_startup(&state.core.settings) {
                    let _ = InstallChanged {
                        install: Some(install),
                    }
                    .emit(&install_handle);
                }
            });

            // Spec §2: poll for WoW every 2 s and tell the UI on each change.
            // The game-exit backup (T8) hooks in here too.
            let target_handle = handle.clone();
            game.spawn(
                move || target_handle.state::<AppState>().core.probe_target(),
                move |_transition, status| {
                    let _ = commands::game::GameStatusChanged(status).emit(&handle);
                },
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    /// Regenerates `src/lib/bindings.ts`. CI runs `cargo test` and then fails if
    /// the file changed, so forgetting to commit regenerated bindings is caught.
    #[test]
    fn export_bindings() {
        super::export_bindings(&super::specta_builder());
    }
}
