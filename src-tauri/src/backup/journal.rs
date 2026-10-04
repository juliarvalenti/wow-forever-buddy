//! The restore journal (spec §5): written after the pre-restore snapshot and
//! before the first file changes, removed when the restore completes. Found
//! at startup, it means a restore was interrupted, and the app offers to roll
//! back or finish it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backup::restore::{RestoreMode, RestoreSelection};
use crate::error::{AppError, AppResult};
use crate::fsx::atomic::atomic_replace;

pub const FILE_NAME: &str = "restore-journal.json";

/// An interrupted restore: enough to roll it back or run it again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Journal {
    /// The snapshot being restored.
    pub source_snapshot: String,
    /// Taken before anything changed; rolling back restores it.
    pub pre_restore_snapshot: String,
    pub selection: RestoreSelection,
    pub mode: RestoreMode,
    pub flavor: String,
    pub started_at: String,
    /// The restore's one-line summary, for the recovery prompt.
    pub summary: String,
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// The journal left by an interrupted restore, if any.
pub fn read(dir: &Path) -> AppResult<Option<Journal>> {
    match std::fs::read(path(dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| AppError::Io(format!("restore journal is unreadable: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn write(dir: &Path, journal: &Journal) -> AppResult<()> {
    let bytes = serde_json::to_vec_pretty(journal).expect("journal serializes");
    atomic_replace(&path(dir), &bytes)
}

pub fn clear(dir: &Path) -> AppResult<()> {
    match std::fs::remove_file(path(dir)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::restore::ScopeItem;

    #[test]
    fn write_read_clear() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read(tmp.path()).unwrap(), None);
        let journal = Journal {
            source_snapshot: "A".into(),
            pre_restore_snapshot: "B".into(),
            selection: RestoreSelection {
                items: vec![ScopeItem::Everything],
            },
            mode: RestoreMode::Mirror,
            flavor: "_classic_beta_".into(),
            started_at: "2026-10-04T00:00:00Z".into(),
            summary: "keybindings".into(),
        };
        write(tmp.path(), &journal).unwrap();
        assert_eq!(read(tmp.path()).unwrap(), Some(journal));
        clear(tmp.path()).unwrap();
        assert_eq!(read(tmp.path()).unwrap(), None);
        clear(tmp.path()).unwrap();
    }

    #[test]
    fn a_damaged_journal_is_an_error_not_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(FILE_NAME), b"{ not json").unwrap();
        assert!(read(tmp.path()).is_err());
    }
}
