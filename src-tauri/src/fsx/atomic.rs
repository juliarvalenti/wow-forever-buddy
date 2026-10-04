use std::io::Write;
use std::path::{Path, PathBuf};
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
/// - The temp file is created by us, not `tempfile`, because `tempfile`
///   marks files `FILE_ATTRIBUTE_TEMPORARY` on Windows and the replaced file
///   would keep that attribute (game files included).
/// - Windows rename: `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`. If that
///   fails with access denied (e.g. a reader holds the target open with
///   delete sharing, as our own `safe_read` does), fall back to
///   `std::fs::rename`, which retries with POSIX semantics
///   (`FileRenameInfoEx`, `REPLACE_IF_EXISTS | POSIX_SEMANTICS`).
/// - A read-only target is refused, never overwritten: players mark
///   `Config.wtf` read-only to pin their settings.
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
    if std::fs::metadata(path).is_ok_and(|m| m.permissions().readonly()) {
        return Err(AppError::Io(format!(
            "{} is read-only; clear the read-only flag to let the app change it",
            path.display()
        )));
    }
    std::fs::create_dir_all(dir)?;

    let tmp = TempFile::create(dir, &name)?;
    {
        let mut file = std::fs::OpenOptions::new().write(true).open(&tmp.path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    } // closed before the rename

    let mut delays = delays.iter();
    loop {
        match replace_file(&tmp.path, path) {
            Ok(()) => {
                tmp.disarm();
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

/// A temp file next to its target, deleted on drop unless disarmed.
struct TempFile {
    path: PathBuf,
    armed: bool,
}

impl TempFile {
    fn create(dir: &Path, name: &str) -> std::io::Result<Self> {
        use std::hash::{BuildHasher, Hasher};
        for attempt in 0u32.. {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u32(attempt);
            h.write_u32(std::process::id());
            let path = dir.join(format!(".{name}{TMP_MARKER}{:016x}", h.finish()));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path, armed: true }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 16 => {}
                Err(e) => return Err(e),
            }
        }
        unreachable!("the loop returns")
    }

    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    const ERROR_ACCESS_DENIED: i32 = 5;

    let (from_w, to_w) = (verbatim_wide(from)?, verbatim_wide(to)?);
    // SAFETY: both are NUL-terminated UTF-16 strings that outlive the call.
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok != 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
        return std::fs::rename(from, to);
    }
    Err(err)
}

/// `\\?\`-prefixed, NUL-terminated UTF-16 path, so raw Win32 calls handle
/// paths longer than MAX_PATH the way std does.
#[cfg(windows)]
fn verbatim_wide(path: &Path) -> std::io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let absolute = std::path::absolute(path)?;
    let plain: Vec<u16> = absolute
        .as_os_str()
        .encode_wide()
        .map(|c| {
            if c == u16::from(b'/') {
                u16::from(b'\\')
            } else {
                c
            }
        })
        .collect();
    let wide = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    let mut out = if plain.starts_with(&wide(r"\\?\")) {
        plain
    } else if plain.starts_with(&wide(r"\\")) {
        let mut v = wide(r"\\?\UNC\");
        v.extend_from_slice(&plain[2..]);
        v
    } else {
        let mut v = wide(r"\\?\");
        v.extend(plain);
        v
    };
    out.push(0);
    Ok(out)
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

    /// Players mark Config.wtf read-only to pin settings; we never override that.
    #[test]
    fn read_only_target_is_refused_and_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("Config.wtf");
        std::fs::write(&target, b"SET pinned 1").unwrap();
        let mut perms = std::fs::metadata(&target).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&target, perms).unwrap();

        let err = replace_with_retry(&target, b"SET pinned 0", &[]).unwrap_err();
        assert!(
            matches!(err, AppError::Io(ref m) if m.contains("read-only")),
            "{err:?}"
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"SET pinned 1");
        assert_eq!(names_in(tmp.path()), ["Config.wtf"], "no temp left");

        let mut perms = std::fs::metadata(&target).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&target, perms).unwrap();
    }

    /// The replaced file must look like a normal file: not TEMPORARY (which
    /// tells Windows not to write it back and makes some backup/sync tools
    /// skip it) and not hidden, even though its temp name started with a dot.
    #[cfg(windows)]
    #[test]
    fn replaced_files_have_normal_attributes() {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_TEMPORARY: u32 = 0x100;

        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("SavedVariables.lua");
        for content in [&b"new"[..], b"replaced"] {
            atomic_replace(&target, content).unwrap();
            let attrs = std::fs::metadata(&target).unwrap().file_attributes();
            assert_eq!(
                attrs & FILE_ATTRIBUTE_TEMPORARY,
                0,
                "TEMPORARY set: {attrs:#x}"
            );
            assert_eq!(attrs & FILE_ATTRIBUTE_HIDDEN, 0, "HIDDEN set: {attrs:#x}");
        }
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
