use serde::{Deserialize, Serialize};
use tauri::State;

use crate::game::process::GameStatus;
use crate::state::AppState;

/// Emitted when WoW starts or stops.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct GameStatusChanged(pub GameStatus);

/// Whether WoW is running, as of the last poll (every 2 s). The UI calls this
/// on mount, then follows `game-status-changed`.
#[tauri::command]
#[specta::specta]
pub fn game_status(state: State<'_, AppState>) -> GameStatus {
    state.core.game.status()
}
