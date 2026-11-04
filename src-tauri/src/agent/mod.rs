//! The agent connection (P2, context/specs/agent-mcp): an MCP server on
//! stdin/stdout that an agent client (Claude Desktop, Claude Code) starts as
//! `wow-forever-buddy mcp`. `main` sends that argument here before Tauri, the
//! single-instance plugin or anything else of the app starts, so this
//! process has no window and no write gate.
//!
//! Every tool reads the app's database opened read-only
//! (`Db::open_read_only`), except the macro tools (MC1), which read the
//! game's own `macros-cache.txt` files in the game folder the app is set up
//! for, as the Macros screen does: read-only, size-capped, through
//! `safe_read`. Every call re-reads settings.json and refuses
//! while "agent access" is off. This process writes only in its own folder:
//! the activity list (`activity`) the app shows in Settings, and proposals
//! (`propose`, P2b) as files in the inbox, which the app checks again and
//! queues for the player's approval.

mod activity;
mod propose;
mod render;
mod rpc;
mod tools;

use std::path::{Path, PathBuf};

pub use activity::{read as read_activity, AgentCall};

/// The bundle identifier in tauri.conf.json: Tauri's app dirs are named
/// after it, and this process finds the same ones without Tauri. A test
/// checks the two agree.
const IDENTIFIER: &str = "com.juliarvalenti.wowforeverbuddy";

/// What an agent client starts the app with: a bare word, not `--mcp`. On
/// Windows, `claude` is usually npm's PowerShell shim, and PowerShell drops
/// the first `--` handed to a script, so `claude mcp add x -- app.exe --mcp`
/// arrived as `claude mcp add x app.exe --mcp` and Claude Code refused
/// `--mcp` as its own unknown option (BUG-MCP). A plain word needs no `--`.
pub const ARG: &str = "mcp";
/// The first spelling, still accepted for configs made before 0.9.
const OLD_FLAG: &str = "--mcp";

/// Whether the command line asks for the agent connection.
pub fn is_agent_start(first_arg: Option<&str>) -> bool {
    matches!(first_arg, Some(ARG) | Some(OLD_FLAG))
}

/// The Claude Code command for Settings › Agents: no `--` (see `ARG`), the
/// path quoted for spaces.
pub fn claude_code_command(exe: &str) -> String {
    format!("claude mcp add forever-buddy \"{exe}\" {ARG}")
}

/// Where the app keeps the files this process reads, the same dirs Tauri's
/// `app_config_dir` and `app_local_data_dir` give the app.
#[derive(Debug, Clone)]
pub struct Paths {
    pub settings: PathBuf,
    pub db: PathBuf,
    /// The agent's own folder: the activity list now, P2b's inbox later.
    pub dir: PathBuf,
    /// The preview's WOWAPI data, as the app downloaded it (UI1's render_ui).
    pub preview_data: PathBuf,
}

/// The app's local data folder (Tauri's `app_local_data_dir`), found
/// without Tauri: for this process and the `render` command line.
pub fn local_dir() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join(IDENTIFIER))
}

impl Paths {
    fn system() -> Option<Self> {
        let config = dirs::config_dir()?.join(IDENTIFIER);
        Some(Self::new(&config, &local_dir()?))
    }

    pub fn new(config_dir: &Path, local_data_dir: &Path) -> Self {
        Self {
            settings: config_dir.join("settings.json"),
            db: local_data_dir.join("buddy.db"),
            dir: local_data_dir.join("agent"),
            preview_data: crate::preview::ui::data_dir(local_data_dir),
        }
    }
}

/// What a call needs from settings.json: the switch, and the game folder and
/// flavor the app is set up for. Read fresh on every call, so turning access
/// off in the app takes effect on the agent's next call.
#[derive(Debug, Clone, PartialEq)]
struct Access {
    on: bool,
    flavor: Option<String>,
    /// The game folder (the macro tools read its WTF).
    root: Option<PathBuf>,
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
        root: value
            .pointer("/install/root")
            .and_then(|r| r.as_str())
            .map(PathBuf::from),
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

    /// BUG-MCP: the copied Claude Code command must survive PowerShell, which
    /// drops a bare `--`, so it has none; both spellings start the server.
    #[test]
    fn the_claude_code_command_needs_no_double_dash() {
        let cmd = claude_code_command(r"C:\Users\Julia\Desktop\WoW Forever Buddy.exe");
        assert_eq!(
            cmd,
            r#"claude mcp add forever-buddy "C:\Users\Julia\Desktop\WoW Forever Buddy.exe" mcp"#
        );
        assert!(
            !cmd.split_whitespace().any(|w| w.starts_with("--")),
            "{cmd}"
        );
        assert!(is_agent_start(Some("mcp")) && is_agent_start(Some("--mcp")));
        assert!(
            !is_agent_start(None)
                && !is_agent_start(Some("—mcp"))
                && !is_agent_start(Some("--mcpx"))
        );
    }

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
