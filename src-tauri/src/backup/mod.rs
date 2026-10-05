//! Backups (spec §5): snapshots of the game's settings stored in a
//! content-addressed blob store, described by manifests, indexed in SQLite.

pub mod export;
pub mod journal;
pub mod manifest;
pub mod restore;
pub mod retention;
pub mod store;
pub mod tree;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::OptionalExtension;

use crate::backup::manifest::{
    Manifest, ManifestDir, ManifestFile, Scope, SkippedFile, SnapshotKind, SnapshotSummary,
    Trigger, MANIFEST_VERSION,
};
use crate::backup::retention::Policy;
use crate::backup::store::BlobStore;
use crate::db::{Db, NEEDS_REINDEX};
use crate::error::{AppError, AppResult};
use crate::fsx::read::safe_read;
use crate::fsx::relpath::{GameRoot, RelPath};
use crate::game::gate::PreWriteSnapshot;

/// Hash-cache entries for files modified this close to the snapshot start
/// are neither trusted nor written: FAT/exFAT mtimes have 2 s granularity and
/// Windows can report mtime late, so a file rewritten right after we hashed
/// it could keep the same size and mtime (the "racy git" problem).
const RACY_WINDOW: Duration = Duration::from_secs(5);

/// Folders a full snapshot covers, relative to the flavor folder.
const WTF: &str = "WTF";
const ADDONS: &str = "Interface/AddOns";

/// What to put in a snapshot.
pub enum SnapshotScope<'a> {
    /// `WTF/**`, plus `Interface/AddOns/**` if asked.
    Full { include_addons: bool },
    /// Just these paths (files, or folders to walk). Paths that don't exist
    /// are recorded as absent.
    Paths(&'a [RelPath]),
}

pub struct SnapshotRequest<'a> {
    pub game: &'a GameRoot,
    pub flavor: &'a str,
    pub trigger: Trigger,
    pub label: Option<String>,
    pub scope: SnapshotScope<'a>,
    pub game_running: bool,
}

/// Whether a prune runs garbage collection now or only if it's due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gc {
    /// At most once per `GC_EVERY` (automatic backups).
    Throttled,
    /// Always ("Prune now").
    Now,
}

const GC_EVERY: Duration = Duration::from_secs(3600);
const LAST_GC: &str = "last_gc";

/// What a prune did.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct PruneReport {
    pub pruned: Vec<String>,
    pub blobs_removed: u32,
    pub freed_bytes: f64,
    /// Store size on disk afterwards.
    pub used_bytes: f64,
    pub budget_bytes: f64,
    /// Still over budget after pruning everything allowed (only manual,
    /// pinned or the newest few are left): the UI shows a warning.
    pub over_budget: bool,
}

/// The storage meter on the Backups screen.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct StorageInfo {
    pub used_bytes: f64,
    pub budget_bytes: f64,
    pub over_budget: bool,
    /// Generated from the policy, shown as is (spec §5).
    pub retention_summary: String,
}

/// Reports progress while files are captured: (done, total).
pub type Progress<'a> = &'a mut dyn FnMut(u32, u32);

/// The backup store. Long work (backups, restores) is serialized by the
/// caller through `AppCore::jobs`, not here, so a restore's safety snapshot
/// (taken inside the restore job) can't deadlock. Small edits to existing
/// manifests take the store's own short `edits` lock.
pub struct BackupService {
    dir: PathBuf,
    blobs: BlobStore,
    manifests: ManifestDir,
    db: Db,
    /// Guards read-modify-write of existing manifests (pin, label, delete,
    /// and pruning in T8). Held for milliseconds, never across a backup, so
    /// pinning while a backup runs doesn't wait.
    edits: std::sync::Mutex<()>,
}

impl BackupService {
    /// Opens the backup store in `backups_dir`. If the db's index doesn't
    /// match the manifests on disk (say the db was quarantined and
    /// recreated), the index is rebuilt from the manifests.
    pub fn open(backups_dir: &Path, db: Db) -> AppResult<Self> {
        let service = Self {
            dir: backups_dir.to_path_buf(),
            blobs: BlobStore::open(backups_dir)?,
            manifests: ManifestDir::open(backups_dir)?,
            db,
            edits: std::sync::Mutex::new(()),
        };
        let indexed: i64 = service
            .db
            .with_conn(|c| Ok(c.query_row("SELECT count(*) FROM snapshots", [], |r| r.get(0))?))?;
        if indexed as usize != service.manifests.all()?.len() {
            service.reindex()?;
        }
        service.reindex_if_flagged()?;
        Ok(service)
    }

