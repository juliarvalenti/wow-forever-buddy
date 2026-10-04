mod commands;
mod config;
mod db;
mod error;
mod fsx;
mod secrets;
mod state;
pub mod sv;
#[cfg(test)]
mod test_support;

use tauri::Manager;

use crate::config::paths::AppPaths;
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
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::secrets::secrets_status,
            commands::secrets::secrets_set,
            commands::secrets::secrets_delete,
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
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let paths = AppPaths::resolve(app.handle())?;
            let core = AppCore::new(paths)?;
            app.manage(AppState { core });
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
