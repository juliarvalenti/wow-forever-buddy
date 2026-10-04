//! Shared test helpers. Compiled only for tests.

use std::path::{Path, PathBuf};

/// The checked-in fake WoW install (`tests/fixtures/wow`). Read-only.
pub fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wow")
}

/// A private, writable copy of the fixture install. The returned `TempDir`
/// deletes it on drop; the install root is `dir.path().join("wow")`.
pub fn fixture_copy() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let root = dir.path().join("wow");
    copy_tree(&fixture_root(), &root);
    (dir, root)
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.expect("walk fixture");
        let target = to.join(entry.path().strip_prefix(from).unwrap());
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).expect("create fixture dir");
        } else {
            std::fs::copy(entry.path(), &target).expect("copy fixture file");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsx::read::safe_read;
    use crate::fsx::relpath::RelPath;

    #[test]
    fn fixture_copy_is_complete_and_byte_exact() {
        let (_dir, root) = fixture_copy();
        let flavor = root.join("_classic_beta_");

        for rel in [
            "WowB.exe",
            "WTF/Config.wtf",
            "WTF/Account/ACCOUNT1/SavedVariables/Details.lua.bak",
            "WTF/Account/ACCOUNT1/Old Blanchy/Brannic/SavedVariables/Questie.lua",
            "WTF/Account/ACCOUNT2/Ashenvale/Fizzwick/macros-cache.txt",
            "Cache/WDB/enUS/itemcache.wdb",
        ] {
            let path = RelPath::new(rel).unwrap().resolve(&flavor).unwrap();
            let original = fixture_root().join("_classic_beta_").join(rel);
            assert_eq!(
                safe_read(&path).unwrap(),
                std::fs::read(original).unwrap(),
                "{rel}"
            );
        }
        assert!(root.join(".build.info").is_file());
        assert!(root.join("_classic_era_/WTF").is_dir());
        assert!(!root.join("_classic_era_/WowB.exe").exists());
    }

    /// Guards the `.gitattributes -text` rule: a CRLF-converting checkout
    /// would silently change bytes and break byte-exact restore tests.
    #[test]
    fn fixture_files_have_lf_line_endings() {
        for entry in walkdir::WalkDir::new(fixture_root()) {
            let entry = entry.unwrap();
            if entry.file_type().is_file() {
                let bytes = std::fs::read(entry.path()).unwrap();
                assert!(
                    !bytes.windows(2).any(|w| w == b"\r\n"),
                    "CRLF in {}",
                    entry.path().display()
                );
            }
        }
    }
}
