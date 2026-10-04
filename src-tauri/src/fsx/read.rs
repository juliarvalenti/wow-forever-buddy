use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::error::{AppError, AppResult};

const READ_ATTEMPTS: u32 = 3;
const READ_BACKOFF: Duration = Duration::from_millis(250);

/// Reads a game file without ever getting in WoW's way (spec §3): opened with
/// full sharing, read to the end, closed immediately. If the file changed
/// while we read it (size or mtime differ before/after, or the byte count is
/// off), retries a few times, then gives up with `Unstable` ("try later").
///
/// This is the only way game files are read; nothing streams from or holds a
/// handle to them.
pub fn safe_read(path: &Path) -> AppResult<Vec<u8>> {
    read_stable(path, READ_ATTEMPTS, READ_BACKOFF, &mut |_| {})
}

fn read_stable(
    path: &Path,
    attempts: u32,
    backoff: Duration,
    after_read: &mut dyn FnMut(u32),
) -> AppResult<Vec<u8>> {
    for attempt in 0..attempts {
        if let Some(bytes) = read_once(path, attempt, after_read)? {
            return Ok(bytes);
        }
        if attempt + 1 < attempts {
            std::thread::sleep(backoff);
        }
    }
    Err(AppError::Unstable(path.display().to_string()))
}

/// One attempt: `Some(bytes)` if the file held still while we read it,
/// `None` if it's worth trying again.
fn read_once(
    path: &Path,
    attempt: u32,
    after_read: &mut dyn FnMut(u32),
) -> AppResult<Option<Vec<u8>>> {
    let Some(before) = retryable(path, stamp(path))? else {
        return Ok(None);
    };
    let read = open_shared(path).and_then(|mut file| {
        let mut buf = Vec::with_capacity(before.0 as usize);
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }); // handle dropped here, before anything else happens
    let bytes = retryable(path, read)?;
    after_read(attempt);
    let Some(after) = retryable(path, stamp(path))? else {
        return Ok(None);
    };
    Ok(bytes.filter(|b| before == after && b.len() as u64 == after.0))
}

/// Sorts I/O errors: someone holding the file without read sharing (a backup
/// tool, antivirus, WoW mid-save) means "try again" (`None`); anything else
/// is a real error.
fn retryable<T>(path: &Path, result: std::io::Result<T>) -> AppResult<Option<T>> {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e)
            if cfg!(windows)
                && matches!(
                    e.raw_os_error(),
                    Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
                ) =>
        {
            Ok(None)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(AppError::NotFound(path.display().to_string()))
        }
        Err(e) => Err(e.into()),
    }
}

fn stamp(path: &Path) -> std::io::Result<(u64, SystemTime)> {
    let meta = std::fs::metadata(path)?;
    Ok((meta.len(), meta.modified()?))
}

/// Read-only open that lets everyone else keep reading, writing, renaming and
/// deleting the file while we have it. Rust's std already uses these share
/// flags by default on Windows; they're spelled out so a refactor can't drop them.
pub(crate) fn open_shared(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_WRITE: u32 = 0x2;
        const FILE_SHARE_DELETE: u32 = 0x4;
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_whole_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Foo.lua");
        std::fs::write(&path, b"FooDB = {}\n").unwrap();
        assert_eq!(safe_read(&path).unwrap(), b"FooDB = {}\n");
    }

    #[test]
    fn missing_file_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(
            safe_read(&tmp.path().join("nope.lua")),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn retries_a_torn_read_then_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Foo.lua");
        std::fs::write(&path, b"half").unwrap();

        // Simulate WoW finishing its save right after our first read.
        let mut hook = |attempt: u32| {
            if attempt == 0 {
                std::fs::write(&path, b"the whole saved file").unwrap();
            }
        };
        let bytes = read_stable(&path, 3, Duration::ZERO, &mut hook).unwrap();
        assert_eq!(bytes, b"the whole saved file");
    }

    #[test]
    fn gives_up_as_unstable_if_it_keeps_changing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Foo.lua");
        std::fs::write(&path, b"x").unwrap();

        let mut grow = |attempt: u32| {
            std::fs::write(&path, vec![b'x'; attempt as usize + 2]).unwrap();
        };
        assert!(matches!(
            read_stable(&path, 3, Duration::ZERO, &mut grow),
            Err(AppError::Unstable(_))
        ));
    }

    /// The point of the share flags: while our handle is open, WoW can still
    /// write, rename over, and delete the file.
    #[cfg(windows)]
    #[test]
    fn open_handle_does_not_block_writers_renamers_or_deleters() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Foo.lua");
        std::fs::write(&path, b"old").unwrap();

        let _ours = open_shared(&path).unwrap();

        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("writer blocked by our read handle");
        let replacement = tmp.path().join("Foo.lua.new");
        std::fs::write(&replacement, b"new").unwrap();
        std::fs::rename(&replacement, &path).expect("rename blocked by our read handle");
        std::fs::remove_file(&path).expect("delete blocked by our read handle");
    }

    /// Someone else holding the file without read sharing is "try later",
    /// not a hard I/O error, and we recover once they let go.
    #[cfg(windows)]
    #[test]
    fn exclusive_holder_means_unstable_then_recovers() {
        use std::os::windows::fs::OpenOptionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Foo.lua");
        std::fs::write(&path, b"data").unwrap();

        let exclusive = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(matches!(
            read_stable(&path, 2, Duration::ZERO, &mut |_| {}),
            Err(AppError::Unstable(_))
        ));
        drop(exclusive);
        assert_eq!(safe_read(&path).unwrap(), b"data");
    }
}