    /// The folder this store lives in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Takes a snapshot. Returns `None` when an automatic full snapshot
    /// would be identical to the latest full one for this flavor.
    pub fn create(
        &self,
        req: SnapshotRequest<'_>,
        progress: Progress<'_>,
    ) -> AppResult<Option<SnapshotSummary>> {
        let started = SystemTime::now();

        // A full snapshot leaves out a file it can't read (vanished mid-walk,
        // locked, odd name) and records it, rather than losing the whole
        // backup to one file. A partial one is a safety copy of exactly what
        // is about to change, so it stays strict: missing one could mean
        // overwriting a file with no copy of it.
        let lenient = matches!(req.scope, SnapshotScope::Full { .. });
        let (targets, absent, mut skipped) = collect_targets(req.game, &req.scope, lenient)?;
        let total = targets.len() as u32;
        let mut files = Vec::with_capacity(targets.len());
        let mut new_bytes = 0;
        for (i, (rel, abs)) in targets.iter().enumerate() {
            // The outer `?` is the backup store failing: always fatal.
            match self.capture(req.flavor, rel, abs, started)? {
                Ok((file, written)) => {
                    files.push(file);
                    new_bytes += written;
                }
                Err(e) if lenient => skipped.push(SkippedFile {
                    path: rel.as_string(),
                    reason: e.to_string(),
                }),
                Err(e) => return Err(e),
            }
            progress(i as u32 + 1, total);
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        // Skipping everything isn't a backup, it's a failure.
        if files.is_empty() {
            if let Some(first) = skipped.first() {
                return Err(AppError::Io(format!(
                    "none of the game files could be read ({} skipped; first: {}: {})",
                    skipped.len(),
                    first.path,
                    first.reason
                )));
            }
        }

        let scope = match req.scope {
            SnapshotScope::Full { .. } => Scope::Full,
            SnapshotScope::Paths(_) => Scope::Partial,
        };
        if scope == Scope::Full
            && req.trigger.kind() == SnapshotKind::Auto
            && self.same_as_latest_full(req.flavor, &files)?
        {
            return Ok(None);
        }

        let manifest = Manifest {
            version: MANIFEST_VERSION,
            id: ulid::Ulid::generate().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            trigger: req.trigger,
            label: req.label,
            pinned: false,
            scope,
            flavor: req.flavor.to_string(),
            game_running: req.game_running,
            include_addons: matches!(
                req.scope,
                SnapshotScope::Full {
                    include_addons: true
                }
            ),
            files,
            absent: absent.iter().map(RelPath::as_string).collect(),
            new_bytes,
            skipped,
        };
        // Manifest first, then index: a crash in between leaves a manifest
        // the next startup re-indexes, never an index entry without data.
        self.manifests.write(&manifest)?;
        self.index(&manifest)?;
        Ok(Some(manifest.summary()))
    }

    /// Hashes and stores one file, using the hash cache when it's safe to.
    /// Returns its manifest entry and how many new bytes the store gained.
    /// The inner `Err` is the game file failing to read (the caller may skip
    /// it); the outer one is the backup store failing.
    fn capture(
        &self,
        flavor: &str,
        rel: &RelPath,
        abs: &Path,
        started: SystemTime,
    ) -> AppResult<AppResult<(ManifestFile, u64)>> {
        let path = rel.as_string();
        let (meta, mtime) = match std::fs::metadata(abs).and_then(|m| Ok((m.modified()?, m))) {
            Ok((mtime, meta)) => (meta, mtime),
            Err(e) => return Ok(Err(e.into())),
        };
        let mtime_ns = mtime
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let settled = mtime + RACY_WINDOW <= started;

        let cached = if settled {
            self.cached_hash(flavor, &path, meta.len(), mtime_ns)?
                .filter(|hash| self.blobs.contains(hash))
        } else {
            None
        };
        let (hash, size, written) = match cached {
            Some(hash) => (hash, meta.len(), 0),
            None => {
                let bytes = match safe_read(abs) {
                    Ok(bytes) => bytes,
                    Err(e) => return Ok(Err(e)),
                };
                let hash = BlobStore::hash(&bytes);
                let written = self.blobs.put(&hash, &bytes)?;
                if settled {
                    self.cache_hash(flavor, &path, bytes.len() as u64, mtime_ns, &hash)?;
                }
                (hash, bytes.len() as u64, written)
            }
        };
        let file = ManifestFile {
            path,
            size,
            mtime: chrono::DateTime::<chrono::Utc>::from(mtime).to_rfc3339(),
            blake3: hash,
        };
        Ok(Ok((file, written)))
    }

    fn cached_hash(
        &self,
        flavor: &str,
        path: &str,
        size: u64,
        mtime_ns: i64,
    ) -> AppResult<Option<String>> {
        self.db.with_conn(|c| {
            Ok(c.query_row(
                "SELECT blake3 FROM file_hash_cache
                 WHERE flavor = ?1 AND path = ?2 AND size = ?3 AND mtime_ns = ?4",
                (flavor, path, size as i64, mtime_ns),
                |r| r.get(0),
            )
            .optional()?)
        })
    }

    fn cache_hash(
        &self,
        flavor: &str,
        path: &str,
        size: u64,
        mtime_ns: i64,
        hash: &str,
    ) -> AppResult<()> {
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO file_hash_cache (flavor, path, size, mtime_ns, blake3)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (flavor, path) DO UPDATE SET
                   size = excluded.size, mtime_ns = excluded.mtime_ns, blake3 = excluded.blake3",
                (flavor, path, size as i64, mtime_ns, hash),
            )?;
            Ok(())
        })
    }

    fn same_as_latest_full(&self, flavor: &str, files: &[ManifestFile]) -> AppResult<bool> {
        let latest: Option<String> = self.db.with_conn(|c| {
            Ok(c.query_row(
                "SELECT id FROM snapshots WHERE scope = 'full' AND flavor = ?1
                 ORDER BY created_at DESC, id DESC LIMIT 1",
                [flavor],
                |r| r.get(0),
            )
            .optional()?)
        })?;
        let Some(id) = latest else { return Ok(false) };
        // Deleted while this backup ran (delete doesn't wait on backups):
        // treat it as "no previous snapshot" rather than failing the backup.
        let previous = match self.manifests.read(&id) {
            Ok(m) => m,
            Err(AppError::NotFound(_)) => return Ok(false),
            Err(e) => return Err(e),
        };
        let key = |f: &ManifestFile| (f.path.clone(), f.blake3.clone());
        Ok(previous.files.iter().map(key).eq(files.iter().map(key)))
    }

    fn index(&self, m: &Manifest) -> AppResult<()> {
        let s = m.summary();
        self.db.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO snapshots (id, created_at, trigger, label, pinned, scope,
                   flavor, file_count, total_bytes, new_bytes, char_count, addon_count, game_running)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    s.id,
                    s.created_at,
                    m.trigger.as_str(),
                    s.label,
                    s.pinned,
                    if m.scope == Scope::Full { "full" } else { "partial" },
                    s.flavor,
                    s.file_count,
                    m.total_bytes() as i64,
                    m.new_bytes as i64,
                    s.char_count,
                    s.addon_count,
                    s.game_running,
                ],
            )?;
            Ok(())
        })
    }

    /// Rebuilds the `snapshots` index from the manifests on disk.
    pub fn reindex(&self) -> AppResult<usize> {
        let manifests = self.manifests.all()?;
        self.db.with_conn(|c| {
            c.execute("DELETE FROM snapshots", [])?;
            Ok(())
        })?;
        for m in &manifests {
            self.index(m)?;
        }
        Ok(manifests.len())
    }

    /// Every snapshot, newest first.
    pub fn list(&self) -> AppResult<Vec<SnapshotSummary>> {
        self.db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT id, created_at, trigger, label, pinned, scope, flavor, file_count,
                        total_bytes, new_bytes, char_count, addon_count, game_running
                 FROM snapshots ORDER BY created_at DESC, id DESC",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    let trigger: String = r.get(2)?;
                    let trigger = Trigger::parse(&trigger).unwrap_or(Trigger::Manual);
                    let scope: String = r.get(5)?;
                    Ok(SnapshotSummary {
                        id: r.get(0)?,
                        created_at: r.get(1)?,
                        trigger,
                        kind: trigger.kind(),
                        label: r.get(3)?,
                        pinned: r.get(4)?,
                        scope: if scope == "full" {
                            Scope::Full
                        } else {
                            Scope::Partial
                        },
                        flavor: r.get(6)?,
                        file_count: r.get(7)?,
                        total_bytes: r.get::<_, i64>(8)? as f64,
                        new_bytes: r.get::<_, i64>(9)? as f64,
                        char_count: r.get(10)?,
                        addon_count: r.get(11)?,
                        game_running: r.get(12)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    pub fn manifest(&self, id: &str) -> AppResult<Manifest> {
        self.manifests.read(id)
    }

    pub fn detail(&self, id: &str) -> AppResult<tree::SnapshotDetail> {
        Ok(tree::detail(&self.manifests.read(id)?))
    }

    /// Removes a snapshot. Its blobs are freed by the next GC (T8).
    pub fn delete(&self, id: &str) -> AppResult<()> {
        let _edit = self.edits.lock().expect("edit lock poisoned");
        self.delete_unlocked(id)
    }

    fn delete_unlocked(&self, id: &str) -> AppResult<()> {
        self.manifests.read(id)?;
        self.manifests.delete(id)?;
        self.db.with_conn(|c| {
            c.execute("DELETE FROM snapshots WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    /// Applies the retention policy, then the storage budget, then frees
    /// blobs nothing references any more (spec §5).
    ///
    /// Must run under `AppCore::jobs`, never alongside a backup: a backup's
    /// new blobs aren't referenced until its manifest is written, so GC in
    /// between would delete them.
    pub fn prune(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        policy: &Policy,
        gc: Gc,
        held: &HashSet<String>,
    ) -> AppResult<PruneReport> {
        let _edit = self.edits.lock().expect("edit lock poisoned");
        // After a db quarantine the index may be incomplete; pruning from it
        // could miss which snapshots are pinned or manual.
        self.reindex_if_flagged()?;

        let mut report = PruneReport::default();
        let expired = retention::expired(&self.list()?, now, policy);
        for id in expired.into_iter().filter(|id| !held.contains(id)) {
            self.delete_unlocked(&id)?;
            report.pruned.push(id);
        }
        // GC reads every manifest and walks the whole blob store, so it runs
        // at most hourly (spec §5) unless asked for ("Prune now").
        // Over budget by the raw size, some of it may be garbage the throttle
        // hasn't collected yet: collect first and re-measure, or snapshots
        // would be deleted to free space that GC alone frees.
        if gc == Gc::Now || self.gc_due(now)? || self.blobs.size_on_disk() > policy.budget_bytes {
            self.collect_garbage(&mut report, now)?;
        }

        if self.blobs.size_on_disk() > policy.budget_bytes {
            let mut candidates = retention::budget_candidates(&self.list()?, policy);
            candidates.retain(|id| !held.contains(id));
            for batch in candidates.chunks(5) {
                for id in batch {
                    self.delete_unlocked(id)?;
                    report.pruned.push(id.clone());
                }
                // Over budget, GC is how deleting a snapshot frees space.
                self.collect_garbage(&mut report, now)?;
                if self.blobs.size_on_disk() <= policy.budget_bytes {
                    break;
                }
            }
        }
        let used = self.blobs.size_on_disk();
        report.used_bytes = used as f64;
        report.budget_bytes = policy.budget_bytes as f64;
        // Only manual/pinned (or the floor) left and still over: tell the user.
        report.over_budget = used > policy.budget_bytes;
        Ok(report)
    }

    fn collect_garbage(
        &self,
        report: &mut PruneReport,
        now: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<()> {
        let refs = self.referenced_blobs()?;
        let (removed, freed) = self.blobs.retain(&refs)?;
        report.blobs_removed += removed as u32;
        report.freed_bytes += freed as f64;
        self.db.set_meta(LAST_GC, &now.to_rfc3339())
    }

    fn gc_due(&self, now: chrono::DateTime<chrono::Utc>) -> AppResult<bool> {
        let last = self
            .db
            .get_meta(LAST_GC)?
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok());
        Ok(last.is_none_or(|t| {
            now.signed_duration_since(t) >= chrono::Duration::from_std(GC_EVERY).unwrap()
        }))
    }

    fn reindex_if_flagged(&self) -> AppResult<()> {
        if self.db.get_meta(NEEDS_REINDEX)?.is_some() {
            self.reindex()?;
            self.db.with_conn(|c| {
                c.execute("DELETE FROM meta WHERE key = ?1", [NEEDS_REINDEX])?;
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Store size and policy, for the Backups header and Settings.
    pub fn storage(&self, policy: &Policy) -> StorageInfo {
        let used = self.blobs.size_on_disk();
        StorageInfo {
            used_bytes: used as f64,
            budget_bytes: policy.budget_bytes as f64,
            over_budget: used > policy.budget_bytes,
            retention_summary: policy.summary(),
        }
    }

    /// Pinned snapshots are never pruned. Stored in the manifest too, so a
    /// rebuilt index keeps it.
    pub fn set_pinned(&self, id: &str, pinned: bool) -> AppResult<SnapshotSummary> {
        self.edit_manifest(id, |m| m.pinned = pinned)
    }

    pub fn set_label(&self, id: &str, label: Option<String>) -> AppResult<SnapshotSummary> {
        let label = clean_label(label)?;
        self.edit_manifest(id, |m| m.label = label)
    }

    fn edit_manifest(
        &self,
        id: &str,
        edit: impl FnOnce(&mut Manifest),
    ) -> AppResult<SnapshotSummary> {
        let _edit = self.edits.lock().expect("edit lock poisoned");
        let mut manifest = self.manifests.read(id)?;
        edit(&mut manifest);
        self.manifests.write(&manifest)?;
        self.index(&manifest)?;
        Ok(manifest.summary())
    }

    /// Blob ids referenced by any manifest (for GC, T8).
    pub fn referenced_blobs(&self) -> AppResult<HashSet<String>> {
        self.manifests.all_blob_refs()
    }

    pub fn blobs(&self) -> &BlobStore {
        &self.blobs
    }
}

/// Trims a user-supplied label; blank means none. At most 200 characters.
pub fn clean_label(label: Option<String>) -> AppResult<Option<String>> {
    let label = label
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());
    if label.as_ref().is_some_and(|l| l.chars().count() > 200) {
        return Err(AppError::InvalidSettings(
            "label is too long (max 200)".into(),
        ));
    }
    Ok(label)
}

/// Files to capture: each `RelPath` with its resolved location.
type Targets = Vec<(RelPath, PathBuf)>;

/// Files a snapshot will capture (`RelPath` + resolved path), and declared
/// paths that don't exist. Symlinks inside the tree are skipped, never
/// followed; a linked `WTF` itself is fine (see `GameRoot`).
/// Files to capture, paths recorded as absent, and (when `lenient`) entries
/// left out because they couldn't be listed or named. The top folders
/// themselves (`WTF`, a requested path) must still resolve: if the whole
/// game folder is gone, that's a failed backup, not a skipped file.
fn collect_targets(
    game: &GameRoot,
    scope: &SnapshotScope<'_>,
    lenient: bool,
) -> AppResult<(Targets, Vec<RelPath>, Vec<SkippedFile>)> {
    let mut targets = Vec::new();
    let mut absent = Vec::new();
    let mut skipped = Vec::new();
    let mut walk = |rel: RelPath, targets: &mut Targets| -> AppResult<bool> {
        let abs = rel.resolve(game)?;
        if abs.is_file() {
            targets.push((rel, abs));
            return Ok(true);
        }
        if !abs.is_dir() {
            return Ok(false);
        }
        for entry in walkdir::WalkDir::new(&abs).follow_links(false) {
            let found = entry
                .map_err(|e| {
                    let path = e.path().map(Path::to_path_buf);
                    (path, AppError::Io(e.to_string()))
                })
                .and_then(|entry| {
                    let name = entry.file_name().to_string_lossy();
                    if !entry.file_type().is_file() || name.contains(".wfb-tmp-") {
                        return Ok(None);
                    }
                    let failed = |e| (Some(entry.path().to_path_buf()), e);
                    let rel = RelPath::from_under(&game.base, entry.path()).map_err(failed)?;
                    let abs = rel.resolve(game).map_err(failed)?;
                    Ok(Some((rel, abs)))
                });
            match found {
                Ok(Some(target)) => targets.push(target),
                Ok(None) => {}
                Err((path, e)) if lenient => skipped.push(SkippedFile {
                    path: path
                        .map(|p| {
                            RelPath::from_under(&game.base, &p)
                                .map(|r| r.as_string())
                                .unwrap_or_else(|_| p.to_string_lossy().into_owned())
                        })
                        .unwrap_or_default(),
                    reason: e.to_string(),
                }),
                Err((_, e)) => return Err(e),
            }
        }
        Ok(true)
    };

    match scope {
        SnapshotScope::Full { include_addons } => {
            walk(RelPath::new(WTF)?, &mut targets)?;
            if *include_addons {
                walk(RelPath::new(ADDONS)?, &mut targets)?;
            }
        }
        SnapshotScope::Paths(paths) => {
            for rel in paths.iter() {
                if !walk(rel.clone(), &mut targets)? {
                    absent.push(rel.clone());
                }
            }
        }
    }
    targets.sort_by(|a, b| a.0.cmp(&b.0));
    targets.dedup_by(|a, b| a.0 == b.0);
    Ok((targets, absent, skipped))
}

/// The write gate's safety snapshot (spec §4): a partial snapshot of exactly
/// the paths about to change.
impl PreWriteSnapshot for BackupService {
    fn snapshot_before_write(
        &self,
        game: &GameRoot,
        op: &str,
        paths: &[RelPath],
        label: &str,
    ) -> AppResult<String> {
        let flavor = game
            .base
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let trigger = if op == "restore" {
            Trigger::PreRestore
        } else {
            Trigger::PreWrite
        };
        let summary = self
            .create(
                SnapshotRequest {
                    game,
                    flavor: &flavor,
                    trigger,
                    label: Some(label.to_string()),
                    scope: SnapshotScope::Paths(paths),
                    game_running: false,
                },
                &mut |_, _| {},
            )?
            .expect("partial snapshots are never skipped");
        Ok(summary.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_copy;

    struct Setup {
        _dir: tempfile::TempDir,
        flavor_dir: PathBuf,
        backups_dir: PathBuf,
        game: GameRoot,
        db: Db,
        service: BackupService,
    }

    fn setup() -> Setup {
        let (dir, root) = fixture_copy();
        let flavor_dir = root.join("_classic_beta_");
        let backups_dir = dir.path().join("backups");
        let db = Db::open(&dir.path().join("buddy.db")).unwrap();
        let service = BackupService::open(&backups_dir, db.clone()).unwrap();
        Setup {
            game: GameRoot::new(&flavor_dir).unwrap(),
            flavor_dir,
            backups_dir,
            db,
            service,
            _dir: dir,
        }
    }

    fn full(s: &Setup, trigger: Trigger) -> Option<SnapshotSummary> {
        s.service
            .create(
                SnapshotRequest {
                    game: &s.game,
                    flavor: "_classic_beta_",
                    trigger,
                    label: None,
                    scope: SnapshotScope::Full {
                        include_addons: false,
                    },
                    game_running: false,
                },
                &mut |_, _| {},
            )
            .unwrap()
    }

    fn rel(p: &str) -> RelPath {
        RelPath::new(p).unwrap()
    }

    /// Makes every fixture file look older than the racy window, so the
    /// hash cache is allowed to work.
    fn age_files(dir: &Path) {
        let old = SystemTime::now() - Duration::from_secs(3600);
        for e in walkdir::WalkDir::new(dir).into_iter().flatten() {
            if e.file_type().is_file() {
                let f = std::fs::File::options().write(true).open(e.path()).unwrap();
                f.set_modified(old).unwrap();
            }
        }
    }

    #[test]
    fn full_snapshot_captures_wtf_only_and_round_trips() {
        let s = setup();
        let mut seen = Vec::new();
        let summary = s
            .service
            .create(
                SnapshotRequest {
                    game: &s.game,
                    flavor: "_classic_beta_",
                    trigger: Trigger::Manual,
                    label: Some("Before patch 1.60.1".into()),
                    scope: SnapshotScope::Full {
                        include_addons: false,
                    },
                    game_running: true,
                },
                &mut |done, total| seen.push((done, total)),
            )
            .unwrap()
            .unwrap();

        assert_eq!(summary.kind, SnapshotKind::Manual);
        assert!(summary.game_running, "flagged as mid-session");
        assert_eq!(seen.last(), Some(&(summary.file_count, summary.file_count)));
        let m = s.service.manifest(&summary.id).unwrap();
        assert!(
            m.files.iter().all(|f| f.path.starts_with("WTF/")),
            "no Cache/Logs/Screenshots/Interface"
        );
        assert!(m
            .files
            .iter()
            .any(|f| f.path.ends_with("Lúthien/SavedVariables/Details.lua")));
        assert_eq!(summary.char_count, 5);
        assert_eq!(s.service.list().unwrap(), vec![summary.clone()]);

        // Every file's bytes come back exactly.
        for f in &m.files {
            let original = std::fs::read(s.flavor_dir.join(&f.path)).unwrap();
            assert_eq!(
                s.service.blobs().get(&f.blake3).unwrap(),
                original,
                "{}",
                f.path
            );
        }
    }

    #[test]
    fn include_addons_adds_the_addons_folder() {
        let s = setup();
        let summary = s
            .service
            .create(
                SnapshotRequest {
                    game: &s.game,
                    flavor: "_classic_beta_",
                    trigger: Trigger::Manual,
                    label: None,
                    scope: SnapshotScope::Full {
                        include_addons: true,
                    },
                    game_running: false,
                },
                &mut |_, _| {},
            )
            .unwrap()
            .unwrap();
        let m = s.service.manifest(&summary.id).unwrap();
        assert!(m.include_addons);
        assert!(m
            .files
            .iter()
            .any(|f| f.path == "Interface/AddOns/Details/Details.toc"));
    }

    #[test]
    fn identical_auto_snapshot_is_skipped_but_manual_is_not() {
        let s = setup();
        let first = full(&s, Trigger::GameExit).unwrap();
        assert!(first.new_bytes > 0.0);
        assert_eq!(full(&s, Trigger::GameExit), None, "nothing changed");
        assert_eq!(full(&s, Trigger::Scheduled), None);

        let manual = full(&s, Trigger::Manual).unwrap();
        assert_eq!(manual.new_bytes, 0.0, "everything deduplicated");

        std::fs::write(s.flavor_dir.join("WTF/Config.wtf"), b"SET changed \"1\"\n").unwrap();
        let after = full(&s, Trigger::GameExit).unwrap();
        assert!(after.new_bytes > 0.0, "only the changed file is new");
        assert_eq!(s.service.list().unwrap().len(), 3);
    }

    #[test]
    fn hash_cache_is_used_for_settled_files_only() {
        let s = setup();
        age_files(&s.flavor_dir);
        full(&s, Trigger::Manual).unwrap();
        let cached: i64 =
            s.db.with_conn(|c| {
                Ok(c.query_row("SELECT count(*) FROM file_hash_cache", [], |r| r.get(0))?)
            })
            .unwrap();
        assert!(cached > 0, "settled files are cached");

        // A file that just changed is never cached, so a same-size, same-mtime
        // rewrite right after can't fool the next snapshot.
        let config = s.flavor_dir.join("WTF/Config.wtf");
        std::fs::write(&config, b"SET fresh \"1\"\n").unwrap();
        full(&s, Trigger::Manual).unwrap();
        let fresh: Option<String> =
            s.db.with_conn(|c| {
                Ok(c.query_row(
                    "SELECT blake3 FROM file_hash_cache WHERE path = 'WTF/Config.wtf'",
                    [],
                    |r| r.get(0),
                )
                .optional()?)
            })
            .unwrap();
        assert_ne!(
            fresh,
            Some(BlobStore::hash(b"SET fresh \"1\"\n")),
            "recently modified file must not be cached"
        );
    }

    #[test]
    fn cache_key_is_case_insensitive_and_install_relative() {
        let s = setup();
        s.service
            .cache_hash("_classic_beta_", "WTF/Config.wtf", 1, 2, &"a".repeat(64))
            .unwrap();
        assert_eq!(
            s.service
                .cached_hash("_classic_beta_", "wtf/config.WTF", 1, 2)
                .unwrap(),
            Some("a".repeat(64))
        );
        assert_eq!(
            s.service
                .cached_hash("_retail_", "WTF/Config.wtf", 1, 2)
                .unwrap(),
            None
        );
    }

    #[test]
    fn missing_blob_is_recaptured_even_if_cached() {
        let s = setup();
        age_files(&s.flavor_dir);
        let first = full(&s, Trigger::Manual).unwrap();
        // GC (or damage) removed every blob, but the hash cache still knows them.
        s.service.blobs().retain(&HashSet::new()).unwrap();
        let second = full(&s, Trigger::Manual).unwrap();
        assert!(second.new_bytes > 0.0);
        let m = s.service.manifest(&second.id).unwrap();
        assert!(m
            .files
            .iter()
            .all(|f| s.service.blobs().contains(&f.blake3)));
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn partial_snapshot_records_absent_paths() {
        let s = setup();
        let macros = rel("WTF/Account/ACCOUNT1/macros-cache.txt");
        let new = rel("WTF/Account/ACCOUNT1/SavedVariables/New.lua");
        let thrandor = rel("WTF/Account/ACCOUNT1/Ashenvale/Thrandor");
        let id = s
            .service
            .snapshot_before_write(
                &s.game,
                "restore",
                &[macros, new, thrandor],
                "Before restoring Thrandor",
            )
            .unwrap();
        let m = s.service.manifest(&id).unwrap();
        assert_eq!(m.trigger, Trigger::PreRestore);
        assert_eq!(m.scope, Scope::Partial);
        assert_eq!(m.label.as_deref(), Some("Before restoring Thrandor"));
        assert_eq!(m.absent, ["WTF/Account/ACCOUNT1/SavedVariables/New.lua"]);
        assert!(m
            .files
            .iter()
            .any(|f| f.path == "WTF/Account/ACCOUNT1/macros-cache.txt"));
        assert_eq!(
            m.files
                .iter()
                .filter(|f| f.path.contains("/Thrandor/"))
                .count(),
            7,
            "folders are walked"
        );
        assert_eq!(m.summary().kind, SnapshotKind::Safety);
    }

    #[test]
    fn pin_and_label_live_in_the_manifest_and_survive_reindex() {
        let s = setup();
        let snap = full(&s, Trigger::Manual).unwrap();
        s.service.set_pinned(&snap.id, true).unwrap();
        s.service
            .set_label(&snap.id, Some("  Clean UI  ".into()))
            .unwrap();

        // Lose the index entirely, as after a db quarantine.
        s.db.with_conn(|c| {
            c.execute("DELETE FROM snapshots", [])?;
            Ok(())
        })
        .unwrap();
        let reopened = BackupService::open(&s.backups_dir, s.db.clone()).unwrap();
        let list = reopened.list().unwrap();
        assert_eq!(list.len(), 1, "index rebuilt from manifests");
        assert!(list[0].pinned);
        assert_eq!(list[0].label.as_deref(), Some("Clean UI"));

        reopened.set_label(&snap.id, Some("   ".into())).unwrap();
        assert_eq!(reopened.list().unwrap()[0].label, None);
        assert!(reopened.set_label(&snap.id, Some("x".repeat(201))).is_err());
    }

    /// Review must-fix 2: GC must never drop blobs a snapshot still needs.
    #[test]
    fn gc_refs_survive_newer_manifests_and_refuse_unreadable_ones() {
        let s = setup();
        let snap = full(&s, Trigger::Manual).unwrap();
        let manifest_path = s
            .backups_dir
            .join("snapshots")
            .join(format!("{}.json", snap.id));
        let all_refs = s.service.referenced_blobs().unwrap();
        assert!(!all_refs.is_empty());

        // A newer build wrote a trigger and field we don't know: listing skips
        // it, but its blobs are still referenced.
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        value["trigger"] = "weekly_auto".into();
        value["version"] = 2.into();
        std::fs::write(&manifest_path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            s.service.manifests.all().unwrap().is_empty(),
            "unparseable for listing"
        );
        assert_eq!(s.service.referenced_blobs().unwrap(), all_refs);

        // A manifest we can't read at all: refuse to compute references, so
        // GC can't run and delete anything.
        std::fs::write(&manifest_path, b"{ truncated").unwrap();
        assert!(matches!(
            s.service.referenced_blobs(),
            Err(AppError::BackupCorrupt { .. })
        ));
        for hash in &all_refs {
            assert!(s.service.blobs().contains(hash));
        }
    }

    /// Moves a snapshot's creation time back, as if it were taken long ago.
    fn backdate(s: &Setup, id: &str, hours: i64) {
        let mut m = s.service.manifests.read(id).unwrap();
        m.created_at = (chrono::Utc::now() - chrono::Duration::hours(hours)).to_rfc3339();
        s.service.manifests.write(&m).unwrap();
        s.service.index(&m).unwrap();
    }

    /// A full automatic snapshot after changing Config.wtf, so it's never
    /// skipped as identical and has one blob of its own.
    fn changed_auto(s: &Setup, n: usize) -> SnapshotSummary {
        std::fs::write(
            s.flavor_dir.join("WTF/Config.wtf"),
            format!("SET change \"{n}\"\n"),
        )
        .unwrap();
        full(s, Trigger::GameExit).unwrap()
    }

    #[test]
    fn prune_expires_old_autos_and_frees_only_their_blobs() {
        let s = setup();
        let old = changed_auto(&s, 1);
        let recent = changed_auto(&s, 2);
        let manual = full(&s, Trigger::Manual).unwrap();
        backdate(&s, &old.id, 24 * 70);
        let old_config = s
            .service
            .manifest(&old.id)
            .unwrap()
            .files
            .into_iter()
            .find(|f| f.path == "WTF/Config.wtf")
            .unwrap()
            .blake3;

        let report = s
            .service
            .prune(
                chrono::Utc::now(),
                &retention::POLICY,
                Gc::Now,
                &HashSet::new(),
            )
            .unwrap();
        assert_eq!(report.pruned, vec![old.id.clone()]);
        assert!(report.blobs_removed >= 1 && report.freed_bytes > 0.0);
        assert!(
            !s.service.blobs().contains(&old_config),
            "its unique blob is gone"
        );
        for id in [&recent.id, &manual.id] {
            for f in s.service.manifest(id).unwrap().files {
                assert!(s.service.blobs().contains(&f.blake3), "shared blobs stay");
            }
        }
        assert!(!report.over_budget);
    }

    #[test]
    fn over_budget_prunes_oldest_autos_down_to_the_floor() {
        let s = setup();
        let manual = full(&s, Trigger::Manual).unwrap();
        let autos: Vec<_> = (0..5).map(|n| changed_auto(&s, n)).collect();
        let tiny = Policy {
            budget_bytes: 1,
            ..retention::POLICY
        };

        let report = s
            .service
            .prune(chrono::Utc::now(), &tiny, Gc::Throttled, &HashSet::new())
            .unwrap();
        assert_eq!(report.pruned, [autos[0].id.clone(), autos[1].id.clone()]);
        assert!(
            report.over_budget,
            "manual + the newest 3 still exceed 1 byte"
        );
        let left: Vec<String> = s
            .service
            .list()
            .unwrap()
            .into_iter()
            .map(|x| x.id)
            .collect();
        assert!(left.contains(&manual.id));
        assert_eq!(left.len(), 4);
    }

    /// T3 carry-over: after a db quarantine, prune rebuilds the index first
    /// and never deletes a pinned or manual snapshot.
    #[test]
    fn prune_after_quarantine_rebuilds_index_and_keeps_protected() {
        let s = setup();
        let manual = full(&s, Trigger::Manual).unwrap();
        let pinned = changed_auto(&s, 1);
        let old = changed_auto(&s, 2);
        s.service.set_pinned(&pinned.id, true).unwrap();
        backdate(&s, &pinned.id, 24 * 400);
        backdate(&s, &old.id, 24 * 400);

        // The db was recreated: index lost, flag set.
        s.db.with_conn(|c| {
            c.execute("DELETE FROM snapshots", [])?;
            Ok(())
        })
        .unwrap();
        s.db.set_meta(NEEDS_REINDEX, "1").unwrap();

        let report = s
            .service
            .prune(
                chrono::Utc::now(),
                &retention::POLICY,
                Gc::Now,
                &HashSet::new(),
            )
            .unwrap();
        assert_eq!(report.pruned, vec![old.id.clone()]);
        let left: Vec<String> = s
            .service
            .list()
            .unwrap()
            .into_iter()
            .map(|x| x.id)
            .collect();
        assert!(left.contains(&manual.id) && left.contains(&pinned.id));
        assert_eq!(s.db.get_meta(NEEDS_REINDEX).unwrap(), None, "flag cleared");
    }

    /// Review item 1: snapshots an interrupted restore needs survive both
    /// the time rules and the budget.
    #[test]
    fn held_snapshots_are_never_pruned() {
        let s = setup();
        let safety: Vec<_> = (0..5)
            .map(|_| full(&s, Trigger::PreRestore).unwrap())
            .collect();
        for x in &safety {
            backdate(&s, &x.id, 24 * 400);
        }
        let tiny = Policy {
            budget_bytes: 1,
            ..retention::POLICY
        };
        let held = HashSet::from([safety[0].id.clone()]);

        let report = s
            .service
            .prune(chrono::Utc::now(), &tiny, Gc::Now, &held)
            .unwrap();
        assert_eq!(report.pruned, [safety[1].id.clone()], "the oldest is held");
        assert!(s.service.manifest(&safety[0].id).is_ok());
        for f in s.service.manifest(&safety[0].id).unwrap().files {
            assert!(s.service.blobs().contains(&f.blake3), "and its blobs");
        }
    }

    /// R1: over budget only because of garbage the hourly throttle hasn't
    /// collected yet: GC runs first, and no snapshot is deleted.
    #[test]
    fn over_budget_collects_garbage_before_pruning() {
        let s = setup();
        let autos: Vec<_> = (0..5).map(|n| changed_auto(&s, n)).collect();
        let now = chrono::Utc::now();
        s.service
            .prune(now, &retention::POLICY, Gc::Now, &HashSet::new())
            .unwrap();
        let budget = s.service.blobs().size_on_disk() + 1024;

        // Garbage pushes the store over budget; GC was just done (throttled).
        for n in 0..8u8 {
            // Incompressible, so it really takes space after zstd.
            let mut junk = vec![0; 16 * 1024];
            blake3::Hasher::new()
                .update(&[n])
                .finalize_xof()
                .fill(&mut junk);
            s.service
                .blobs()
                .put(&BlobStore::hash(&junk), &junk)
                .unwrap();
        }
        assert!(s.service.blobs().size_on_disk() > budget);
        let policy = Policy {
            budget_bytes: budget,
            ..retention::POLICY
        };
        let soon = now + chrono::Duration::minutes(5);
        let report = s
            .service
            .prune(soon, &policy, Gc::Throttled, &HashSet::new())
            .unwrap();
        assert!(report.pruned.is_empty(), "GC alone was enough");
        assert_eq!(report.blobs_removed, 8);
        assert!(!report.over_budget);
        assert_eq!(s.service.list().unwrap().len(), autos.len());
    }

    /// R1: a file that can't be read is left out of a full snapshot and
    /// recorded, instead of failing the whole backup; a safety snapshot of
    /// that same path still fails.
    #[cfg(windows)]
    #[test]
    fn full_snapshot_skips_a_locked_file() {
        use std::os::windows::fs::OpenOptionsExt;

        let s = setup();
        let locked = s.flavor_dir.join("WTF/Config.wtf");
        let _hold = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked)
            .unwrap();
        assert_skipped_but_strict_for_safety(&s, "WTF/Config.wtf");
    }

    #[cfg(unix)]
    #[test]
    fn full_snapshot_skips_unreadable_and_oddly_named_files() {
        use std::os::unix::fs::PermissionsExt;

        let s = setup();
        let config = s.flavor_dir.join("WTF/Config.wtf");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&config).is_ok() {
            return; // running as root: permissions don't apply
        }
        // A name `RelPath` refuses (a DOS device name) can't be captured either.
        std::fs::write(s.flavor_dir.join("WTF/CON.lua"), b"x").unwrap();

        let id = full(&s, Trigger::Manual).unwrap().id;
        let m = s.service.manifest(&id).unwrap();
        assert!(
            m.skipped.iter().any(|f| f.path.ends_with("CON.lua")),
            "{m:?}"
        );
        assert_skipped_but_strict_for_safety(&s, "WTF/Config.wtf");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    /// #27 review: a snapshot that captured nothing is a failure, not a
    /// success with every file skipped.
    #[cfg(unix)]
    #[test]
    fn skipping_every_file_is_a_failure() {
        use std::os::unix::fs::PermissionsExt;

        let s = setup();
        let files: Vec<_> = walkdir::WalkDir::new(s.flavor_dir.join("WTF"))
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.into_path())
            .collect();
        let mode = |m| {
            for f in &files {
                std::fs::set_permissions(f, std::fs::Permissions::from_mode(m)).unwrap();
            }
        };
        mode(0o000);
        if std::fs::read(&files[0]).is_ok() {
            mode(0o644);
            return; // running as root: permissions don't apply
        }
        let err = s.service.create(
            SnapshotRequest {
                game: &s.game,
                flavor: "_classic_beta_",
                trigger: Trigger::GameExit,
                label: None,
                scope: SnapshotScope::Full {
                    include_addons: false,
                },
                game_running: false,
            },
            &mut |_, _| {},
        );
        mode(0o644);
        assert!(matches!(err, Err(AppError::Io(m)) if m.contains("none of the game files")));
        assert!(s.service.list().unwrap().is_empty());
    }

    #[cfg(any(windows, unix))]
    fn assert_skipped_but_strict_for_safety(s: &Setup, path: &str) {
        // Manual, so it's never skipped as identical to an earlier one.
        let summary = full(s, Trigger::Manual).expect("backed up anyway");
        let m = s.service.manifest(&summary.id).unwrap();
        assert!(m.files.iter().all(|f| f.path != path), "left out");
        assert!(m.files.len() > 1, "everything else captured");
        let skipped = m.skipped.iter().find(|f| f.path == path).expect("recorded");
        assert!(!skipped.reason.is_empty());
        let detail = s.service.detail(&summary.id).unwrap();
        assert!(detail.skipped.iter().any(|f| f.path == path));

        let rel = [RelPath::new(path).unwrap()];
        let strict = s.service.create(
            SnapshotRequest {
                game: &s.game,
                flavor: "_classic_beta_",
                trigger: Trigger::PreRestore,
                label: None,
                scope: SnapshotScope::Paths(&rel),
                game_running: false,
            },
            &mut |_, _| {},
        );
        assert!(strict.is_err(), "a safety snapshot never skips");
    }

    /// Review item 2: GC runs at most hourly unless forced.
    #[test]
    fn gc_is_throttled_unless_forced() {
        let s = setup();
        full(&s, Trigger::Manual).unwrap();
        let now = chrono::Utc::now();
        // First prune: nothing recorded yet, so GC is due.
        s.service
            .prune(now, &retention::POLICY, Gc::Throttled, &HashSet::new())
            .unwrap();

        // An orphan blob appears (e.g. its snapshot was deleted).
        let orphan = BlobStore::hash(b"orphan");
        s.service.blobs().put(&orphan, b"orphan").unwrap();

        let soon = now + chrono::Duration::minutes(10);
        let r = s
            .service
            .prune(soon, &retention::POLICY, Gc::Throttled, &HashSet::new())
            .unwrap();
        assert_eq!(r.blobs_removed, 0, "throttled within the hour");
        assert!(s.service.blobs().contains(&orphan));

        let forced = s
            .service
            .prune(soon, &retention::POLICY, Gc::Now, &HashSet::new())
            .unwrap();
        assert_eq!(forced.blobs_removed, 1, "Prune now always collects");

        s.service.blobs().put(&orphan, b"orphan").unwrap();
        let later = soon + chrono::Duration::minutes(61);
        let r = s
            .service
            .prune(later, &retention::POLICY, Gc::Throttled, &HashSet::new())
            .unwrap();
        assert_eq!(r.blobs_removed, 1, "due again after an hour");
    }

    #[test]
    fn storage_reports_size_budget_and_summary() {
        let s = setup();
        full(&s, Trigger::Manual).unwrap();
        let info = s.service.storage(&retention::POLICY);
        assert!(info.used_bytes > 0.0);
        assert_eq!(info.budget_bytes, retention::POLICY.budget_bytes as f64);
        assert!(!info.over_budget);
        assert_eq!(info.retention_summary, retention::POLICY.summary());
    }

    #[test]
    fn delete_removes_manifest_and_index() {
        let s = setup();
        let snap = full(&s, Trigger::Manual).unwrap();
        s.service.delete(&snap.id).unwrap();
        assert!(s.service.list().unwrap().is_empty());
        assert!(matches!(
            s.service.manifest(&snap.id),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            s.service.delete(&snap.id),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn linked_wtf_is_backed_up_from_its_target() {
        let s = setup();
        let synced = s.backups_dir.parent().unwrap().join("Dropbox").join("WTF");
        std::fs::create_dir_all(synced.parent().unwrap()).unwrap();
        std::fs::rename(s.flavor_dir.join("WTF"), &synced).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&synced, s.flavor_dir.join("WTF")).unwrap();
        #[cfg(windows)]
        assert!(std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(s.flavor_dir.join("WTF"))
            .arg(&synced)
            .status()
            .unwrap()
            .success());

        let game = GameRoot::new(&s.flavor_dir).unwrap();
        let summary = s
            .service
            .create(
                SnapshotRequest {
                    game: &game,
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
            .unwrap();
        assert!(summary.file_count > 10);
        let m = s.service.manifest(&summary.id).unwrap();
        assert!(m.files.iter().all(|f| f.path.starts_with("WTF/")));
    }
}
