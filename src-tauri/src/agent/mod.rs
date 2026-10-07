//! The agent connection (P2, docs/specs/agent-mcp.md): an MCP server on
//! stdin/stdout that an agent client (Claude Desktop, Claude Code) starts as
//! `wow-forever-buddy --mcp`. `main` sends that flag here before Tauri, the
//! single-instance plugin or anything else of the app starts, so this
//! process has no window, no game paths and no write gate.
//!
//! Every tool reads the app's database opened read-only
//! (`Db::open_read_only`). Every call re-reads settings.json and refuses
//! while "agent access" is off. This process writes only in its own folder:
//! the activity list (`activity`) the app shows in Settings, and proposals
//! (`propose`, P2b) as files in the inbox, which the app checks again and
//! queues for the player's approval.

mod activity;
mod propose;
mod rpc;
mod tools;

use std::path::{Path, PathBuf};

pub use activity::{read as read_activity, AgentCall};

/// The bundle identifier in tauri.conf.json: Tauri's app dirs are named
/// after it, and this process finds the same ones without Tauri. A test
/// checks the two agree.
const IDENTIFIER: &str = "com.juliarvalenti.wowforeverbuddy";

/// The flag an agent client starts the app with.
pub const FLAG: &str = "--mcp";

/// Where the app keeps the files this process reads, the same dirs Tauri's
/// `app_config_dir` and `app_local_data_dir` give the app.
#[derive(Debug, Clone)]
pub struct Paths {
    pub settings: PathBuf,
    pub db: PathBuf,
    /// The agent's own folder: the activity list now, P2b's inbox later.
    pub dir: PathBuf,
}

impl Paths {
    fn system() -> Option<Self> {
        let config = dirs::config_dir()?.join(IDENTIFIER);
        let local = dirs::data_local_dir()?.join(IDENTIFIER);
        Some(Self::new(&config, &local))
    }

    pub fn new(config_dir: &Path, local_data_dir: &Path) -> Self {
        Self {
            settings: config_dir.join("settings.json"),
            db: local_data_dir.join("buddy.db"),
            dir: local_data_dir.join("agent"),
        }
    }
}

/// What a call needs from settings.json: the switch, and the flavor the app
/// is set up for. Read fresh on every call, so turning access off in the
/// app takes effect on the agent's next call.
#[derive(Debug, Clone, PartialEq)]
struct Access {
    on: bool,
    flavor: Option<String>,
}

fn access(settings: &Path) -> Access {
    let value: serde_json::Value = std::fs::read(settings)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    Access {
        // Only an explicit `true` is on: a missing or unreadable file is off.
        on: value.get("agent_access") == Some(&serde_json::Value::Bool(true)),
        flavor: value
            .pointer("/install/flavor")
            .and_then(|f| f.as_str())
            .map(str::to_string),
    }
}

/// Serves MCP on stdin/stdout until the client closes stdin. Returns the
/// process exit code.
pub fn serve_stdio() -> i32 {
    let Some(paths) = Paths::system() else {
        eprintln!("forever-buddy: can't find this user's app data folders");
        return 1;
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match rpc::serve(&paths, stdin.lock(), stdout.lock()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("forever-buddy: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifier_matches_the_bundle() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], IDENTIFIER);
    }

    #[test]
    fn access_is_on_only_when_set_true() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(!access(&path).on, "no file");
        for (text, on) in [
            ("{}", false),
            (r#"{"agent_access": false}"#, false),
            (r#"{"agent_access": "true"}"#, false),
            ("not json", false),
            (r#"{"agent_access": true}"#, true),
        ] {
            std::fs::write(&path, text).unwrap();
            assert_eq!(access(&path).on, on, "{text}");
        }
        std::fs::write(
            &path,
            r#"{"agent_access": true, "install": {"root": "/x", "flavor": "_classic_beta_"}}"#,
        )
        .unwrap();
        assert_eq!(access(&path).flavor.as_deref(), Some("_classic_beta_"));
    }
}
