use serde::Serialize;

/// The single error type every command returns. Serialized as
/// `{ kind: "...", detail?: ... }` so the frontend can switch on `kind`.
// The full error model from spec §8, defined up front so the TS union is stable;
// most variants get their first constructor in later tickets.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error, Serialize, specta::Type)]
#[serde(tag = "kind", content = "detail")]
pub enum AppError {
    #[error("World of Warcraft is running; close it first")]
    GameRunning,
    #[error("no game install configured")]
    NoInstall,
    #[error("invalid game install: {0}")]
    InvalidInstall(String),
    #[error("invalid settings: {0}")]
    InvalidSettings(String),
    #[error("path escapes the game folder: {0}")]
    PathEscape(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("file is changing, try again later: {0}")]
    Unstable(String),
    #[error("parse error in {file} at {line}:{col}: {msg}")]
    Parse {
        file: String,
        line: u32,
        col: u32,
        msg: String,
    },
    #[error("backup is corrupt: {files:?}")]
    BackupCorrupt { files: Vec<String> },
    /// Game files marked read-only (players pin e.g. Config.wtf this way).
    /// Never overridden; the user clears the flag to allow the change.
    #[error("read-only files: {paths:?}")]
    ReadOnly { paths: Vec<String> },
    /// A restore was interrupted; it must be rolled back, finished or
    /// discarded before another restore can start.
    #[error("an interrupted restore needs attention first")]
    RestorePending,
    #[error("secret store error: {0}")]
    Secret(String),
    #[error("database error: {0}")]
    Db(String),
    #[error("another backup or restore is in progress")]
    Busy,
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_kind_tag() {
        let unit = serde_json::to_value(AppError::GameRunning).unwrap();
        assert_eq!(unit, serde_json::json!({ "kind": "GameRunning" }));

        let tuple = serde_json::to_value(AppError::NotFound("x".into())).unwrap();
        assert_eq!(
            tuple,
            serde_json::json!({ "kind": "NotFound", "detail": "x" })
        );

        let strukt = serde_json::to_value(AppError::BackupCorrupt {
            files: vec!["a".into()],
        })
        .unwrap();
        assert_eq!(
            strukt,
            serde_json::json!({ "kind": "BackupCorrupt", "detail": { "files": ["a"] } })
        );
    }
}
