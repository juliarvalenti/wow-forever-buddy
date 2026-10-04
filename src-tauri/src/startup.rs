//! When the app's own data can't be opened at startup (a database from a
//! newer build after a downgrade, unreadable settings, a full disk), the
//! window still opens and explains what happened instead of the app exiting
//! silently. See design/mocks/round-3/startup-error.html.

use std::path::PathBuf;

use serde::Serialize;

use crate::config::paths::AppPaths;
use crate::error::AppError;

/// Which of the app's files is the problem, so the UI can mark it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum StartupProblem {
    /// `buddy.db`, e.g. written by a newer version of the app.
    Database,
    /// `settings.json`.
    Settings,
    /// Anything else, e.g. a data folder that can't be created.
    Other,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct StartupFailure {
    pub problem: StartupProblem,
    /// The error, for "Error details" and "Copy error details".
    pub message: String,
    pub paths: AppPaths,
    /// The file at fault, shown in red; `None` if it isn't one file.
    pub at_fault: Option<PathBuf>,
}

impl StartupFailure {
    pub fn new(paths: &AppPaths, err: &AppError) -> Self {
        let (problem, at_fault) = match err {
            AppError::Db(_) => (StartupProblem::Database, Some(paths.db_file())),
            AppError::InvalidSettings(_) => (StartupProblem::Settings, Some(paths.settings_file())),
            _ => (StartupProblem::Other, None),
        };
        Self {
            problem,
            message: err.to_string(),
            paths: paths.clone(),
            at_fault,
        }
    }

    /// The folder "Open data folder" shows: the one holding the file at
    /// fault, else the local data folder (database and backups).
    pub fn folder(&self) -> PathBuf {
        self.at_fault
            .as_ref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.paths.local_data_dir.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_at_fault_follows_the_error() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(tmp.path());

        let db = StartupFailure::new(&paths, &AppError::Db("schema version 9 is newer".into()));
        assert_eq!(db.problem, StartupProblem::Database);
        assert_eq!(db.at_fault, Some(paths.db_file()));
        assert_eq!(db.folder(), paths.local_data_dir);
        assert!(db.message.contains("newer"));

        let settings = StartupFailure::new(&paths, &AppError::InvalidSettings("bad".into()));
        assert_eq!(settings.problem, StartupProblem::Settings);
        assert_eq!(settings.folder(), paths.config_dir);

        let other = StartupFailure::new(&paths, &AppError::Io("denied".into()));
        assert_eq!(other.problem, StartupProblem::Other);
        assert_eq!(other.at_fault, None);
        assert_eq!(other.folder(), paths.local_data_dir);
    }

    #[test]
    fn a_real_newer_database_produces_a_database_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(tmp.path());
        std::fs::create_dir_all(&paths.local_data_dir).unwrap();
        // A database whose schema is far ahead of this build.
        let conn = rusqlite::Connection::open(paths.db_file()).unwrap();
        conn.pragma_update(None, "user_version", 9999).unwrap();
        drop(conn);

        let err = match crate::state::AppCore::new(paths.clone()) {
            Ok(_) => panic!("a newer database must not open"),
            Err(e) => e,
        };
        let failure = StartupFailure::new(&paths, &err);
        assert_eq!(failure.problem, StartupProblem::Database);
        assert_eq!(failure.at_fault, Some(paths.db_file()));
    }
}
