use std::io::Write;
use std::path::Path;

use crate::error::{AppError, AppResult};

/// Replace `path` with `bytes` atomically: temp file in the same directory,
/// fsync, then rename over the target. A crash leaves either the old file or
/// the new one, never a half-written file.
///
/// Unguarded: only for files the app owns (settings, manifests, blobs).
/// Game files must go through the guarded writer, which checks that WoW isn't
/// running and snapshots first.
pub fn atomic_replace(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let dir = path
        .parent()
        .ok_or_else(|| AppError::Io(format!("no parent directory: {}", path.display())))?;
    std::fs::create_dir_all(dir)?;

    let mut tmp = tempfile::Builder::new()
        .prefix(".wfb-tmp-")
        .tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)
        .map_err(|e| AppError::Io(e.error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_replaces_without_leftovers() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("nested").join("file.json");

        atomic_replace(&target, b"one").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"one");

        atomic_replace(&target, b"two").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"two");

        let entries: Vec<_> = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("file.json")]);
    }
}
