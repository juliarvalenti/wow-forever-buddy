mod addon;
mod addons;
mod adventures;
mod ah;
mod applog;
mod backup;
mod bridge;
pub mod casc;
mod characters;
mod commands;
mod config;
mod db;
mod error;
mod fsx;
mod game;
mod icons;
mod ingest;
mod install;
mod ledger;
mod lists;
mod macros;
mod notes;
mod plans;
mod quests;
mod secrets;
mod sessions;
mod startup;
mod state;
pub mod sv;
#[cfg(test)]
mod test_support;
mod triggers;

use std::sync::mpsc::Sender;

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
            commands::addon::addon_status,
            commands::addon::addon_install,
            commands::addon::addon_remove,
            commands::addons::addons_list,
            commands::addons::addons_apply,
            commands::addons::addons_undo,
            commands::adventures::adventure_get,
            commands::adventures::adventure_set_note,
            commands::app::app_info,
            commands::ledger::ledger_get,
            commands::ledger::ledger_export_csv,
            commands::macros::macros_list,
            commands::notes::notes_list,
            commands::notes::notes_add,
            commands::notes::notes_delete,
            commands::ah::ah_status,
            commands::ah::ah_search,
            commands::ah::ah_history,
            commands::ah::ah_watchlist,
            commands::ah::ah_set_watched,
            commands::ah::ah_worth_selling,
            commands::ah::ah_goods_worth,
            commands::ah::ah_prices,
            commands::backup::backup_auto_status,
            commands::backup::backup_create,
            commands::backup::backup_delete,
            commands::backup::backup_export_zip,
            commands::backup::backup_get,
            commands::backup::backup_list,
            commands::backup::backup_prune_now,
            commands::backup::backup_move_location,
            commands::backup::backup_set_label,
            commands::backup::backup_set_pinned,
            commands::backup::backup_storage,
            commands::restore::backup_restore_preview,
            commands::restore::backup_restore,
            commands::restore::backup_verify,
            commands::restore::restore_journal_status,
            commands::restore::restore_journal_preview,
            commands::restore::restore_journal_resolve,
            commands::game::game_status,
            commands::icons::icons_cache_status,
            commands::icons::icons_cache_rebuild,
            commands::icons::icons_cache_clear,
            commands::sessions::sessions_list,
            commands::sessions::characters_list,
            commands::ingest::ingest_problems,
            commands::characters::characters_overview,
            commands::characters::character_detail,
            commands::characters::characters_search,
            commands::characters::lockouts_list,
            commands::characters::character_set_bank_alt,
            commands::characters::character_quests,
            commands::characters::quests_available,
            commands::plans::plans_list,
            commands::plans::plan_clear,
            commands::lists::lists_get,
            commands::lists::list_create,
            commands::lists::list_update,
            commands::lists::list_delete,
            commands::lists::list_item_add,
            commands::lists::list_item_need,
            commands::lists::list_item_remove,
            commands::lists::items_seen_search,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::secrets::secrets_status,
            commands::secrets::secrets_set,
            commands::secrets::secrets_delete,
            commands::install::install_detect,
            commands::install::install_get,
            commands::install::install_set,
            commands::app::app_open_folder,
            commands::app::startup_failure,
            commands::app::startup_open_data_folder,
        ])
        .events(tauri_specta::collect_events![
            InstallChanged,
            commands::backup::BackupCreated,
            commands::backup::BackupFailed,
            commands::backup::BackupProgress,
            commands::backup::ExportProgress,
            commands::backup::MoveProgress,
            commands::restore::RestoreProgress,
            commands::restore::RestoreCompleted,
            commands::game::GameStatusChanged,
            sessions::SessionsChanged,
            ah::PricesUpdated,
            ingest::IngestCompleted
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
        .register_asynchronous_uri_scheme_protocol("icon", icon_protocol)
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let paths = AppPaths::resolve(app.handle())?;
            let core = match AppCore::new(paths.clone()) {
                Ok(core) => core,
                // Open the window anyway and explain; nothing else starts.
                Err(e) => {
                    app.manage(startup::StartupFailure::new(&paths, &e));
                    return Ok(());
                }
            };
            let game = core.game.clone();
            // Before the watcher's first poll, which may start a new one.
            let _ = sessions::drop_unfinished(&core.db);
            let db = core.db.clone();
            app.manage(AppState { core });

            // Startup step 4 (spec §8) runs in the background; the window
            // shows right away and hears about the result via the event.
            // Automatic backups start after it, since they need the install.
            let handle = app.handle().clone();
            // v0.2 ingest: reads each character's ForeverBuddy.lua.
            let ingest = spawn_ingest(&handle);
            let install_handle = handle.clone();
            let ingest_start = ingest.clone();
            std::thread::spawn(move || {
                let state = install_handle.state::<AppState>();
                if let Some(install) = state.core.resolve_install_on_startup() {
                    let _ = InstallChanged {
                        install: Some(install),
                    }
                    .emit(&install_handle);
                }
                // Replay backups, then scan, once the game folder is known.
                let _ = ingest_start.send(ingest::Job::Start);
                spawn_auto_backups(&install_handle);
            });
            // A logout to the character screen or a /reload writes the file
            // while WoW runs: the watcher asks for a scan a few seconds after.
            let watch_handle = handle.clone();
            let ingest_watch = ingest.clone();
            // Validating the game folder rescans the install, so the folder
            // is looked up when WoW starts and once a minute after, not on
            // every poll; the poll only stats files, and the scan it asks
            // for validates again.
            std::thread::spawn(move || {
                let mut watcher = ingest::Watcher::default();
                let mut folder: Option<(std::path::PathBuf, std::time::Instant)> = None;
                loop {
                    std::thread::sleep(ingest::WATCH_EVERY);
                    let core = &watch_handle.state::<AppState>().core;
                    if !core.game.status().running {
                        folder = None;
                        continue;
                    }
                    if folder
                        .as_ref()
                        .is_none_or(|(_, at)| at.elapsed().as_secs() >= 60)
                    {
                        folder = core
                            .active_game()
                            .ok()
                            .map(|g| (g.root.base, std::time::Instant::now()));
                    }
                    if let Some((dir, _)) = &folder {
                        if watcher.poll(dir) {
                            let _ = ingest_watch.send(ingest::Job::Scan);
                        }
                    }
                }
            });

            // Spec §2: poll for WoW every 2 s and tell the UI on each change.
            // When the game stops, the game-exit backup runs (spec §5), and
            // the session is recorded (T14).
            let target_handle = handle.clone();
            let sessions = spawn_sessions(&handle, db);
            game.spawn(
                move || target_handle.state::<AppState>().core.probe_target(),
                move |transition, status| {
                    let _ = commands::game::GameStatusChanged(status.clone()).emit(&handle);
                    let now = || chrono::Utc::now().to_rfc3339();
                    match transition {
                        game::process::Transition::Started => {
                            let at = status.since.unwrap_or_else(now);
                            let _ = sessions.send(sessions::SessionEvent::Started(at));
                        }
                        game::process::Transition::Stopped => {
                            let _ = sessions.send(sessions::SessionEvent::Stopped(now()));
                            spawn_game_exit_backup(&handle);
                            let _ = ingest.send(ingest::Job::AfterExit);
                        }
                        // Listing blipped and came back with the game's state
                        // unchanged: the session carries on.
                        game::process::Transition::Unknown | game::process::Transition::Known => {}
                    }
                },
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// `icon://localhost/<fileDataId>` (`http://icon.localhost/…` on Windows):
/// an item icon as PNG, or 404 for "no icon", and the UI keeps its letter
/// tile. The path must be a number; nothing from it reaches the file system
/// except as that number. While item icons are off (F8c) every request is a
/// 404, cached or not.
fn icon_protocol(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    use tauri::http::{header, Response, StatusCode};
    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap_or_default()
    };
    let id = request
        .uri()
        .path()
        .trim_start_matches('/')
        .parse::<u32>()
        .ok()
        .filter(|&id| id > 0);
    let Some(state) = ctx.app_handle().try_state::<AppState>() else {
        return responder.respond(not_found());
    };
    let (Some(id), Some(flavor)) = (id, state.core.icon_flavor_dir()) else {
        return responder.respond(not_found());
    };
    state.core.icons.get(
        flavor,
        id,
        Box::new(move |png| {
            responder.respond(match png {
                Some(png) => Response::builder()
                    .header(header::CONTENT_TYPE, "image/png")
                    .header(header::CACHE_CONTROL, "max-age=3600")
                    .body(png)
                    .unwrap_or_else(|_| not_found()),
                None => not_found(),
            })
        }),
    );
}

/// The ingest worker: jobs run in order on one thread, and the UI hears
/// `ingest-completed` when characters' data changed.
fn spawn_ingest(handle: &tauri::AppHandle) -> std::sync::mpsc::Sender<ingest::Job> {
    let h = handle.clone();
    ingest::spawn_worker(move |job| {
        let core = &h.state::<AppState>().core;
        let changed = ingest::run_job(core, &job, |wtf| {
            triggers::wait_until_settled(wtf, triggers::EXIT_SETTLE, triggers::EXIT_SETTLE_TIMEOUT);
        });
        if !changed.is_empty() {
            let _ = ingest::IngestCompleted {
                characters: changed.into_iter().map(|id| id as u32).collect(),
            }
            .emit(&h);
        }
        // Item icons (F8): read any new item's icon now, in the background,
        // so screens find them cached. Only when the user turned them on.
        if let (Some(flavor), Ok(ids)) = (core.icon_flavor_dir(), icons::known_ids(&core.db)) {
            core.icons.prefetch(flavor, ids);
        }
        // Auctionator's prices (F5), on the same triggers.
        if let Ok(game) = core.active_game() {
            match ah::scan(&core.db, &game.flavor, &game.root.base) {
                Ok(Some(items)) => {
                    let _ = ah::PricesUpdated { items }.emit(&h);
                }
                Ok(None) => {}
                Err(e) => {
                    applog::append(&core.paths.log_dir, &format!("auction prices failed: {e}"))
                }
            }
        }
        // The tooltip index (bridge spec §5), from what changed above. It's
        // rewritten only when its data changed, and waits while WoW runs:
        // the ingest after the game exits sends it.
        match core.send_to_game() {
            Ok(_) | Err(error::AppError::NoInstall) => {}
            Err(e) => applog::append(
                &core.paths.log_dir,
                &format!("sending to the game failed: {e}"),
            ),
        }
    })
}

/// Emits `backup-created` for an automatic snapshot.
fn announce(handle: &tauri::AppHandle) -> impl Fn(&backup::manifest::SnapshotSummary) + '_ {
    move |summary| {
        let _ = commands::backup::BackupCreated(summary.clone()).emit(handle);
    }
}

/// Tells the UI an automatic backup failed (R1). `run_auto` has already
/// logged it and kept it for `backup_auto_status`; "no game folder yet" isn't
/// a failure.
fn report_failure<T>(handle: &tauri::AppHandle, result: error::AppResult<T>) {
    if result.is_err_and(|e| !matches!(e, error::AppError::NoInstall)) {
        let core = &handle.state::<AppState>().core;
        if let Ok(Some(failure)) = triggers::last_failure(core) {
            let _ = commands::backup::BackupFailed(failure).emit(handle);
        }
    }
}

/// App-start and scheduled backups (spec §5), each on its own thread so
/// startup and the UI never wait for them. A failure (backup drive missing,
/// game folder gone) is logged and shown on the Backups screen.
fn spawn_auto_backups(handle: &tauri::AppHandle) {
    use backup::manifest::Trigger;

    // The db's own daily copy (v0.2 spec §4): at start, then checked hourly,
    // so an app left open across midnight still takes the next day's.
    let h = handle.clone();
    std::thread::spawn(move || loop {
        let core = &h.state::<AppState>().core;
        core.take_daily_db_copy(chrono::Local::now().date_naive());
        std::thread::sleep(std::time::Duration::from_secs(3600));
    });

    let h = handle.clone();
    std::thread::spawn(move || {
        let core = &h.state::<AppState>().core;
        if triggers::app_start_due(core, chrono::Utc::now()).unwrap_or(false) {
            report_failure(
                &h,
                triggers::run_auto(core, Trigger::AppStart, &announce(&h)),
            );
        }
    });

    let h = handle.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(triggers::SCHEDULE_TICK);
        let core = &h.state::<AppState>().core;
        if triggers::schedule_due(core, chrono::Utc::now()).unwrap_or(false) {
            report_failure(
                &h,
                triggers::run_auto(core, Trigger::Scheduled, &announce(&h)),
            );
        }
    });
}

