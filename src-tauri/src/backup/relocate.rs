//! Moving the backup store to another folder (F1, Settings › Store backups
//! in). `AppCore::move_backups` runs it as a job: copy, check, switch the
//! setting, then remove the old copy. Until the switch the old store is
//! untouched and still the one in use, so a failure at any step leaves the
//! backups where they were.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::backup::Progress;
use crate::config::settings::{is_within, resolve_existing};
use crate::error::{AppError, AppResult};

/// What a move did, for the Settings screen.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct MoveReport {
    /// The store's folder now.
    pub dir: String,
    pub files: u32,
    pub bytes: f64,
    /// The old folder, if removing it failed after the move. The backups
    /// are safe in the new one; this is a leftover copy to delete by hand.
    pub left_behind: Option<String>,
}

/// The same folder, however it's spelled (links, `..`, case on Windows).
pub fn same_dir(a: &Path, b: &Path) -> bool {
    let (a, b) = (resolve_existing(a), resolve_existing(b));
    is_within(&a, &b) && is_within(&b, &a)
}

/// Copies the store at `src` into `dst`, which must be missing or empty.
/// Each file is written, flushed to disk and read back to compare. The copy
/// is built in `<dst>.moving` and renamed into place at the end, so `dst`
/// never holds half a store. `staging/` (in-progress writes) is skipped.
/// `progress` gets (files done, files in all). Returns the files and bytes
/// copied.
pub fn copy_store(src: &Path, dst: &Path, progress: Progress<'_>) -> AppResult<(u32, u64)> {
    let (rs, rd) = (resolve_existing(src), resolve_existing(dst));
    if is_within(&rd, &rs) || is_within(&rs, &rd) {
        return Err(AppError::InvalidSettings(format!(
            "the new backup folder can't be inside the current one, or the other way round: {}",
            dst.display()
        )));
    }
    if dst.exists() && fs::read_dir(dst)?.next().is_some() {
        return Err(AppError::InvalidSettings(format!(
            "{} already has files in it. Pick another folder, or empty that one first.",
            dst.display()
        )));
    }

    let tmp = moving_dir(dst);
    // Only ever our own leftover, from a move that was interrupted.
    if tmp.exists() {
        fs::remove_dir_all(&tmp)?;
    }
    fs::create_dir_all(&tmp)?;
    let mut copy = Copy {
        files: 0,
        bytes: 0,
        total: count_files(src, true)?,
        progress,
    };
    let result = copy_dir(src, &tmp, true, &mut copy).and_then(|()| {
        if dst.exists() {
            fs::remove_dir(dst)?; // empty, checked above
        }
        fs::rename(&tmp, dst)?;
        Ok(())
    });
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    Ok((copy.files, copy.bytes))
}

struct Copy<'a> {
    files: u32,
    bytes: u64,
    total: u32,
    progress: Progress<'a>,
}

/// The files `copy_dir` will copy, for the progress bar.
fn count_files(dir: &Path, top: bool) -> AppResult<u32> {
    let mut n = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if top && entry.file_name() == "staging" {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            n += count_files(&entry.path(), false)?;
        } else {
            n += 1;
        }
    }
    Ok(n)
}

fn moving_dir(dst: &Path) -> PathBuf {
    let mut name = dst.file_name().unwrap_or_default().to_os_string();
    name.push(".moving");
    dst.with_file_name(name)
}

fn copy_dir(src: &Path, dst: &Path, top: bool, copy: &mut Copy<'_>) -> AppResult<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if top && name == "staging" {
            continue;
        }
        let (from, to) = (entry.path(), dst.join(&name));
        let kind = entry.file_type()?;
        if kind.is_dir() {
            fs::create_dir(&to)?;
            copy_dir(&from, &to, false, copy)?;
        } else if kind.is_file() {
            let bytes = fs::read(&from)?;
            {
                // File::create opens for writing, which sync_all needs on Windows.
                let mut f = fs::File::create(&to)?;
                f.write_all(&bytes)?;
                f.sync_all()?;
            }
            if fs::read(&to)? != bytes {
                return Err(AppError::Io(format!(
                    "the copy of {} didn't read back the same; nothing was moved",
                    from.display()
                )));
            }
            copy.files += 1;
            copy.bytes += bytes.len() as u64;
            (copy.progress)(copy.files, copy.total);
        } else {
            // The store never makes links; one here isn't ours to follow.
            return Err(AppError::Io(format!(
                "unexpected link in the backup folder, so it wasn't moved: {}",
                from.display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &Path) {
        fs::create_dir_all(dir.join("objects/ab")).unwrap();
        fs::create_dir_all(dir.join("manifests")).unwrap();
        fs::create_dir_all(dir.join("staging")).unwrap();
        fs::write(dir.join("objects/ab/cdef"), b"blob").unwrap();
        fs::write(dir.join("manifests/S1.json"), b"{}").unwrap();
        fs::write(dir.join("staging/half"), b"in progress").unwrap();
    }

    #[test]
    fn copies_everything_but_staging() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("old"), tmp.path().join("new"));
        store(&src);

        let mut seen = Vec::new();
        let copied = copy_store(&src, &dst, &mut |done, total| seen.push((done, total))).unwrap();
        assert_eq!(copied, (2, 6));
        assert_eq!(seen, vec![(1, 2), (2, 2)], "staging isn't counted either");
        assert_eq!(fs::read(dst.join("objects/ab/cdef")).unwrap(), b"blob");
        assert_eq!(fs::read(dst.join("manifests/S1.json")).unwrap(), b"{}");
        assert!(!dst.join("staging").exists());
        assert!(!tmp.path().join("new.moving").exists());
        assert!(
            src.join("objects/ab/cdef").exists(),
            "the source is left alone"
        );
    }

    #[test]
    fn an_empty_target_is_fine_a_used_one_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("old");
        store(&src);

        let empty = tmp.path().join("empty");
        fs::create_dir(&empty).unwrap();
        copy_store(&src, &empty, &mut |_, _| {}).unwrap();

        let used = tmp.path().join("used");
        fs::create_dir(&used).unwrap();
        fs::write(used.join("mine.txt"), b"someone's file").unwrap();
        let err = copy_store(&src, &used, &mut |_, _| {}).unwrap_err();
        assert!(matches!(err, AppError::InvalidSettings(_)), "{err:?}");
        assert_eq!(fs::read_dir(&used).unwrap().count(), 1, "nothing added");
    }

    #[test]
    fn refuses_a_target_inside_the_store() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("old");
        store(&src);
        let err = copy_store(&src, &src.join("inner"), &mut |_, _| {}).unwrap_err();
        assert!(matches!(err, AppError::InvalidSettings(_)), "{err:?}");
        assert!(!src.join("inner").exists());
    }

    #[test]
    fn clears_a_leftover_from_an_interrupted_move() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("old"), tmp.path().join("new"));
        store(&src);
        fs::create_dir_all(tmp.path().join("new.moving/objects")).unwrap();
        fs::write(tmp.path().join("new.moving/objects/stale"), b"x").unwrap();

        copy_store(&src, &dst, &mut |_, _| {}).unwrap();
        assert!(!dst.join("objects/stale").exists());
    }

    #[test]
    fn same_dir_sees_through_spelling() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        fs::create_dir(&a).unwrap();
        assert!(same_dir(&a, &tmp.path().join("a/../a")));
        assert!(!same_dir(&a, tmp.path()));
    }
}
