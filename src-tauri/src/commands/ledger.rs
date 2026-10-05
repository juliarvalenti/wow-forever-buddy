use std::path::PathBuf;

use chrono::{Local, Utc};
use tauri::State;

use crate::error::AppResult;
use crate::ledger::{self, Ledger, LedgerExport, LedgerRange};
use crate::state::AppState;

/// The Ledger for the active flavor: tiles, the daily gold chart and the
/// journal, over `range`, in the user's time zone.
#[tauri::command(async)]
#[specta::specta]
pub fn ledger_get(state: State<'_, AppState>, range: LedgerRange) -> AppResult<Ledger> {
    let core = &state.core;
    let flavor = core.active_game()?.flavor;
    let tz = *Local::now().offset();
    ledger::ledger(&core.db, &flavor, range, Utc::now().timestamp(), &tz)
}

/// "Export CSV": the gold table or the journal, to `dest` from the save
/// dialog (never inside the game or backup folders). Returns the path
/// written, with `.csv` added if it was missing.
#[tauri::command(async)]
#[specta::specta]
pub fn ledger_export_csv(
    state: State<'_, AppState>,
    range: LedgerRange,
    kind: LedgerExport,
    dest: PathBuf,
) -> AppResult<String> {
    let core = &state.core;
    let flavor = core.active_game()?.flavor;
    let mut forbidden = vec![core.backups_dir()];
    if let Some(install) = core.settings.get().install {
        forbidden.push(install.root);
        forbidden.extend(install.links.into_iter().map(|l| l.target));
    }
    let tz = *Local::now().offset();
    let written = ledger::export(
        &core.db,
        &flavor,
        range,
        kind,
        &dest,
        &forbidden,
        Utc::now().timestamp(),
        &tz,
    )?;
    Ok(written.display().to_string())
}
