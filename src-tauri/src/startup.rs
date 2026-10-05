//! When the app's own data can't be opened at startup (a database from a
//! newer build after a downgrade, unreadable settings, a full disk), the
//! window still opens and explains what happened instead of the app exiting
//! silently. See design/mocks/round-3/startup-error.html.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;

use crate::config::paths::AppPaths;
use crate::db::copies::{self, CopyFailure};
use crate::db::OpenOptions;
use crate::error::{AppError, AppResult};

/// Which of the app's files is the problem, so the UI can mark it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum StartupProblem {
    /// `buddy.db`, e.g. written by a newer version of the app.
    Database,
    /// The safety copy before updating `buddy.db` couldn't be made, so the
    /// update didn't run (`?case=copy`): Try again, or update without it.
    UpgradeCopy,
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
    /// For `UpgradeCopy`: why, in plain terms (disk full with numbers,
    /// locked, other).
    pub copy_failure: Option<CopyFailure>,
    /// For `UpgradeCopy`'s confirm: the date (YYYY-MM-DD) of the newest daily
    /// copy the app would fall back to if the update failed, if any.
    pub fallback_copy: Option<String>,
}

impl StartupFailure {
    pub fn new(paths: &AppPaths, err: &AppError) -> Self {
        let (problem, at_fault) = match err {
            AppError::Db(_) => (StartupProblem::Database, Some(paths.db_file())),
            AppError::UpgradeCopyFailed(_) => (StartupProblem::UpgradeCopy, Some(paths.db_file())),
            AppError::InvalidSettings(_) => (StartupProblem::Settings, Some(paths.settings_file())),
            _ => (StartupProblem::Other, None),
        };
        let copy_failure = match err {
            AppError::UpgradeCopyFailed(why) => Some(why.clone()),
            _ => None,
        };
        let fallback_copy = copy_failure.as_ref().and_then(|_| {
            copies::list(&copies::dir_for(&paths.db_file()))
                .first()
                .map(|(day, _)| day.format("%Y-%m-%d").to_string())
        });
        Self {
            problem,
            message: err.to_string(),
            paths: paths.clone(),
            at_fault,
            copy_failure,
            fallback_copy,
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

/// Managed state: why the app couldn't start, or empty once it has. Behind
/// a mutex so the startup screen's retry can replace or clear it.
#[derive(Default)]
pub struct StartupSlot(Mutex<Option<StartupFailure>>);

/// What a retry from the startup screen came to.
pub enum Retry<T> {
    /// The app's data is open; the caller starts the app with it.
    Started(T),
    /// Still failing; the new failure is in the slot and returned.
    Failed(StartupFailure),
}

impl StartupSlot {
    pub fn failed(failure: StartupFailure) -> Self {
        Self(Mutex::new(Some(failure)))
    }

    pub fn get(&self) -> Option<StartupFailure> {
        self.0.lock().expect("startup lock poisoned").clone()
    }

    /// Runs `build` again (the app's startup) with the given override.
    /// `skip_safety_copy` is refused unless the current failure is that the
    /// safety copy failed, so it can't be used to skip the copy in general;
    /// it's passed to this one attempt only and never stored. The lock is
    /// held throughout, so two retries can't race.
    pub fn retry<T>(
        &self,
        skip_safety_copy: bool,
        build: impl FnOnce(AppPaths, OpenOptions) -> AppResult<T>,
    ) -> AppResult<Retry<T>> {
        let mut slot = self.0.lock().expect("startup lock poisoned");
        let current = slot
            .as_ref()
            .ok_or_else(|| AppError::NotFound("the app started normally".into()))?;
        if skip_safety_copy && current.problem != StartupProblem::UpgradeCopy {
            return Err(AppError::NotFound(
                "updating without a safety copy is only offered when the safety copy failed".into(),
            ));
        }
        let paths = current.paths.clone();
        let opts = OpenOptions {
            skip_pre_migration_copy: skip_safety_copy,
        };
        match build(paths.clone(), opts) {
            Ok(started) => {
                *slot = None;
                Ok(Retry::Started(started))
            }
            Err(e) => {
                let failure = StartupFailure::new(&paths, &e);
                *slot = Some(failure.clone());
                Ok(Retry::Failed(failure))
            }
        }
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

    fn copy_failed(paths: &AppPaths) -> StartupFailure {
        StartupFailure::new(paths, &AppError::UpgradeCopyFailed(CopyFailure::Locked))
    }

    /// V5b: the copy-failed case carries its reason and the fallback date.
    #[test]
    fn a_failed_safety_copy_carries_its_reason_and_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(tmp.path());
        let dir = copies::dir_for(&paths.db_file());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("buddy-2026-10-03.db"), b"").unwrap();
        std::fs::write(dir.join("buddy-2026-10-04.db"), b"").unwrap();

        let failure = copy_failed(&paths);
        assert_eq!(failure.problem, StartupProblem::UpgradeCopy);
        assert_eq!(failure.at_fault, Some(paths.db_file()));
        assert_eq!(failure.copy_failure, Some(CopyFailure::Locked));
        assert_eq!(failure.fallback_copy.as_deref(), Some("2026-10-04"));
    }

    /// V5b: "Update without a safety copy" only works from the copy-failed
    /// state, applies to that one attempt, and a success clears the failure.
    #[test]
    fn retry_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(tmp.path());

        // Another kind of failure: skipping the copy is refused, untried.
        let slot = StartupSlot::failed(StartupFailure::new(
            &paths,
            &AppError::InvalidSettings("bad".into()),
        ));
        let mut called = false;
        let refused = slot.retry(true, |_, _| -> AppResult<()> {
            called = true;
            Ok(())
        });
        assert!(refused.is_err() && !called);

        // The copy-failed state: the override reaches this attempt only.
        let slot = StartupSlot::failed(copy_failed(&paths));
        let mut seen = Vec::new();
        let again = slot
            .retry(false, |_, opts| -> AppResult<()> {
                seen.push(opts.skip_pre_migration_copy);
                Err(AppError::UpgradeCopyFailed(CopyFailure::Locked))
            })
            .unwrap();
        assert!(matches!(again, Retry::Failed(f) if f.problem == StartupProblem::UpgradeCopy));
        let started = slot
            .retry(true, |_, opts| -> AppResult<()> {
                seen.push(opts.skip_pre_migration_copy);
                Ok(())
            })
            .unwrap();
        assert!(matches!(started, Retry::Started(())));
        assert_eq!(seen, [false, true], "never carried over between attempts");
        assert!(slot.get().is_none(), "started: no failure left");
        assert!(slot
            .retry(false, |_, _| -> AppResult<()> { Ok(()) })
            .is_err());
    }
}
