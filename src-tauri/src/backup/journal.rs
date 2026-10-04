//! The restore journal (spec §5): written after the pre-restore snapshot and
//! before the first file changes, removed when the restore completes. Found
//! at startup, it means a restore was interrupted, and the app offers to roll
//! back or finish it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backup::restore::{RestoreMode, RestoreSelection};
use crate::error::{AppError, AppResult};
use crate::fsx::atomic::atomic_replace;

pub const FILE_NAME: &str = "restore-journal.json";

/// An interrupted restore: enough to roll it back or run it again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Journal {
    /// The snapshot the user chose to restore.
    pub source_snapshot: String,
    /// Taken before the user's restore changed anything. Rolling back always
    /// restores this one, even after an interrupted recovery.
    pub original_pre_restore: String,
    /// Taken before the latest attempt (the restore, or a recovery of it).
    /// Equal to `original_pre_restore` on the first attempt.
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

/// Snapshots an unresolved restore still needs, so pruning keeps them (T8).
/// `Err` if the journal is unreadable: then the caller must not prune at all,
/// since it can't tell which snapshots a roll back would need.
pub fn held_snapshots(dir: &Path) -> AppResult<HashSet<String>> {
    Ok(read(dir)?
        .map(|j| {
            HashSet::from([
                j.source_snapshot,
                j.original_pre_restore,
                j.pre_restore_snapshot,
            ])
        })
        .unwrap_or_default())
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

/// What the UI shows about interrupted restores.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoveryStatus {
    /// No interrupted restore.
    None,
    /// Roll back, finish or leave as is.
    Pending { journal: Journal },
    /// A journal exists but can't be read, so roll back and finish aren't
    /// possible. `latest_safety` is the newest pre-restore snapshot, for
    /// "Open the safety copy"; clearing the notice discards the journal.
    Unreadable {
        error: String,
        latest_safety: Option<String>,
    },
}

/// The recovery status of `dir`. `latest_safety` is only asked when the
/// journal is unreadable.
pub fn status(dir: &Path, latest_safety: impl FnOnce() -> Option<String>) -> RecoveryStatus {
    match read(dir) {
        Ok(None) => RecoveryStatus::None,
        Ok(Some(journal)) => RecoveryStatus::Pending { journal },
        Err(e) => RecoveryStatus::Unreadable {
            error: e.to_string(),
            latest_safety: latest_safety(),
        },
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
            original_pre_restore: "B".into(),
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
        assert!(held_snapshots(tmp.path()).is_err(), "so nothing is pruned");
    }

    #[test]
    fn held_snapshots_are_the_journals_three_ids() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(held_snapshots(tmp.path()).unwrap().is_empty());
        let journal = Journal {
            source_snapshot: "A".into(),
            original_pre_restore: "B".into(),
            pre_restore_snapshot: "C".into(),
            selection: RestoreSelection {
                items: vec![ScopeItem::Everything],
            },
            mode: RestoreMode::Overlay,
            flavor: "_classic_beta_".into(),
            started_at: "2026-10-04T00:00:00Z".into(),
            summary: "everything".into(),
        };
        write(tmp.path(), &journal).unwrap();
        let held = held_snapshots(tmp.path()).unwrap();
        assert_eq!(held, HashSet::from(["A".into(), "B".into(), "C".into()]));
    }

    #[test]
    fn status_covers_none_pending_and_unreadable() {
        let tmp = tempfile::tempdir().unwrap();
        let never = || -> Option<String> { panic!("only asked when unreadable") };
        assert_eq!(status(tmp.path(), never), RecoveryStatus::None);

        let journal = Journal {
            source_snapshot: "A".into(),
            original_pre_restore: "B".into(),
            pre_restore_snapshot: "B".into(),
            selection: RestoreSelection {
                items: vec![ScopeItem::Everything],
            },
            mode: RestoreMode::Overlay,
            flavor: "_classic_beta_".into(),
            started_at: "2026-10-04T00:00:00Z".into(),
            summary: "macros".into(),
        };
        write(tmp.path(), &journal).unwrap();
        assert_eq!(
            status(tmp.path(), never),
            RecoveryStatus::Pending {
                journal: journal.clone()
            }
        );

        std::fs::write(tmp.path().join(FILE_NAME), b"{ damaged").unwrap();
        let status = status(tmp.path(), || Some("SAFE".into()));
        assert!(matches!(
            &status,
            RecoveryStatus::Unreadable { latest_safety: Some(id), .. } if id == "SAFE"
        ));
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["kind"], "unreadable");
    }
}
