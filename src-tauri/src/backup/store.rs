//! Content-addressed blob store (spec §5): each distinct file content is
//! stored once, zstd-compressed, named by the blake3 hash of the
//! *uncompressed* bytes.
//!
//! ```text
//! <backups>/objects/ab/cdef0123…   blob
//! <backups>/staging/               in-progress writes; swept at startup
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::fsx::atomic::sweep_temp_files_shallow;

const ZSTD_LEVEL: i32 = 3;

/// Why a stored copy can't be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobFault {
    /// The blob file is gone.
    Missing,
    /// It's there but unreadable, doesn't decompress, or doesn't match its hash.
    Damaged,
}

pub struct BlobStore {
    objects: PathBuf,
    staging: PathBuf,
}

impl BlobStore {
    /// Opens the store under `backups_dir`, creating it if needed, and clears
    /// out anything a crash left in staging.
    pub fn open(backups_dir: &Path) -> AppResult<Self> {
        let store = Self {
            objects: backups_dir.join("objects"),
            staging: backups_dir.join("staging"),
        };
        std::fs::create_dir_all(&store.objects)?;
        std::fs::create_dir_all(&store.staging)?;
        sweep_temp_files_shallow(&store.staging)?;
        Ok(store)
    }

    pub fn hash(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    fn path_for(&self, hash: &str) -> AppResult<PathBuf> {
        let valid = hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit());
        if !valid {
            return Err(AppError::BackupCorrupt {
                files: vec![format!("bad blob id {hash:?}")],
                missing: Vec::new(),
            });
        }
        Ok(self.objects.join(&hash[..2]).join(&hash[2..]))
    }

    pub fn contains(&self, hash: &str) -> bool {
        self.path_for(hash).is_ok_and(|p| p.is_file())
    }

    /// Stores `bytes` (whose hash the caller computed) unless already there.
    /// Returns how many bytes were written to disk: 0 for a duplicate.
    pub fn put(&self, hash: &str, bytes: &[u8]) -> AppResult<u64> {
        let target = self.path_for(hash)?;
        if target.is_file() {
            return Ok(0);
        }
        let compressed = zstd::bulk::compress(bytes, ZSTD_LEVEL)?;

        // Write in staging, then rename into place: the blob appears whole
        // or not at all, and leftovers stay in the one folder we sweep.
        let mut tmp = tempfile::Builder::new()
            .prefix(".blob.wfb-tmp-")
            .tempfile_in(&self.staging)?;
        tmp.write_all(&compressed)?;
        tmp.as_file().sync_all()?;
        std::fs::create_dir_all(target.parent().expect("blob paths have a parent"))?;
        match tmp.persist_noclobber(&target) {
            Ok(_) => Ok(compressed.len() as u64),
            // Someone stored the same content in the meantime: fine.
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(0),
            Err(e) => Err(e.error.into()),
        }
    }

    /// Reads a blob back and verifies it against its hash.
    pub fn get(&self, hash: &str) -> AppResult<Vec<u8>> {
        self.read(hash).map_err(|fault| AppError::BackupCorrupt {
            files: vec![hash.to_string()],
            missing: match fault {
                BlobFault::Missing => vec![hash.to_string()],
                BlobFault::Damaged => Vec::new(),
            },
        })
    }

    /// Why a blob can't be used, if it can't: for "missing" vs
    /// "hash mismatch" in the damaged-snapshot dialog.
    pub fn check(&self, hash: &str) -> Option<BlobFault> {
        self.read(hash).err()
    }

