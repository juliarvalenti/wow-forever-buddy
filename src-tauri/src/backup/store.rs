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
    #[allow(dead_code)] // first production caller is restore (T9)
    pub fn get(&self, hash: &str) -> AppResult<Vec<u8>> {
        let corrupt = || AppError::BackupCorrupt {
            files: vec![hash.to_string()],
        };
        let compressed = std::fs::read(self.path_for(hash)?).map_err(|_| corrupt())?;
        let bytes = zstd::stream::decode_all(compressed.as_slice()).map_err(|_| corrupt())?;
        if Self::hash(&bytes) != hash {
            return Err(corrupt());
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
            let hash = format!("{prefix}{}", entry.file_name().to_string_lossy());
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