/// The play-session worker (T14): records the watcher's starts and stops in
/// order, off the watcher thread, and tells the UI after each change. A stop
/// waits for WoW's exit writes before attributing characters.
fn spawn_sessions(handle: &tauri::AppHandle, db: db::Db) -> Sender<sessions::SessionEvent> {
    let (game_handle, change_handle) = (handle.clone(), handle.clone());
    let (tx, _worker) = sessions::spawn_worker(
        db,
        move || {
            let game = game_handle.state::<AppState>().core.active_game().ok()?;
            Some(sessions::GameContext {
                flavor: game.flavor,
                dir: game.root.base,
            })
        },
        |wtf| {
            triggers::wait_until_settled(wtf, triggers::EXIT_SETTLE, triggers::EXIT_SETTLE_TIMEOUT);
        },
        move || {
            let _ = sessions::SessionsChanged.emit(&change_handle);
        },
    );
    tx
}

/// After WoW exits: wait for its last SavedVariables writes, then back up.
fn spawn_game_exit_backup(handle: &tauri::AppHandle) {
    let h = handle.clone();
    std::thread::spawn(move || {
        let core = &h.state::<AppState>().core;
        if !core.settings.get().backup.on_game_exit {
            return;
        }
        let result = triggers::game_exit_backup(
            core,
            triggers::EXIT_SETTLE,
            triggers::EXIT_SETTLE_TIMEOUT,
            &announce(&h),
        );
        report_failure(&h, result);
    });
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