    fn read(&self, hash: &str) -> Result<Vec<u8>, BlobFault> {
        let path = self.path_for(hash).map_err(|_| BlobFault::Damaged)?;
        let compressed = std::fs::read(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => BlobFault::Missing,
            _ => BlobFault::Damaged,
        })?;
        let bytes =
            zstd::stream::decode_all(compressed.as_slice()).map_err(|_| BlobFault::Damaged)?;
        if Self::hash(&bytes) != hash {
            return Err(BlobFault::Damaged);
        }
        Ok(bytes)
    }

    /// Deletes every blob not in `keep`. Returns (blobs removed, bytes freed).
    pub fn retain(&self, keep: &std::collections::HashSet<String>) -> AppResult<(u64, u64)> {
        let (mut removed, mut freed) = (0, 0);
        for entry in walkdir::WalkDir::new(&self.objects)
            .min_depth(2)
            .max_depth(2)
        {
            let Ok(entry) = entry else { continue };
            let prefix = entry
                .path()
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = entry.file_name().to_string_lossy();
            // Only ever delete files shaped like ours (`ab/<62 hex>`): a
            // stray file someone put under objects/ isn't ours to remove.
            // Lowercase only, exactly as `hash` writes them.
            let hex = |s: &str, len| {
                s.len() == len
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            };
            if !hex(&prefix, 2) || !hex(&name, 62) {
                continue;
            }
            let hash = format!("{prefix}{name}");
            if entry.file_type().is_file() && !keep.contains(&hash) {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if std::fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                    freed += size;
                }
            }
        }
        Ok((removed, freed))
    }

    /// Bytes the store actually occupies on disk.
    pub fn size_on_disk(&self) -> u64 {
        walkdir::WalkDir::new(&self.objects)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn stores_once_and_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BlobStore::open(tmp.path()).unwrap();
        let data = b"FooDB = { [\"a\"] = 1 }\n".repeat(100);
        let hash = BlobStore::hash(&data);

        assert!(!store.contains(&hash));
        let written = store.put(&hash, &data).unwrap();
        assert!(written > 0 && written < data.len() as u64, "compressed");
        assert!(store.contains(&hash));
        assert_eq!(store.put(&hash, &data).unwrap(), 0, "deduplicated");
        assert_eq!(store.get(&hash).unwrap(), data);
        assert_eq!(store.size_on_disk(), written);
    }

    #[test]
    fn detects_corruption() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BlobStore::open(tmp.path()).unwrap();
        let hash = BlobStore::hash(b"original");
        store.put(&hash, b"original").unwrap();

        // Swap the blob's content for different (valid zstd) data.
        let path = store.path_for(&hash).unwrap();
        std::fs::write(&path, zstd::bulk::compress(b"tampered", 3).unwrap()).unwrap();
        assert!(matches!(
            store.get(&hash),
            Err(AppError::BackupCorrupt { .. })
        ));

        std::fs::write(&path, b"not zstd").unwrap();
        assert!(store.get(&hash).is_err());
        assert!(store.get("../../etc/passwd").is_err(), "ids are validated");
    }

    #[test]
    fn retain_removes_unreferenced_blobs() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BlobStore::open(tmp.path()).unwrap();
        let keep = BlobStore::hash(b"keep");
        let drop = BlobStore::hash(b"drop");
        store.put(&keep, b"keep").unwrap();
        store.put(&drop, b"drop").unwrap();

        let (removed, freed) = store.retain(&HashSet::from([keep.clone()])).unwrap();
        assert_eq!(removed, 1);
        assert!(freed > 0);
        assert!(store.contains(&keep));
        assert!(!store.contains(&drop));
    }

    /// H1: GC only deletes files shaped like blobs; anything else under
    /// objects/ (a user's own folder that happened to be called that) stays.
    #[test]
    fn retain_only_touches_blob_shaped_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = BlobStore::open(tmp.path()).unwrap();
        let objects = tmp.path().join("objects");
        let strays = [
            objects.join("ab").join("notes.txt"),
            objects.join("photos").join("a".repeat(62)),
            objects.join("ab").join("a".repeat(61)),
            objects.join("zz").join("a".repeat(62)),
            objects.join("AB").join("A".repeat(62)), // our hashes are lowercase
        ];
        for p in &strays {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"mine").unwrap();
        }
        let orphan = BlobStore::hash(b"orphan");
        store.put(&orphan, b"orphan").unwrap();

        let (removed, _) = store.retain(&HashSet::new()).unwrap();
        assert_eq!(removed, 1, "only the real orphan blob");
        assert!(!store.contains(&orphan));
        for p in &strays {
            assert!(p.exists(), "{p:?} kept");
        }
    }

    #[test]
    fn open_sweeps_staging_leftovers() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("staging")).unwrap();
        let leftover = tmp.path().join("staging/.blob.wfb-tmp-x");
        std::fs::write(&leftover, b"half a blob").unwrap();
        BlobStore::open(tmp.path()).unwrap();
        assert!(!leftover.exists());
    }
}
