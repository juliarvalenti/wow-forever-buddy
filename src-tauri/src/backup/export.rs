//! "Export snapshot as .zip" (spec §5): a plain zip built from a manifest,
//! so a snapshot can leave the content-addressed store (another PC, a forum
//! post, a manual restore). Entries mirror the flavor folder (`WTF/...`,
//! `Interface/AddOns/...`), so extracting into the flavor folder puts every
//! file back where it was.

use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

use crate::backup::BackupService;
use crate::config::settings::{is_within, resolve_existing};
use crate::error::{AppError, AppResult};
use crate::fsx::relpath::RelPath;

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ExportReport {
    pub snapshot_id: String,
    /// Where the zip was saved (`.zip` is added if the name lacked it).
    pub path: String,
    pub files: u32,
    /// Size of the zip.
    pub bytes: f64,
}

/// Writes snapshot `id` to a zip at `dest`. `forbidden` are folders the zip
/// may not land in: the game folder (and its linked folders' targets),
/// which only the write gate may change, and the backup store.
///
/// Every file is read back and checked against its hash first; if any is
/// missing or corrupt nothing is written and the error lists them. The zip
/// is built in a temp file next to `dest` and moved into place at the end,
/// so a failed export never leaves a half-written zip (or replaces an
/// existing one).
pub fn export_zip(
    backups: &BackupService,
    id: &str,
    dest: &Path,
    forbidden: &[PathBuf],
    progress: &mut dyn FnMut(u32, u32),
) -> AppResult<ExportReport> {
    let dest = check_destination(dest, forbidden)?;
    let manifest = backups.manifest(id)?;
    let total = manifest.files.len() as u32;

    let parent = dest.parent().expect("checked: dest has a parent");
    let tmp = tempfile::Builder::new()
        .prefix(".wfb-export-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    let mut zip = zip::ZipWriter::new(tmp);
    let mut corrupt = Vec::new();
    for (i, f) in manifest.files.iter().enumerate() {
        // A manifest is ours, but it's also a file on disk: never let a
        // tampered one name an entry like `../../x` (zip slip).
        let name = RelPath::new(&f.path)?.as_string();
        let bytes = match backups.blobs().get(&f.blake3) {
            Ok(b) => b,
            Err(_) => {
                corrupt.push(f.path.clone());
                continue;
            }
        };
        let mut options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(bytes.len() as u64 >= u64::from(u32::MAX));
        if let Some(t) = zip_time(&f.mtime) {
            options = options.last_modified_time(t);
        }
        zip.start_file(name, options).map_err(zip_err)?;
        zip.write_all(&bytes)?;
        progress(i as u32 + 1, total);
    }
    if !corrupt.is_empty() {
        return Err(AppError::BackupCorrupt { files: corrupt });
    }

    let tmp = zip.finish().map_err(zip_err)?;
    tmp.as_file().sync_all()?;
    let file = tmp.persist(&dest).map_err(|e| AppError::from(e.error))?;
    Ok(ExportReport {
        snapshot_id: manifest.id,
        path: dest.to_string_lossy().into_owned(),
        files: total,
        bytes: file.metadata()?.len() as f64,
    })
}

/// `dest` as it will be written: absolute, ending in `.zip`, in a folder
/// that exists and isn't inside a forbidden one (compared after resolving
/// links, so an alias of the game folder can't sneak by).
fn check_destination(dest: &Path, forbidden: &[PathBuf]) -> AppResult<PathBuf> {
    let bad = |why: &str| AppError::BadDestination(format!("{why}: {}", dest.display()));
    if !dest.is_absolute() {
        return Err(bad("not an absolute path"));
    }
    let (Some(parent), Some(name)) = (dest.parent(), dest.file_name()) else {
        return Err(bad("not a file path"));
    };
    if !parent.is_dir() {
        return Err(bad("the folder doesn't exist"));
    }
    let mut dest = dest.to_path_buf();
    let is_zip = Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !is_zip {
        let mut name = name.to_os_string();
        name.push(".zip");
        dest.set_file_name(name);
    }
    if dest.is_dir() {
        return Err(bad("that's a folder"));
    }
    let resolved = resolve_existing(&dest);
    for root in forbidden {
        if is_within(&resolved, &resolve_existing(root)) {
            return Err(bad("inside the game or backup folder"));
        }
    }
    Ok(dest)
}

/// A manifest mtime (RFC 3339, UTC) as zip time. Zip times have no time
/// zone and every unzip tool reads them as local time, so convert to local.
fn zip_time(mtime: &str) -> Option<zip::DateTime> {
    let local = chrono::DateTime::parse_from_rfc3339(mtime)
        .ok()?
        .with_timezone(&chrono::Local);
    zip::DateTime::try_from(local.naive_local()).ok()
}

fn zip_err(e: zip::result::ZipError) -> AppError {
    AppError::Io(format!("zip: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::manifest::Trigger;
    use crate::backup::{SnapshotRequest, SnapshotScope};
    use crate::db::Db;
    use crate::fsx::relpath::GameRoot;
    use crate::test_support::fixture_copy;
    use std::io::Read;

    struct Setup {
        dir: tempfile::TempDir,
        flavor_dir: PathBuf,
        backups_dir: PathBuf,
        service: BackupService,
        id: String,
    }

    fn setup() -> Setup {
        let (dir, root) = fixture_copy();
        let flavor_dir = root.join("_classic_beta_");
        let backups_dir = dir.path().join("backups");
        let db = Db::open(&dir.path().join("buddy.db")).unwrap();
        let service = BackupService::open(&backups_dir, db).unwrap();
        let id = service
            .create(
                SnapshotRequest {
                    game: &GameRoot::new(&flavor_dir).unwrap(),
                    flavor: "_classic_beta_",
                    trigger: Trigger::Manual,
                    label: None,
                    scope: SnapshotScope::Full {
                        include_addons: false,
                    },
                    game_running: false,
                },
                &mut |_, _| {},
            )
            .unwrap()
            .unwrap()
            .id;
        Setup {
            dir,
            flavor_dir,
            backups_dir,
            service,
            id,
        }
    }

    fn out_dir(s: &Setup) -> PathBuf {
        let out = s.dir.path().join("exports");
        std::fs::create_dir_all(&out).unwrap();
        out
    }

    #[test]
    fn zip_holds_every_file_byte_for_byte() {
        let s = setup();
        let dest = out_dir(&s).join("my backup");
        let mut ticks = Vec::new();
        let report = export_zip(&s.service, &s.id, &dest, &[], &mut |d, t| {
            ticks.push((d, t))
        })
        .unwrap();

        assert!(report.path.ends_with("my backup.zip"), "adds .zip");
        let manifest = s.service.manifest(&s.id).unwrap();
        assert_eq!(report.files as usize, manifest.files.len());
        assert_eq!(ticks.last(), Some(&(report.files, report.files)));

        let mut zip = zip::ZipArchive::new(std::fs::File::open(&report.path).unwrap()).unwrap();
        assert_eq!(zip.len(), manifest.files.len());
        for f in &manifest.files {
            let mut entry = zip.by_name(&f.path).unwrap();
            let mut got = Vec::new();
            entry.read_to_end(&mut got).unwrap();
            assert_eq!(got, std::fs::read(s.flavor_dir.join(&f.path)).unwrap());
        }
        let leftovers: Vec<_> = std::fs::read_dir(out_dir(&s))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".wfb-export-"))
            .collect();
        assert!(leftovers.is_empty(), "no temp file left");
    }

    #[test]
    fn refuses_game_and_backup_folders_and_bad_paths() {
        let s = setup();
        let forbidden = [s.flavor_dir.clone(), s.backups_dir.clone()];
        for dest in [
            s.flavor_dir.join("WTF/export.zip"),
            s.backups_dir.join("export.zip"),
            PathBuf::from("relative.zip"),
            out_dir(&s).join("missing-folder/export.zip"),
        ] {
            let err = export_zip(&s.service, &s.id, &dest, &forbidden, &mut |_, _| {});
            assert!(
                matches!(err, Err(AppError::BadDestination(_))),
                "{dest:?}: {err:?}"
            );
        }
        assert!(!s.flavor_dir.join("WTF/export.zip").exists());
    }

    #[test]
    fn a_corrupt_blob_writes_nothing_and_keeps_the_old_zip() {
        let s = setup();
        let dest = out_dir(&s).join("export.zip");
        std::fs::write(&dest, b"previous export").unwrap();
        let manifest = s.service.manifest(&s.id).unwrap();
        let victim = &manifest.files[0];
        let h = &victim.blake3;
        let blob = s.backups_dir.join("objects").join(&h[..2]).join(&h[2..]);
        std::fs::write(&blob, b"garbage").unwrap();

        let err = export_zip(&s.service, &s.id, &dest, &[], &mut |_, _| {}).unwrap_err();
        match err {
            AppError::BackupCorrupt { files } => assert!(files.contains(&victim.path)),
            other => panic!("{other:?}"),
        }
        assert_eq!(std::fs::read(&dest).unwrap(), b"previous export");
    }

    #[test]
    fn a_tampered_manifest_cant_zip_slip() {
        let s = setup();
        let mut m = s.service.manifest(&s.id).unwrap();
        m.files[0].path = "../../evil.lua".into();
        s.service.manifests.write(&m).unwrap();
        let dest = out_dir(&s).join("export.zip");

        let err = export_zip(&s.service, &s.id, &dest, &[], &mut |_, _| {});
        assert!(matches!(err, Err(AppError::PathEscape(_))), "{err:?}");
        assert!(!dest.exists());
    }

    #[test]
    fn unknown_snapshot_is_not_found() {
        let s = setup();
        let dest = out_dir(&s).join("x.zip");
        assert!(matches!(
            export_zip(&s.service, "nope", &dest, &[], &mut |_, _| {}),
            Err(AppError::NotFound(_))
        ));
    }
}
