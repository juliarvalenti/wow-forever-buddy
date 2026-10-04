use std::io::Write;
use std::path::Path;
use std::time::Duration;

use crate::error::{AppError, AppResult};

/// Marker in every temp file name, so leftovers from a crash can be found and swept.
const TMP_MARKER: &str = ".wfb-tmp-";

/// Waits between rename attempts when Windows reports the target as in use
/// (antivirus, the search indexer, another reader without delete sharing).
const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(400),
    Duration::from_millis(800),
    Duration::from_millis(1600),
];

/// Replace `path` with `bytes` atomically: temp file in the same directory
/// (`.<name>.wfb-tmp-<rand>`), fsync, rename over the target, then flush the
/// directory so the rename itself is on disk. A crash leaves either the old
/// file or the new one, never a half-written file. A failed write never
/// leaves its temp file behind.
///
/// The rename is `std::fs::rename`. On Windows that's
/// `MoveFileExW(REPLACE_EXISTING)` first; only if that fails with access
/// denied (e.g. a reader holds the target open with delete sharing, as our
/// own `safe_read` does) does std retry with POSIX semantics
/// (`FileRenameInfoEx`), which also ignores the read-only attribute. So a
/// read-only target is replaced, not refused; callers that want to honor
/// read-only files must check before writing.
///
/// This does no safety checks of its own. App-owned files (settings,
/// manifests, blobs) call it directly; game files go through the guarded
/// writer, which checks WoW isn't running and snapshots first.
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> AppResult<()> {
    replace_with_retry(path, bytes, &RETRY_DELAYS)
}

fn replace_with_retry(path: &Path, bytes: &[u8], delays: &[Duration]) -> AppResult<()> {
    let dir = path
        .parent()
        .ok_or_else(|| AppError::Io(format!("no parent directory: {}", path.display())))?;
    let name = path
        .file_name()
        .ok_or_else(|| AppError::Io(format!("no file name: {}", path.display())))?
        .to_string_lossy();
    std::fs::create_dir_all(dir)?;

    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{name}{TMP_MARKER}"))
        .tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    // Close our handle before renaming; the temp file is still deleted if we
    // return early.
    let tmp = tmp.into_temp_path();

    let mut delays = delays.iter();
    loop {
        match std::fs::rename(&tmp, path) {
            Ok(()) => {
                // It's been renamed away; nothing left to delete.
                let _ = tmp.keep();
                flush_dir(dir)?;
                return Ok(());
            }
            Err(e) => match delays.next() {
                Some(delay) if is_transient(&e) => std::thread::sleep(*delay),
                // Dropping `tmp` deletes the temp file.
                _ => return Err(e.into()),
            },
        }
    }
}

/// Makes a rename in `dir` durable by flushing the directory entry.
#[cfg(not(windows))]
fn flush_dir(dir: &Path) -> AppResult<()> {
    std::fs::File::open(dir)?.sync_all()?;
    Ok(())
}

/// Windows: NTFS journals the rename's metadata, and flushing a directory
/// handle asks it to commit that now. Best effort, because some file systems
/// and network shares refuse a directory handle with write access. The data
/// itself was already fsynced before the rename.
#[cfg(windows)]
fn flush_dir(dir: &Path) -> AppResult<()> {
    let _ = flush_dir_handle(dir);
    Ok(())
}

/// Opens `dir` itself (FILE_FLAG_BACKUP_SEMANTICS) with write access and
/// calls FlushFileBuffers on it. Separate from `flush_dir` so tests can prove
/// it really succeeds on NTFS instead of being silently skipped.
#[cfg(windows)]
fn flush_dir_handle(dir: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(dir)?
        .sync_all()
}

/// Errors that mean "someone briefly has the file open", worth retrying.
fn is_transient(e: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        const ERROR_ACCESS_DENIED: i32 = 5;
        const ERROR_SHARING_VIOLATION: i32 = 32;
        const ERROR_LOCK_VIOLATION: i32 = 33;
        matches!(
            e.raw_os_error(),
            Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
        )
    }
    #[cfg(not(windows))]
    {
        let _ = e;
        false
    }
}

/// Deletes temp files left by a crash mid-write anywhere under `root`.
/// Doesn't follow symlinks. Returns how many were removed.
pub fn sweep_temp_files(root: &Path) -> AppResult<usize> {
    sweep(root, usize::MAX)
}

/// Like `sweep_temp_files`, but only `root` itself, not subfolders. For
/// folders with big trees under them (the backup store), which handle their
/// own in-progress files.
pub fn sweep_temp_files_shallow(root: &Path) -> AppResult<usize> {
    sweep(root, 1)
}

