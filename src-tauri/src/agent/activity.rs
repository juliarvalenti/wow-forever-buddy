//! "Recent agent activity" (spec §1): the last calls an agent made, kept by
//! the agent process in `<local data>/agent/activity.json` and read by the
//! app for Settings › Agents. Only the client's name, the tool, the time and
//! whether it worked: never the arguments.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::fsx::atomic::atomic_replace;

const FILE: &str = "activity.json";
/// Calls kept, newest first.
pub const KEPT: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct AgentCall {
    /// RFC 3339, UTC.
    pub at: String,
    /// The client's own name for itself (MCP `clientInfo.name`): a claim,
    /// not a verified identity.
    pub client: String,
    pub tool: String,
    /// False when the tool refused or failed (access off, unknown character).
    pub ok: bool,
}

/// The kept calls, newest first; empty if there are none or the file is
/// unreadable.
pub fn read(dir: &Path) -> Vec<AgentCall> {
    let mut calls: Vec<AgentCall> = std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    calls.truncate(KEPT);
    calls
}

/// Adds a call at the front. Best effort: a call is never refused because
/// its activity line couldn't be written.
pub fn record(dir: &Path, call: AgentCall) {
    let mut calls = read(dir);
    calls.insert(0, call);
    calls.truncate(KEPT);
    let write = || -> crate::error::AppResult<()> {
        std::fs::create_dir_all(dir)?;
        let bytes =
            serde_json::to_vec(&calls).map_err(|e| crate::error::AppError::Io(e.to_string()))?;
        atomic_replace(&dir.join(FILE), &bytes)
    };
    if let Err(e) = write() {
        eprintln!("forever-buddy: couldn't note the call in the activity list: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(n: usize) -> AgentCall {
        AgentCall {
            at: format!("2026-10-07T20:{:02}:00Z", n % 60),
            client: "Claude Desktop".into(),
            tool: format!("tool{n}"),
            ok: true,
        }
    }

    #[test]
    fn keeps_the_newest_calls_first() {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join("agent");
        assert!(read(&agent).is_empty());
        for n in 0..KEPT + 5 {
            record(&agent, call(n));
        }
        let calls = read(&agent);
        assert_eq!(calls.len(), KEPT);
        assert_eq!(calls[0].tool, format!("tool{}", KEPT + 4));
        assert_eq!(calls[KEPT - 1].tool, "tool5");
    }
}
