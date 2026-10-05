//! Snapshot manifests (spec §5): one JSON file per snapshot under
//! `<backups>/snapshots/<id>.json`. Manifests are the source of truth; the
//! `snapshots` table in SQLite is an index rebuilt from them when needed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::fsx::atomic::atomic_replace;

pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Manual,
    AppStart,
    GameExit,
    Scheduled,
    PreWrite,
    PreRestore,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Trigger::Manual => "manual",
            Trigger::AppStart => "app_start",
            Trigger::GameExit => "game_exit",
            Trigger::Scheduled => "scheduled",
            Trigger::PreWrite => "pre_write",
            Trigger::PreRestore => "pre_restore",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Trigger::Manual,
            Trigger::AppStart,
            Trigger::GameExit,
            Trigger::Scheduled,
            Trigger::PreWrite,
            Trigger::PreRestore,
        ]
        .into_iter()
        .find(|t| t.as_str() == s)
    }

    /// The UI's three groups: Manual, Auto, Safety.
    pub fn kind(self) -> SnapshotKind {
        match self {
            Trigger::Manual => SnapshotKind::Manual,
            Trigger::AppStart | Trigger::GameExit | Trigger::Scheduled => SnapshotKind::Auto,
            Trigger::PreWrite | Trigger::PreRestore => SnapshotKind::Safety,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum SnapshotKind {
    Manual,
    Auto,
    Safety,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// The whole tree that backups cover.
    Full,
    /// Only specific paths (safety snapshots before a write).
    Partial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestFile {
    /// Relative to the flavor folder, `/`-separated.
    pub path: String,
    pub size: u64,
    /// Modification time (RFC 3339, UTC), restored for information only.
    pub mtime: String,
    pub blake3: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub id: String,
    pub created_at: String,
    pub trigger: Trigger,
    /// Also kept here (not only in the db) so a rebuilt index keeps them.
    pub label: Option<String>,
    pub pinned: bool,
    pub scope: Scope,
    pub flavor: String,
    pub game_running: bool,
    pub include_addons: bool,
    pub files: Vec<ManifestFile>,
    /// Paths that didn't exist when a partial snapshot was taken, so undo
    /// can delete what the write created.
    #[serde(default)]
    pub absent: Vec<String>,
    /// Bytes this snapshot added to the blob store.
    pub new_bytes: u64,
    /// Files a full snapshot couldn't capture (vanished mid-walk, unreadable,
    /// odd names) and left out instead of failing the whole backup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedFile>,
}

/// A file a full snapshot left out, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SkippedFile {
    /// Relative to the flavor folder when it could be expressed that way;
    /// otherwise the path as the OS reported it.
    pub path: String,
    pub reason: String,
}

impl Manifest {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Distinct characters: `WTF/Account/<acct>/<group>/<char>/…`, where the
    /// group is Forever's opaque id (`70`) or, in older folders, the realm.
    pub fn char_count(&self) -> usize {
        self.files
            .iter()
            .filter_map(|f| {
                let parts: Vec<&str> = f.path.split('/').collect();
                (parts.len() >= 6
                    && parts[0].eq_ignore_ascii_case("WTF")
                    && parts[1].eq_ignore_ascii_case("Account")
                    && !parts[3].eq_ignore_ascii_case("SavedVariables"))
                .then(|| parts[2..5].join("/").to_lowercase())
            })
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// Distinct addons: SavedVariables file names (account or character), plus
    /// AddOns folders when those were included.
    pub fn addon_count(&self) -> usize {
        self.files
            .iter()
            .filter_map(|f| {
                let parts: Vec<&str> = f.path.split('/').collect();
                let n = parts.len();
                if n >= 2 && parts[n - 2].eq_ignore_ascii_case("SavedVariables") {
                    let name = parts[n - 1];
                    let stem = name
                        .strip_suffix(".lua.bak")
                        .or_else(|| name.strip_suffix(".lua"))?;
                    Some(stem.to_lowercase())
                } else if n >= 3
                    && parts[0].eq_ignore_ascii_case("Interface")
                    && parts[1].eq_ignore_ascii_case("AddOns")
                {
                    Some(parts[2].to_lowercase())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>()
            .len()
    }

    pub fn summary(&self) -> SnapshotSummary {
        SnapshotSummary {
            id: self.id.clone(),
            created_at: self.created_at.clone(),
            trigger: self.trigger,
            kind: self.trigger.kind(),
            label: self.label.clone(),
            pinned: self.pinned,
            scope: self.scope,
            flavor: self.flavor.clone(),
            game_running: self.game_running,
            file_count: self.files.len() as u32,
            char_count: self.char_count() as u32,
            addon_count: self.addon_count() as u32,
            total_bytes: self.total_bytes() as f64,
            new_bytes: self.new_bytes as f64,
        }
    }
}

/// One row of the Backups list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SnapshotSummary {
    pub id: String,
    pub created_at: String,
    pub trigger: Trigger,
    pub kind: SnapshotKind,
    pub label: Option<String>,
    pub pinned: bool,
    pub scope: Scope,
    pub flavor: String,
    pub game_running: bool,
    pub file_count: u32,
    pub char_count: u32,
    pub addon_count: u32,
    /// Bytes are f64 because TypeScript numbers can't hold a u64 safely;
    /// exact up to 9 PB.
    pub total_bytes: f64,
    pub new_bytes: f64,
}

/// Reads and writes manifests in `<backups>/snapshots`.
pub struct ManifestDir {
    dir: PathBuf,
}

impl ManifestDir {
    pub fn open(backups_dir: &Path) -> AppResult<Self> {
        let dir = backups_dir.join("snapshots");
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path_for(&self, id: &str) -> AppResult<PathBuf> {
        // Ids are ULIDs; refuse anything else so an id can't name another file.
        if id.len() != 26 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(AppError::NotFound(format!("snapshot {id:?}")));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn write(&self, manifest: &Manifest) -> AppResult<()> {
        let json = serde_json::to_vec_pretty(manifest)
            .map_err(|e| AppError::Io(format!("serialize manifest: {e}")))?;
        atomic_replace(&self.path_for(&manifest.id)?, &json)
    }

    pub fn read(&self, id: &str) -> AppResult<Manifest> {
        let bytes = std::fs::read(self.path_for(id)?).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AppError::NotFound(format!("snapshot {id}")),
            _ => e.into(),
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|_| AppError::corrupt(format!("snapshots/{id}.json")))
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        match std::fs::remove_file(self.path_for(id)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Blob ids referenced by every manifest on disk, for GC. Strict where
    /// `all()` is lenient: a manifest from a newer build (unknown variants,
    /// new fields) still contributes its `files[].blake3` through a loose
    /// JSON read, and if any manifest can't be read or isn't JSON at all, this
    /// fails, so GC never deletes blobs a snapshot might still need.
    pub fn all_blob_refs(&self) -> AppResult<std::collections::HashSet<String>> {
        let mut refs = std::collections::HashSet::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let refuse = |why: &str| {
                AppError::corrupt(format!(
                    "{} ({why}); not collecting garbage",
                    path.display()
                ))
            };
            let bytes = std::fs::read(&path).map_err(|_| refuse("unreadable"))?;
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| refuse("not valid JSON"))?;
            let files = value["files"]
                .as_array()
                .ok_or_else(|| refuse("no file list"))?;
            for file in files {
                let hash = file["blake3"]
                    .as_str()
                    .ok_or_else(|| refuse("file without a hash"))?;
                refs.insert(hash.to_string());
            }
        }
        Ok(refs)
    }

    /// Every readable manifest. Unreadable ones are skipped, not fatal
    /// (fine for listing; GC uses `all_blob_refs`).
    pub fn all(&self) -> AppResult<Vec<Manifest>> {
        let mut manifests = Vec::new();
        for entry in std::fs::read_dir(&self.dir)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(id) = name.strip_suffix(".json") {
                if let Ok(m) = self.read(id) {
                    manifests.push(m);
                }
            }
        }
        manifests.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(manifests)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn file(path: &str) -> ManifestFile {
        ManifestFile {
            path: path.into(),
            size: 10,
            mtime: "2026-10-04T00:00:00Z".into(),
            blake3: "0".repeat(64),
        }
    }

    pub fn manifest(id: &str, files: &[&str]) -> Manifest {
        Manifest {
            version: MANIFEST_VERSION,
            id: id.into(),
            created_at: "2026-10-04T11:52:00Z".into(),
            trigger: Trigger::GameExit,
            label: None,
            pinned: false,
            scope: Scope::Full,
            flavor: "_classic_beta_".into(),
            game_running: false,
            include_addons: false,
            files: files.iter().map(|p| file(p)).collect(),
            absent: Vec::new(),
            new_bytes: 0,
            skipped: Vec::new(),
        }
    }

    #[test]
    fn counts_characters_and_addons() {
        let m = manifest(
            "01J9ZZZZZZZZZZZZZZZZZZZZZZ",
            &[
                "WTF/Config.wtf",
                "WTF/Account/A1/SavedVariables/Details.lua",
                "WTF/Account/A1/SavedVariables/Details.lua.bak",
                "WTF/Account/A1/SavedVariables/WeakAuras.lua",
                "WTF/Account/A1/macros-cache.txt",
                "WTF/Account/A1/Ashenvale/Thrandor/AddOns.txt",
                "WTF/Account/A1/Ashenvale/Thrandor/SavedVariables/Questie.lua",
                "WTF/Account/A1/Ashenvale/Velyra/layout-local.txt",
                "WTF/Account/A2/Ashenvale/Thrandor/macros-cache.txt",
                "Interface/AddOns/Details/Details.toc",
            ],
        );
        assert_eq!(m.char_count(), 3, "same name on another account counts");
        assert_eq!(m.addon_count(), 3, "details, weakauras, questie");
        assert_eq!(m.total_bytes(), 100);
        let s = m.summary();
        assert_eq!(s.kind, SnapshotKind::Auto);
        assert_eq!(s.file_count, 10);
    }

    #[test]
    fn trigger_strings_round_trip() {
        for t in [
            Trigger::Manual,
            Trigger::AppStart,
            Trigger::GameExit,
            Trigger::Scheduled,
            Trigger::PreWrite,
            Trigger::PreRestore,
        ] {
            assert_eq!(Trigger::parse(t.as_str()), Some(t));
            assert_eq!(
                serde_json::to_value(t).unwrap(),
                serde_json::Value::String(t.as_str().into())
            );
        }
        assert_eq!(Trigger::PreRestore.kind(), SnapshotKind::Safety);
    }

    #[test]
    fn manifests_round_trip_and_reject_bad_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = ManifestDir::open(tmp.path()).unwrap();
        let m = manifest("01J9ZZZZZZZZZZZZZZZZZZZZZZ", &["WTF/Config.wtf"]);
        dir.write(&m).unwrap();
        assert_eq!(dir.read(&m.id).unwrap(), m);
        assert_eq!(dir.all().unwrap(), vec![m.clone()]);

        assert!(matches!(dir.read("../x"), Err(AppError::NotFound(_))));
        std::fs::write(
            tmp.path().join("snapshots/01J9YYYYYYYYYYYYYYYYYYYYYY.json"),
            b"{",
        )
        .unwrap();
        assert!(matches!(
            dir.read("01J9YYYYYYYYYYYYYYYYYYYYYY"),
            Err(AppError::BackupCorrupt { .. })
        ));
        assert_eq!(dir.all().unwrap().len(), 1, "corrupt manifest skipped");

        dir.delete(&m.id).unwrap();
        dir.delete(&m.id).unwrap();
        assert!(dir.all().unwrap().is_empty());
    }
}