fn sweep(root: &Path, max_depth: usize) -> AppResult<usize> {
    if !root.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(max_depth)
    {
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_file()
            && entry.file_name().to_string_lossy().contains(TMP_MARKER)
            && std::fs::remove_file(entry.path()).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn creates_and_replaces_without_leftovers() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("nested").join("file.json");

        atomic_replace(&target, b"one").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"one");

        atomic_replace(&target, b"two").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"two");

        assert_eq!(names_in(target.parent().unwrap()), ["file.json"]);
    }

    #[test]
    fn failed_write_leaves_target_and_no_temp() {
        let tmp = tempfile::tempdir().unwrap();
        // Renaming a file over a non-empty directory fails on every platform.
        let target = tmp.path().join("occupied");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), b"x").unwrap();

        assert!(replace_with_retry(&target, b"new", &[]).is_err());
        assert_eq!(names_in(tmp.path()), ["occupied"]);
        assert!(target.join("keep").exists());
    }

    #[test]
    fn sweep_removes_only_temp_files() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("WTF/Account/X/SavedVariables");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("Foo.lua"), b"keep").unwrap();
        std::fs::write(deep.join(".Foo.lua.wfb-tmp-abc123"), b"junk").unwrap();
        std::fs::write(tmp.path().join(".settings.json.wfb-tmp-zz"), b"junk").unwrap();

        assert_eq!(sweep_temp_files(tmp.path()).unwrap(), 2);
        assert_eq!(names_in(&deep), ["Foo.lua"]);
        assert_eq!(sweep_temp_files(&tmp.path().join("missing")).unwrap(), 0);
    }

    #[test]
    fn shallow_sweep_leaves_subfolders_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("backups/objects/ab");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join(".blob.wfb-tmp-1"), b"in progress").unwrap();
        std::fs::write(tmp.path().join(".buddy.db.wfb-tmp-2"), b"junk").unwrap();

        assert_eq!(sweep_temp_files_shallow(tmp.path()).unwrap(), 1);
        assert!(nested.join(".blob.wfb-tmp-1").exists());
    }

    /// WoW (or our own reader) holding the file open with full sharing must
    /// not stop a replace.
    #[cfg(windows)]
    #[test]
    fn replace_succeeds_while_a_sharing_reader_has_it_open() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("Foo.lua");
        std::fs::write(&target, b"old").unwrap();

        let _reader = crate::fsx::read::open_shared(&target).unwrap();
        atomic_replace(&target, b"new").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    /// A handle that refuses all sharing blocks the rename: we retry, then
    /// fail cleanly with the old file intact and no temp file left behind.
    #[cfg(windows)]
    #[test]
    fn exclusive_lock_retries_then_fails_cleanly() {
        use std::os::windows::fs::OpenOptionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("Foo.lua");
        std::fs::write(&target, b"old").unwrap();

        let exclusive = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        let delays = [Duration::from_millis(10); 3];
        let started = std::time::Instant::now();
        let result = replace_with_retry(&target, b"new", &delays);
        assert!(result.is_err());
        assert!(
            started.elapsed() >= Duration::from_millis(30),
            "should have retried"
        );
        drop(exclusive);

        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert_eq!(names_in(tmp.path()), ["Foo.lua"]);
    }

    /// The rename's durability rests on this: on NTFS, opening the directory
    /// for write with backup semantics and flushing it must actually succeed
    /// (not be skipped by the best-effort wrapper).
    #[cfg(windows)]
    #[test]
    fn directory_flush_succeeds_on_ntfs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("probe"), b"x").unwrap();
        flush_dir_handle(tmp.path()).expect("FlushFileBuffers on a directory handle");

        // And it runs as part of a real replace (also on a long path).
        let target = tmp.path().join("Config.wtf");
        atomic_replace(&target, b"SET a 1").unwrap();
        flush_dir_handle(target.parent().unwrap()).unwrap();
    }

    /// Deep WTF trees plus long realm/character/addon names can pass MAX_PATH.
    #[cfg(windows)]
    #[test]
    fn long_paths_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut dir = tmp.path().to_path_buf();
        while dir.as_os_str().len() < 300 {
            dir.push("a_rather_long_folder_name_for_testing");
        }
        let target = dir.join("SavedVariables.lua");

        atomic_replace(&target, b"deep").unwrap();
        assert_eq!(crate::fsx::read::safe_read(&target).unwrap(), b"deep");
        assert_eq!(sweep_temp_files(tmp.path()).unwrap(), 0);
    }
}
