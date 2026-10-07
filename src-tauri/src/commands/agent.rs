use serde::Serialize;
use tauri::State;

use crate::agent::{self, AgentCall};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Serialize, specta::Type)]
pub struct AgentStatus {
    /// This app's own executable: what an agent client starts, with `--mcp`.
    pub command: String,
    pub flag: String,
    /// The last calls an agent made, newest first.
    pub activity: Vec<AgentCall>,
}

/// Settings › Agents (P2): the command to give an agent client, and what
/// agents have called lately. The switch itself is `settings.agent_access`.
#[tauri::command(async)]
#[specta::specta]
pub fn agent_status(state: State<'_, AppState>) -> AppResult<AgentStatus> {
    let paths = &state.core.paths;
    let command = std::env::current_exe()
        .map_err(|e| AppError::Io(e.to_string()))?
        .to_string_lossy()
        .into_owned();
    let dir = agent::Paths::new(&paths.config_dir, &paths.local_data_dir).dir;
    Ok(AgentStatus {
        command,
        flag: agent::FLAG.to_string(),
        activity: agent::read_activity(&dir),
    })
}
