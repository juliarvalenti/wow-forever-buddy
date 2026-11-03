//! WOWAPI's data folder for the preview (PV1): the API docs, global strings
//! and atlas and icon lists the emulator needs, about 8 MB. It's derived
//! from Blizzard's files, so it isn't in our installer (nor in the repo,
//! security review on #199): the player downloads it from the Preview tab,
//! once, from one fixed URL, and each file must match the SHA-256 the app
//! carries or it isn't kept. Nothing is sent. Settings can remove it.

use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The WOWAPI commit `tools/wowapi` is vendored from (ORIGIN.md).
pub const COMMIT: &str = "aea2b451a9075a5a5591ac3d56cf6e344c0557d0";
const BASE: &str = "https://raw.githubusercontent.com/Reece01-Dock/WOWAPI";

pub struct DataFile {
    pub name: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

/// Each file WOWAPI's `data/` holds at `COMMIT`.
pub const FILES: [DataFile; 6] = [
    DataFile {
        name: "apidocs.lua",
        sha256: "5fdafde2c823e0ef07b357d5def45b84bef26e074456b53cdf15b85f88031585",
        bytes: 1_301_428,
    },
    DataFile {
        name: "atlases.txt",
        sha256: "258cb935234fffd2916e9aa1f8dd872a15ade3aa07c5178cf0362e5bada6cc39",
        bytes: 2_360_375,
    },
    DataFile {
        name: "constants.lua",
        sha256: "0dd619c63f29febac44ea908bc377bd0101ea32d719119e1b225b20253e776ba",
        bytes: 103_543,
    },
    DataFile {
        name: "globalstrings_enUS.lua",
        sha256: "b7dd107d215455f1a1fa6759addf954f1c99f9361af618c1c18001c13bb5e879",
        bytes: 1_631_975,
    },
    DataFile {
        name: "icons.txt",
        sha256: "2195a41eb6e5fddaa9113da539728321addd75f1418d4594d3f28085237a9ac9",
        bytes: 1_276_582,
    },
    DataFile {
        name: "resources.lua",
        sha256: "81386835342b6b1316c9d8439bc102f63ae92c567138827a855a74bf8e68790f",
        bytes: 1_094_442,
    },
];

/// The download's size, for the button ("8 MB").
pub fn total_bytes() -> u64 {
    FILES.iter().map(|f| f.bytes).sum()
}

fn url(file: &DataFile) -> String {
    format!("{BASE}/{COMMIT}/wowapi/data/{}", file.name)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every file is there and matches its hash: the preview may run.
pub fn is_ready(dir: &Path) -> bool {
    FILES.iter().all(|f| {
        std::fs::read(dir.join(f.name))
            .is_ok_and(|b| b.len() as u64 == f.bytes && sha256_hex(&b) == f.sha256)
    })
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DataError {
    #[error("couldn't download {file}: {why}")]
    Network { file: &'static str, why: String },
    #[error("{file} from GitHub isn't the file this app expects, so it wasn't kept")]
    Mismatch { file: &'static str },
    #[error("couldn't save the preview data: {0}")]
    Disk(String),
}

/// Downloads what's missing or wrong into `dir`. A file is kept only once
/// its hash matches; `progress` gets the bytes done so far.
pub fn download(dir: &Path, mut progress: impl FnMut(u64)) -> Result<(), DataError> {
    download_from(dir, &mut progress, |file| {
        let mut resp = ureq::get(&url(file))
            .call()
            .map_err(|e| DataError::Network {
                file: file.name,
                why: e.to_string(),
            })?;
        resp.body_mut()
            .with_config()
            .limit(file.bytes + 1)
            .read_to_vec()
            .map_err(|e| DataError::Network {
                file: file.name,
                why: e.to_string(),
            })
    })
}

fn download_from(
    dir: &Path,
    progress: &mut dyn FnMut(u64),
    fetch: impl Fn(&DataFile) -> Result<Vec<u8>, DataError>,
) -> Result<(), DataError> {
    std::fs::create_dir_all(dir).map_err(|e| DataError::Disk(e.to_string()))?;
    let mut done = 0;
    for file in &FILES {
        let target = dir.join(file.name);
        let ok = std::fs::read(&target).is_ok_and(|b| sha256_hex(&b) == file.sha256);
        if !ok {
            let bytes = fetch(file)?;
            if sha256_hex(&bytes) != file.sha256 {
                return Err(DataError::Mismatch { file: file.name });
            }
            write_atomic(&target, &bytes).map_err(|e| DataError::Disk(e.to_string()))?;
        }
        done += file.bytes;
        progress(done);
    }
    Ok(())
}

fn write_atomic(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp: PathBuf = target.with_extension("download");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, target)
}

/// Settings > "Remove preview data".
pub fn remove(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_hashes_match_the_repo_data() {
        // fetch-data.sh fetches the same commit: the files must be the ones
        // the app expects, or every player's download would be refused.
        let dir = super::super::sandbox::tests::repo_data();
        for f in &FILES {
            let b = std::fs::read(dir.join(f.name)).unwrap();
            assert_eq!(b.len() as u64, f.bytes, "{}", f.name);
            assert_eq!(sha256_hex(&b), f.sha256, "{}", f.name);
        }
        assert!(is_ready(&dir));
    }

    #[test]
    fn a_file_that_doesnt_match_is_refused_and_not_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let err =
            download_from(tmp.path(), &mut |_| {}, |_| Ok(b"not the file".to_vec())).unwrap_err();
        assert_eq!(
            err,
            DataError::Mismatch {
                file: "apidocs.lua"
            }
        );
        assert!(!tmp.path().join("apidocs.lua").exists());
        assert!(!is_ready(tmp.path()));
    }

    #[test]
    fn a_download_that_matches_is_kept_and_ready() {
        let repo = super::super::sandbox::tests::repo_data();
        let tmp = tempfile::tempdir().unwrap();
        let mut seen = Vec::new();
        download_from(tmp.path(), &mut |n| seen.push(n), |f| {
            Ok(std::fs::read(repo.join(f.name)).unwrap())
        })
        .unwrap();
        assert!(is_ready(tmp.path()));
        assert_eq!(seen.last(), Some(&total_bytes()));
        remove(tmp.path()).unwrap();
        assert!(!is_ready(tmp.path()));
    }

    #[test]
    fn it_fetches_from_the_pinned_commit_only() {
        for f in &FILES {
            assert!(url(f)
                .starts_with("https://raw.githubusercontent.com/Reece01-Dock/WOWAPI/aea2b451"));
        }
    }
}
