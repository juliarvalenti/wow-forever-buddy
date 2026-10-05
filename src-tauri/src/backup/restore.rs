//! Restore (spec §5): put a snapshot's files back for a selection of
//! accounts, characters, categories or addons, safely.
//!
//! `plan` works out what a restore would do without touching anything.
//! `run` re-plans against the folder as it is now, refuses before any change
//! if a target is read-only or the backup is damaged, then changes files only
//! through the write gate (which refuses while WoW runs and takes the
//! pre-restore snapshot), with a journal so a crash mid-restore can be rolled
//! back or finished on the next start.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::backup::journal::{self, Journal};
use crate::backup::manifest::{Manifest, ManifestFile, Scope};
use crate::backup::store::{BlobFault, BlobStore};
use crate::backup::tree::Category;
use crate::backup::BackupService;
use crate::error::{AppError, AppResult};
use crate::fsx::read::safe_read;
use crate::fsx::relpath::{GameRoot, RelPath};
use crate::game::gate::{MutationTarget, WriteGate};
use crate::state::AppCore;

/// What to restore. A file is restored if any item matches it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct RestoreSelection {
    pub items: Vec<ScopeItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind")]
pub enum ScopeItem {
    /// Every file in the snapshot.
    Everything,
    /// Account-wide files (not its characters). `None` = every category.
    Account {
        account: String,
        categories: Option<Vec<Category>>,
    },
    /// One character's folder. `None` = every category.
    Character {
        account: String,
        realm: String,
        character: String,
        categories: Option<Vec<Category>>,
    },
    /// One addon's SavedVariables (`<addon>.lua` and `.lua.bak`).
    AddonData { addon: String, target: AddonTarget },
    /// Exact files or folders, relative to the flavor folder.
    Paths { paths: Vec<RelPath> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind")]
pub enum AddonTarget {
    Account {
        account: String,
    },
    Character {
        account: String,
        realm: String,
        character: String,
    },
    Everywhere,
}

/// Overlay writes the snapshot's files and leaves others alone (the default
/// for every scope). Mirror also removes files under the scope that the
/// snapshot doesn't have.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMode {
    #[default]
    Overlay,
    Mirror,
}

/// Files to write in one folder, for "…\Thrandor\SavedVariables\ (41 files)".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PlanFolder {
    /// Relative to the flavor folder, `/`-separated.
    pub folder: String,
    pub files: Vec<String>,
    pub bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct RestorePlan {
    pub snapshot_id: String,
    pub mode: RestoreMode,
    pub write: Vec<PlanFolder>,
    pub write_count: u32,
    /// Shown prominently, with its own count, in the confirm dialog.
    pub delete: Vec<String>,
    /// Files that already match the snapshot.
    pub unchanged: u32,
    pub bytes: f64,
    /// Targets marked read-only. The restore refuses until the flag is
    /// cleared; players do this on purpose (e.g. to pin Config.wtf).
    pub read_only: Vec<String>,
    /// Files (or folders) in the selection that the snapshot couldn't read
    /// at the time: not in this backup, so left as they are, never deleted.
    pub not_backed_up: Vec<String>,
    /// "keybindings, macros and 41 addon settings".
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct RestoreReport {
    pub snapshot_id: String,
    /// The safety snapshot taken before anything changed; `None` if there
    /// was nothing to do.
    pub pre_restore_snapshot: Option<String>,
    pub written: u32,
    pub deleted: u32,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct VerifyReport {
    pub snapshot_id: String,
    pub files: u32,
    /// Files whose stored copy is missing, unreadable or fails its checksum.
    pub corrupt: Vec<String>,
    /// Those of `corrupt` whose stored copy is gone.
    pub missing: Vec<String>,
    /// Those of `corrupt` whose copy couldn't be read right now (retry).
    pub unreadable: Vec<String>,
}

/// What a restore resolves to.
#[derive(Debug, Default)]
struct Resolved {
    writes: Vec<(RelPath, ManifestFile)>,
    deletes: Vec<RelPath>,
    unchanged: u32,
    read_only: Vec<String>,
    not_backed_up: Vec<String>,
}

/// Works out what restoring `selection` from `manifest` would do. Touches
/// nothing.
pub fn plan(
    manifest: &Manifest,
    game: &GameRoot,
    selection: &RestoreSelection,
    mode: RestoreMode,
) -> AppResult<RestorePlan> {
    let resolved = resolve(manifest, game, selection, mode)?;
    let mut folders: BTreeMap<String, PlanFolder> = BTreeMap::new();
    for (rel, file) in &resolved.writes {
        let path = rel.as_string();
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
        let entry = folders
            .entry(folder.to_string())
            .or_insert_with(|| PlanFolder {
                folder: folder.to_string(),
                files: Vec::new(),
                bytes: 0.0,
            });
        entry.files.push(name.to_string());
        entry.bytes += file.size as f64;
    }
    Ok(RestorePlan {
        snapshot_id: manifest.id.clone(),
        mode,
        write_count: resolved.writes.len() as u32,
        bytes: resolved.writes.iter().map(|(_, f)| f.size as f64).sum(),
        write: folders.into_values().collect(),
        delete: resolved.deletes.iter().map(RelPath::as_string).collect(),
        unchanged: resolved.unchanged,
        read_only: resolved.read_only.clone(),
        not_backed_up: resolved.not_backed_up.clone(),
        summary: summarize(&resolved),
    })
}

/// Checks every stored copy in a snapshot against its checksum.
pub fn verify(backups: &BackupService, manifest: &Manifest) -> VerifyReport {
    let found = corrupt_files(backups.blobs(), manifest.files.iter());
    VerifyReport {
        snapshot_id: manifest.id.clone(),
        files: manifest.files.len() as u32,
        corrupt: found.files,
        missing: found.missing,
        unreadable: found.unreadable,
    }
}

/// Files whose stored copy can't be used, by why. Paths only.
#[derive(Debug, Default)]
pub struct CorruptFiles {
    pub files: Vec<String>,
    pub missing: Vec<String>,
    pub unreadable: Vec<String>,
}

impl CorruptFiles {
    pub fn add(&mut self, path: &str, fault: BlobFault) {
        self.files.push(path.to_string());
        match fault {
            BlobFault::Missing => self.missing.push(path.to_string()),
            BlobFault::Unreadable => self.unreadable.push(path.to_string()),
            BlobFault::Damaged => {}
        }
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn into_error(self) -> AppError {
        AppError::BackupCorrupt {
            files: self.files,
            missing: self.missing,
            unreadable: self.unreadable,
        }
    }
}

/// The files whose stored copy is missing, unreadable or fails its
/// checksum. Each blob is read once, even if several files share it.
fn corrupt_files<'f>(
    blobs: &BlobStore,
    files: impl Iterator<Item = &'f ManifestFile>,
) -> CorruptFiles {
    let mut fault: HashMap<&str, Option<BlobFault>> = HashMap::new();
    let mut found = CorruptFiles::default();
    for f in files {
        if let Some(why) = *fault
            .entry(&f.blake3)
            .or_insert_with(|| blobs.check(&f.blake3))
        {
            found.add(&f.path, why);
        }
    }
    found
}

/// How to recover an interrupted restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    RollBack,
    Finish,
}

impl Recovery {
    /// The snapshot, selection and mode the recovery restores.
    fn target(self, journal: &Journal) -> (&str, RestoreSelection, RestoreMode) {
        match self {
            // The pre-restore snapshot is partial: restoring all of it writes
            // the original files back and removes the ones the restore created.
            Recovery::RollBack => (
                &journal.original_pre_restore,
                RestoreSelection {
                    items: vec![ScopeItem::Everything],
                },
                RestoreMode::Overlay,
            ),
            Recovery::Finish => (
                &journal.source_snapshot,
                journal.selection.clone(),
                journal.mode,
            ),
        }
    }
}

/// Everything `run` needs from the app.
pub struct Restorer<'a> {
    pub backups: &'a BackupService,
    pub gate: &'a WriteGate,
    pub target: &'a MutationTarget,
    /// The active flavor; a snapshot of another flavor is refused.
    pub flavor: &'a str,
    /// Folder holding the restore journal (app data).
    pub journal_dir: &'a Path,
}

/// Builds a `Restorer` for the active game folder and runs `f` with it.
pub fn with_restorer<T>(
    core: &AppCore,
    f: impl FnOnce(&Restorer<'_>) -> AppResult<T>,
) -> AppResult<T> {
    let backups = core.backups()?;
    let gate = core.write_gate()?;
    let target = core.mutation_target()?;
    let game = core.active_game()?;
    f(&Restorer {
        backups: &backups,
        gate: &gate,
        target: &target,
        flavor: &game.flavor,
        journal_dir: &core.paths.local_data_dir,
    })
}

/// Progress callback: (done, total). Returning an error stops the restore
/// where it is, leaving the journal for recovery (tests use this to simulate
/// a crash).
pub type RestoreProgress<'a> = &'a mut dyn FnMut(u32, u32) -> AppResult<()>;

impl Restorer<'_> {
    /// Restores `selection` from snapshot `id`. Refused with `RestorePending`
    /// while an interrupted restore's journal exists: a new restore would
    /// snapshot the half-restored tree and replace the journal, so "roll
    /// back" could never reach the original state again. Only
    /// `roll_back`/`finish`/`discard` may proceed then.
    ///
    /// `confirmed_deletes` is the deletion list the user saw and confirmed in
    /// the preview. If the run would delete anything else (say WoW wrote a new
    /// file at logout after the preview), it's refused with
    /// `DeletionsChanged` before any change, so a confirmed list is a
    /// guarantee rather than a promise.
    pub fn run(
        &self,
        id: &str,
        selection: &RestoreSelection,
        mode: RestoreMode,
        confirmed_deletes: &[RelPath],
        progress: RestoreProgress<'_>,
    ) -> AppResult<RestoreReport> {
        // An unreadable journal counts as pending too; `discard` clears it.
        if journal::read(self.journal_dir).map_or(true, |j| j.is_some()) {
            return Err(AppError::RestorePending);
        }
        self.execute(id, selection, mode, Some(confirmed_deletes), None, progress)
    }

    /// What recovering an interrupted restore would do, for the same
    /// confirm-the-deletions step as a normal restore. Touches nothing.
    pub fn recovery_plan(&self, journal: &Journal, how: Recovery) -> AppResult<RestorePlan> {
        let (id, selection, mode) = how.target(journal);
        plan(
            &self.backups.manifest(id)?,
            &self.target.game,
            &selection,
            mode,
        )
    }

    /// After a crash mid-restore, either
    /// - `RollBack`: put back what was there before the user's restore
    ///   started, even if an earlier recovery was itself interrupted; or
    /// - `Finish`: finish the user's restore (files already written count as
    ///   unchanged).
    ///
    /// Like `run`, refused with `DeletionsChanged` if it would delete a file
    /// not in `confirmed_deletes` (from `recovery_plan`). This matters: after
    /// a play session, finishing a mirror restore would otherwise remove the
    /// SavedVariables WoW wrote meanwhile without anyone seeing the list.
    pub fn recover(
        &self,
        journal: &Journal,
        how: Recovery,
        confirmed_deletes: &[RelPath],
        progress: RestoreProgress<'_>,
    ) -> AppResult<RestoreReport> {
        let (id, selection, mode) = how.target(journal);
        let report = self.execute(
            id,
            &selection,
            mode,
            Some(confirmed_deletes),
            Some(journal),
            progress,
        )?;
        journal::clear(self.journal_dir)?;
        Ok(report)
    }

    /// Forgets an interrupted restore without changing any files, e.g. when
    /// the journal is unreadable. Its pre-restore snapshot stays in the
    /// Safety list, so the user can still restore it by hand.
    pub fn discard(&self) -> AppResult<()> {
        journal::clear(self.journal_dir)
    }

    /// The restore itself. `resuming` is the interrupted restore's journal
    /// when recovering: the journal written for this run keeps its source,
    /// selection and original pre-restore snapshot, so another interruption
    /// still rolls back to the state before the user's restore.
    fn execute(
        &self,
        id: &str,
        selection: &RestoreSelection,
        mode: RestoreMode,
        confirmed_deletes: Option<&[RelPath]>,
        resuming: Option<&Journal>,
        progress: RestoreProgress<'_>,
    ) -> AppResult<RestoreReport> {
        let manifest = self.backups.manifest(id)?;
        if !manifest.flavor.eq_ignore_ascii_case(self.flavor) {
            return Err(AppError::InvalidInstall(format!(
                "this backup is of {}, but the active game folder is {}",
                manifest.flavor, self.flavor
            )));
        }
        // Re-plan against the folder as it is now; never trust an old preview.
        let resolved = resolve(&manifest, &self.target.game, selection, mode)?;
        let summary = summarize(&resolved);
        if !resolved.read_only.is_empty() {
            return Err(AppError::ReadOnly {
                paths: resolved.read_only,
            });
        }
        if let Some(confirmed) = confirmed_deletes {
            // Case-insensitive: folder casing can differ between preview and
            // run on Windows.
            let confirmed: HashSet<String> = confirmed
                .iter()
                .map(|p| p.as_string().to_lowercase())
                .collect();
            let unconfirmed: Vec<String> = resolved
                .deletes
                .iter()
                .map(RelPath::as_string)
                .filter(|p| !confirmed.contains(&p.to_lowercase()))
                .collect();
            if !unconfirmed.is_empty() {
                return Err(AppError::DeletionsChanged { paths: unconfirmed });
            }
        }
        if resolved.writes.is_empty() && resolved.deletes.is_empty() {
            return Ok(RestoreReport {
                snapshot_id: manifest.id,
                pre_restore_snapshot: None,
                written: 0,
                deleted: 0,
                summary,
            });
        }
        // A damaged backup stops here, before anything changes.
        let corrupt = corrupt_files(self.backups.blobs(), resolved.writes.iter().map(|(_, f)| f));
        if !corrupt.is_empty() {
            return Err(corrupt.into_error());
        }

        let touched: Vec<RelPath> = resolved
            .writes
            .iter()
            .map(|(rel, _)| rel.clone())
            .chain(resolved.deletes.iter().cloned())
            .collect();
        let label = short(&format!("Before restoring {summary}"));
        // Refuses while WoW runs; takes the pre-restore snapshot.
        let guard = self.gate.begin("restore", self.target, &touched, &label)?;
        let pre_restore = guard.snapshot_id().to_string();
        let entry = match resuming {
            Some(original) => Journal {
                pre_restore_snapshot: pre_restore,
                ..original.clone()
            },
            None => Journal {
                source_snapshot: manifest.id.clone(),
                original_pre_restore: pre_restore.clone(),
                pre_restore_snapshot: pre_restore,
                selection: selection.clone(),
                mode,
                flavor: manifest.flavor.clone(),
                started_at: chrono::Utc::now().to_rfc3339(),
                summary: summary.clone(),
            },
        };
        journal::write(self.journal_dir, &entry)?;

        let total = touched.len() as u32;
        let mut done = 0;
        for (rel, file) in &resolved.writes {
            let bytes = self.backups.blobs().get(&file.blake3)?;
            guard.write(rel, &bytes)?;
            done += 1;
            progress(done, total)?;
        }
        for rel in &resolved.deletes {
            guard.remove(rel)?;
            done += 1;
            progress(done, total)?;
        }

        let pre_restore = guard.snapshot_id().to_string();
        guard.commit()?;
        journal::clear(self.journal_dir)?;
        Ok(RestoreReport {
            snapshot_id: manifest.id,
            pre_restore_snapshot: Some(pre_restore),
            written: resolved.writes.len() as u32,
            deleted: resolved.deletes.len() as u32,
            summary,
        })
    }
}

/// Snapshot labels stay short enough for the Backups list.
fn short(label: &str) -> String {
    let mut out: String = label.chars().take(120).collect();
    if out.len() < label.len() {
        out.push('…');
    }
    out
}

// Resolution -----------------------------------------------------------------

/// Where a file sits in the WTF tree, by the same rules as the restore
/// panel's tree (`tree::detail`).
enum Place<'p> {
    Account(&'p str),
    Character(&'p str, &'p str, &'p str),
    Outside,
}

/// The file's place and the path parts below it.
fn place<'p>(parts: &'p [&'p str]) -> (Place<'p>, &'p [&'p str]) {
    let n = parts.len();
    let in_account =
        n >= 4 && parts[0].eq_ignore_ascii_case("WTF") && parts[1].eq_ignore_ascii_case("Account");
    if !in_account {
        return (Place::Outside, parts);
    }
    if n >= 6 && Category::of(&parts[3..]) == Category::Other {
        (Place::Character(parts[2], parts[3], parts[4]), &parts[5..])
    } else {
        (Place::Account(parts[2]), &parts[3..])
    }
}

/// Folder and file names compare case-insensitively, like Windows.
fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn in_categories(categories: &Option<Vec<Category>>, rest: &[&str]) -> bool {
    categories
        .as_ref()
        .is_none_or(|c| c.contains(&Category::of(rest)))
}

fn matches(item: &ScopeItem, path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let (place, rest) = place(&parts);
    match item {
        ScopeItem::Everything => true,
        ScopeItem::Account {
            account,
            categories,
        } => {
            matches!(place, Place::Account(a) if same(a, account))
                && in_categories(categories, rest)
        }
        ScopeItem::Character {
            account,
            realm,
            character,
            categories,
        } => {
            matches!(place, Place::Character(a, r, c)
                if same(a, account) && same(r, realm) && same(c, character))
                && in_categories(categories, rest)
        }
        ScopeItem::AddonData { addon, target } => {
            let is_addon_file = rest.len() == 2
                && same(rest[0], "SavedVariables")
                && [".lua", ".lua.bak"]
                    .iter()
                    .any(|ext| same(rest[1], &format!("{addon}{ext}")));
            is_addon_file
                && match (target, &place) {
                    (AddonTarget::Everywhere, Place::Account(_) | Place::Character(..)) => true,
                    (AddonTarget::Account { account }, Place::Account(a)) => same(a, account),
                    (
                        AddonTarget::Character {
                            account,
                            realm,
                            character,
                        },
                        Place::Character(a, r, c),
                    ) => same(a, account) && same(r, realm) && same(c, character),
                    _ => false,
                }
        }
        ScopeItem::Paths { paths } => paths.iter().any(|p| {
            let p = p.as_string().to_lowercase();
            let path = path.to_lowercase();
            path == p || path.starts_with(&format!("{p}/"))
        }),
    }
}

fn selected(selection: &RestoreSelection, path: &str) -> bool {
    selection.items.iter().any(|item| matches(item, path))
}

fn resolve(
    manifest: &Manifest,
    game: &GameRoot,
    selection: &RestoreSelection,
    mode: RestoreMode,
) -> AppResult<Resolved> {
    let mut out = Resolved::default();
    let in_snapshot: HashSet<String> = manifest
        .files
        .iter()
        .map(|f| f.path.to_lowercase())
        .collect();

    for file in manifest
        .files
        .iter()
        .filter(|f| selected(selection, &f.path))
    {
        let rel = RelPath::new(&file.path)?;
        let abs = rel.resolve(game)?;
        if abs.is_file() {
            let meta = std::fs::metadata(&abs)?;
            if meta.len() == file.size && BlobStore::hash(&safe_read(&abs)?) == file.blake3 {
                out.unchanged += 1;
                continue;
            }
            if meta.permissions().readonly() {
                out.read_only.push(file.path.clone());
            }
        }
        out.writes.push((rel, file.clone()));
    }

    // Paths a partial (safety) snapshot recorded as not existing yet: the
    // change created them, so undoing it removes them.
    let mut deletes: Vec<RelPath> = Vec::new();
    for path in manifest.absent.iter().filter(|p| selected(selection, p)) {
        let rel = RelPath::new(path)?;
        deletes.extend(existing_files(game, &rel)?);
    }
    // Mirror: also remove current files in scope that the snapshot lacks.
    // A file the snapshot *skipped* (couldn't read) isn't lacking, it was
    // never copied: deleting it would destroy a file with no backup. So
    // skipped paths, and anything under a skipped folder, are kept; and a
    // skip with no path at all (a walk error) means no Mirror deletions.
    let skipped: Vec<String> = manifest
        .skipped
        .iter()
        .map(|s| s.path.to_lowercase())
        .collect();
    let mirror_safe = !skipped.iter().any(String::is_empty);
    out.not_backed_up = manifest
        .skipped
        .iter()
        .filter(|s| !s.path.is_empty() && selected(selection, &s.path))
        .map(|s| s.path.clone())
        .collect();
    let was_skipped = |path: &str| {
        let path = path.to_lowercase();
        skipped.iter().any(|s| {
            path == *s
                || path
                    .strip_prefix(s.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
    };
    if mode == RestoreMode::Mirror && manifest.scope == Scope::Full && mirror_safe {
        let mut roots = vec![RelPath::new("WTF")?];
        if manifest.include_addons {
            roots.push(RelPath::new("Interface/AddOns")?);
        }
        for root in roots {
            for rel in existing_files(game, &root)? {
                let path = rel.as_string();
                if selected(selection, &path)
                    && !in_snapshot.contains(&path.to_lowercase())
                    && !was_skipped(&path)
                {
                    deletes.push(rel);
                }
            }
        }
    }
    deletes.sort();
    deletes.dedup();
    let writing: HashSet<String> = out.writes.iter().map(|(r, _)| r.as_string()).collect();
    for rel in deletes {
        if writing.contains(&rel.as_string()) {
            continue;
        }
        let abs = rel.resolve(game)?;
        if std::fs::metadata(&abs).is_ok_and(|m| m.permissions().readonly()) {
            out.read_only.push(rel.as_string());
        }
        out.deletes.push(rel);
    }
    out.read_only.sort();
    Ok(out)
}

/// Files at or under `rel` right now (a file, or every file in a folder).
/// Links inside the tree are skipped, never followed, like backups do.
fn existing_files(game: &GameRoot, rel: &RelPath) -> AppResult<Vec<RelPath>> {
    let abs = rel.resolve(game)?;
    if abs.is_file() {
        return Ok(vec![rel.clone()]);
    }
    if !abs.is_dir() {
        return Ok(Vec::new());
    }
    // An unreadable subfolder, or a name `RelPath` refuses (`CON.lua`,
    // non-UTF-8), is skipped like backups skip it: such a file can't be in
    // any snapshot, and leaving it out only ever means *not* deleting it.
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(&abs)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if !entry.file_type().is_file() || entry.file_name().to_string_lossy().contains(".wfb-tmp-")
        {
            continue;
        }
        if let Ok(rel) = RelPath::from_under(&game.base, entry.path()) {
            files.push(rel);
        }
    }
    Ok(files)
}

/// "keybindings, macros and 41 addon settings", plus removals.
fn summarize(resolved: &Resolved) -> String {
    let (mut bindings, mut macros, mut addons, mut chat, mut other) = (false, false, 0, false, 0);
    for (rel, _) in &resolved.writes {
        let path = rel.as_string();
        let parts: Vec<&str> = path.split('/').collect();
        let (_, rest) = place(&parts);
        let name = parts.last().copied().unwrap_or_default().to_lowercase();
        match Category::of(rest) {
            Category::BindingsMacros if name == "bindings-cache.wtf" => bindings = true,
            Category::BindingsMacros => macros = true,
            Category::AddonSettings if name.ends_with(".lua") => addons += 1,
            Category::AddonSettings => {}
            Category::ChatLayout => chat = true,
            Category::Other => other += 1,
        }
    }
    let plural = |n: u32, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    let mut items: Vec<String> = Vec::new();
    if bindings {
        items.push("keybindings".into());
    }
    if macros {
        items.push("macros".into());
    }
    if addons > 0 {
        items.push(plural(addons, "addon setting", "addon settings"));
    }
    if chat {
        items.push("chat and layout".into());
    }
    if other > 0 {
        items.push(plural(other, "other file", "other files"));
    }
    let mut text = match items.len() {
        0 => String::new(),
        1 => items.remove(0),
        _ => {
            let last = items.pop().unwrap();
            format!("{} and {last}", items.join(", "))
        }
    };
    let deletes = resolved.deletes.len() as u32;
    if deletes > 0 {
        let removal = format!("removes {}", plural(deletes, "file", "files"));
        text = if text.is_empty() {
            removal
        } else {
            format!("{text}; {removal}")
        };
    }
    if text.is_empty() {
        text = "nothing to restore".into();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::manifest::{SkippedFile, Trigger};
    use crate::backup::{SnapshotRequest, SnapshotScope};
    use crate::config::paths::AppPaths;
    use crate::game::process::fake::FakeProbe;
    use crate::secrets::MemoryStore;
    use crate::test_support::fixture_copy;
    use std::path::PathBuf;
    use std::sync::Arc;

    const ACCT: &str = "WTF/Account/ACCOUNT1";
    const THRANDOR: &str = "WTF/Account/ACCOUNT1/Ashenvale/Thrandor";

    struct T {
        core: AppCore,
        probe: Arc<FakeProbe>,
        flavor: PathBuf,
        _game: tempfile::TempDir,
        _app: tempfile::TempDir,
    }

    fn setup() -> T {
        let (game, root) = fixture_copy();
        let app = tempfile::tempdir().unwrap();
        let probe = Arc::new(FakeProbe::default());
        let core = AppCore::with_parts(
            AppPaths::under(app.path()),
            Arc::new(MemoryStore::default()),
            probe.clone(),
        )
        .unwrap();
        crate::install::set(&core.settings, &root, Some("_classic_beta_")).unwrap();
        T {
            core,
            probe,
            flavor: root.join("_classic_beta_"),
            _game: game,
            _app: app,
        }
    }

    fn snapshot(t: &T) -> String {
        let game = t.core.active_game().unwrap();
        t.core
            .backups()
            .unwrap()
            .create(
                SnapshotRequest {
                    game: &game.root,
                    flavor: &game.flavor,
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
            .id
    }

    fn sel(items: Vec<ScopeItem>) -> RestoreSelection {
        RestoreSelection { items }
    }

    fn everything() -> RestoreSelection {
        sel(vec![ScopeItem::Everything])
    }

    fn preview(t: &T, id: &str, s: &RestoreSelection, mode: RestoreMode) -> RestorePlan {
        let game = t.core.active_game().unwrap();
        let manifest = t.core.backups().unwrap().manifest(id).unwrap();
        plan(&manifest, &game.root, s, mode).unwrap()
    }

    /// The deletions a user would confirm: the preview's list, as the UI
    /// sends it.
    fn confirmed(t: &T, id: &str, s: &RestoreSelection, mode: RestoreMode) -> Vec<RelPath> {
        let game = t.core.active_game().unwrap();
        let manifest = t.core.backups().unwrap().manifest(id).unwrap();
        plan(&manifest, &game.root, s, mode)
            .map(|p| p.delete.iter().map(|d| RelPath::new(d).unwrap()).collect())
            .unwrap_or_default()
    }

    /// Previews, then restores with that preview's deletions confirmed.
    fn restore(
        t: &T,
        id: &str,
        s: &RestoreSelection,
        mode: RestoreMode,
    ) -> AppResult<RestoreReport> {
        let ok = confirmed(t, id, s, mode);
        with_restorer(&t.core, |r| r.run(id, s, mode, &ok, &mut |_, _| Ok(())))
    }

    /// Every file under WTF, by relative path.
    fn tree(t: &T) -> BTreeMap<String, Vec<u8>> {
        let wtf = t.flavor.join("WTF");
        walkdir::WalkDir::new(&wtf)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| {
                let rel = e.path().strip_prefix(&t.flavor).unwrap();
                let rel = rel.to_string_lossy().replace('\\', "/");
                (rel, std::fs::read(e.path()).unwrap())
            })
            .collect()
    }

    fn path(t: &T, rel: &str) -> PathBuf {
        rel.split('/').fold(t.flavor.clone(), |p, c| p.join(c))
    }

    fn write(t: &T, rel: &str, text: &str) {
        let p = path(t, rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn snapshots(t: &T) -> usize {
        t.core.backups().unwrap().list().unwrap().len()
    }

    /// Edits one file, deletes one and adds one, as WoW and the user might.
    fn mutate(t: &T) {
        write(t, "WTF/Config.wtf", "SET gxWindow \"0\"\n");
        std::fs::remove_file(path(t, &format!("{ACCT}/SavedVariables/WeakAuras.lua"))).unwrap();
        write(
            t,
            &format!("{ACCT}/SavedVariables/NewAddon.lua"),
            "NewAddonDB = {}\n",
        );
    }

    #[test]
    fn mirror_puts_the_tree_back_exactly() {
        let t = setup();
        let original = tree(&t);
        let id = snapshot(&t);
        mutate(&t);

        let plan = preview(&t, &id, &everything(), RestoreMode::Mirror);
        assert_eq!(plan.write_count, 2);
        assert_eq!(plan.delete, [format!("{ACCT}/SavedVariables/NewAddon.lua")]);
        assert_eq!(plan.unchanged as usize, original.len() - 2);
        assert_eq!(
            plan.summary,
            "1 addon setting and 1 other file; removes 1 file"
        );

        let report = restore(&t, &id, &everything(), RestoreMode::Mirror).unwrap();
        assert_eq!((report.written, report.deleted), (2, 1));
        assert_eq!(tree(&t), original);

        // The safety snapshot was taken first, and the journal is gone.
        let pre = report.pre_restore_snapshot.unwrap();
        let pre = t.core.backups().unwrap().manifest(&pre).unwrap();
        assert_eq!(pre.trigger, Trigger::PreRestore);
        assert!(journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .is_none());
    }

    /// #27 review blocker: a file (or folder) the snapshot skipped was never
    /// copied, so Mirror must not delete it as "not in that snapshot"; and a
    /// skip without a path means no Mirror deletions at all.
    #[test]
    fn mirror_never_deletes_what_the_snapshot_skipped() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t); // adds NewAddon.lua, which Mirror would remove
        let store = t.core.backups().unwrap();
        let mut m = store.manifest(&id).unwrap();
        let config = "WTF/Config.wtf";
        let skip = |m: &mut Manifest, path: &str| {
            m.files
                .retain(|f| f.path != path && !f.path.starts_with(&format!("{path}/")));
            m.skipped.push(SkippedFile {
                path: path.into(),
                reason: "locked".into(),
            });
        };
        skip(&mut m, config);
        skip(&mut m, &THRANDOR.to_uppercase()); // case-insensitive
        store.manifests.write(&m).unwrap();

        let plan = preview(&t, &id, &everything(), RestoreMode::Mirror);
        assert_eq!(
            plan.delete,
            [format!("{ACCT}/SavedVariables/NewAddon.lua")],
            "only the truly new file; nothing skipped"
        );
        assert_eq!(
            plan.not_backed_up,
            [config.to_string(), THRANDOR.to_uppercase()],
            "listed as left as is"
        );

        m.skipped.push(SkippedFile {
            path: String::new(),
            reason: "walk error".into(),
        });
        store.manifests.write(&m).unwrap();
        let plan = preview(&t, &id, &everything(), RestoreMode::Mirror);
        assert!(plan.delete.is_empty(), "a pathless skip: no Mirror deletes");
    }

    #[test]
    fn deletions_beyond_the_confirmed_list_are_refused() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        // The user previews and confirms the mirror deletions shown...
        let ok = confirmed(&t, &id, &everything(), RestoreMode::Mirror);
        assert_eq!(ok.len(), 1);
        // ...then WoW writes a new file at logout, before Restore is clicked.
        write(
            &t,
            &format!("{THRANDOR}/SavedVariables/FromLogout.lua"),
            "X = 1\n",
        );
        let before = tree(&t);
        let snapshots_before = snapshots(&t);

        let err = with_restorer(&t.core, |r| {
            r.run(&id, &everything(), RestoreMode::Mirror, &ok, &mut |_, _| {
                Ok(())
            })
        })
        .unwrap_err();
        assert!(
            matches!(&err, AppError::DeletionsChanged { paths }
                if paths == &[format!("{THRANDOR}/SavedVariables/FromLogout.lua")]),
            "{err}"
        );
        assert_eq!(tree(&t), before);
        assert_eq!(snapshots(&t), snapshots_before);

        // Confirmed paths match case-insensitively (Windows folder casing).
        let shouting: Vec<RelPath> = confirmed(&t, &id, &everything(), RestoreMode::Mirror)
            .iter()
            .map(|p| RelPath::new(&p.as_string().to_uppercase()).unwrap())
            .collect();
        with_restorer(&t.core, |r| {
            r.run(
                &id,
                &everything(),
                RestoreMode::Mirror,
                &shouting,
                &mut |_, _| Ok(()),
            )
        })
        .unwrap();
    }

    #[test]
    fn overlay_of_a_full_snapshot_never_deletes() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        for s in [
            everything(),
            sel(vec![ScopeItem::Account {
                account: "ACCOUNT1".into(),
                categories: None,
            }]),
        ] {
            assert!(preview(&t, &id, &s, RestoreMode::Overlay).delete.is_empty());
        }
    }

    #[test]
    fn overlay_leaves_extra_files_alone() {
        let t = setup();
        let id = snapshot(&t);
        let original = tree(&t);
        mutate(&t);

        let plan = preview(&t, &id, &everything(), RestoreMode::Overlay);
        assert!(plan.delete.is_empty());
        restore(&t, &id, &everything(), RestoreMode::Overlay).unwrap();

        let mut expected = original;
        expected.insert(
            format!("{ACCT}/SavedVariables/NewAddon.lua"),
            b"NewAddonDB = {}\n".to_vec(),
        );
        assert_eq!(tree(&t), expected);
    }

    #[test]
    fn nothing_to_do_takes_no_snapshot() {
        let t = setup();
        let id = snapshot(&t);
        let before = snapshots(&t);
        let report = restore(&t, &id, &everything(), RestoreMode::Mirror).unwrap();
        assert_eq!((report.written, report.deleted), (0, 0));
        assert_eq!(report.pre_restore_snapshot, None);
        assert_eq!(report.summary, "nothing to restore");
        assert_eq!(snapshots(&t), before);
    }

    #[test]
    fn character_scope_honours_categories() {
        let t = setup();
        let id = snapshot(&t);
        write(&t, &format!("{THRANDOR}/macros-cache.txt"), "changed");
        write(
            &t,
            &format!("{THRANDOR}/SavedVariables/Details.lua"),
            "changed",
        );
        write(
            &t,
            "WTF/Account/ACCOUNT1/Ashenvale/Velyra/AddOns.txt",
            "changed",
        );

        let only_macros = sel(vec![ScopeItem::Character {
            account: "account1".into(), // case-insensitive, like Windows
            realm: "Ashenvale".into(),
            character: "thrandor".into(),
            categories: Some(vec![Category::BindingsMacros]),
        }]);
        let plan = preview(&t, &id, &only_macros, RestoreMode::Overlay);
        assert_eq!(plan.write_count, 1);
        assert_eq!(plan.write[0].folder, THRANDOR);
        assert_eq!(plan.write[0].files, ["macros-cache.txt"]);
        assert_eq!(plan.summary, "macros");

        let whole = sel(vec![ScopeItem::Character {
            account: "ACCOUNT1".into(),
            realm: "Ashenvale".into(),
            character: "Thrandor".into(),
            categories: None,
        }]);
        assert_eq!(
            preview(&t, &id, &whole, RestoreMode::Overlay).write_count,
            2
        );
    }

    #[test]
    fn account_scope_is_account_wide_files_only() {
        let t = setup();
        let id = snapshot(&t);
        write(&t, &format!("{ACCT}/bindings-cache.wtf"), "changed");
        write(&t, &format!("{THRANDOR}/bindings-cache.wtf"), "changed");
        let account = sel(vec![ScopeItem::Account {
            account: "ACCOUNT1".into(),
            categories: None,
        }]);
        let plan = preview(&t, &id, &account, RestoreMode::Overlay);
        assert_eq!(plan.write_count, 1);
        assert_eq!(plan.write[0].folder, ACCT);
        assert_eq!(plan.summary, "keybindings");
    }

    #[test]
    fn addon_data_by_target() {
        let t = setup();
        let id = snapshot(&t);
        write(&t, &format!("{ACCT}/SavedVariables/Details.lua"), "changed");
        write(
            &t,
            &format!("{THRANDOR}/SavedVariables/Details.lua"),
            "changed",
        );
        write(
            &t,
            &format!("{THRANDOR}/SavedVariables/WeakAuras.lua"),
            "changed",
        );

        let addon = |target| {
            sel(vec![ScopeItem::AddonData {
                addon: "details".into(),
                target,
            }])
        };
        let everywhere = preview(
            &t,
            &id,
            &addon(AddonTarget::Everywhere),
            RestoreMode::Overlay,
        );
        assert_eq!(everywhere.write_count, 2);
        assert_eq!(everywhere.summary, "2 addon settings");

        let account = AddonTarget::Account {
            account: "ACCOUNT1".into(),
        };
        let plan = preview(&t, &id, &addon(account), RestoreMode::Overlay);
        assert_eq!(plan.write[0].folder, format!("{ACCT}/SavedVariables"));

        let character = AddonTarget::Character {
            account: "ACCOUNT1".into(),
            realm: "Ashenvale".into(),
            character: "Thrandor".into(),
        };
        let plan = preview(&t, &id, &addon(character), RestoreMode::Overlay);
        assert_eq!(plan.write_count, 1);
        assert_eq!(plan.write[0].folder, format!("{THRANDOR}/SavedVariables"));
    }

    #[test]
    fn paths_scope() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        let paths = sel(vec![ScopeItem::Paths {
            paths: vec![RelPath::new("WTF/Config.wtf").unwrap()],
        }]);
        let plan = preview(&t, &id, &paths, RestoreMode::Mirror);
        assert_eq!(plan.write_count, 1);
        assert!(plan.delete.is_empty());
    }

    #[test]
    fn refused_while_wow_runs_and_nothing_changes() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        let mutated = tree(&t);
        let before = snapshots(&t);
        t.probe.set_running(true);

        let err = restore(&t, &id, &everything(), RestoreMode::Mirror).unwrap_err();
        assert!(matches!(err, AppError::GameRunning(_)), "{err}");
        assert_eq!(tree(&t), mutated);
        assert_eq!(snapshots(&t), before);
        assert!(journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .is_none());
    }

    #[test]
    fn read_only_targets_are_refused_before_any_write() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        let config = path(&t, "WTF/Config.wtf");
        let mut perms = std::fs::metadata(&config).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&config, perms.clone()).unwrap();
        let mutated = tree(&t);

        let plan = preview(&t, &id, &everything(), RestoreMode::Overlay);
        assert_eq!(plan.read_only, ["WTF/Config.wtf"]);
        let err = restore(&t, &id, &everything(), RestoreMode::Overlay).unwrap_err();
        assert!(
            matches!(&err, AppError::ReadOnly { paths } if paths == &["WTF/Config.wtf"]),
            "{err}"
        );
        assert_eq!(tree(&t), mutated);

        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&config, perms).unwrap();
    }

    #[test]
    fn a_damaged_backup_is_refused_before_any_write() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        let mutated = tree(&t);
        let backups = t.core.backups().unwrap();
        let manifest = backups.manifest(&id).unwrap();
        let config = manifest
            .files
            .iter()
            .find(|f| f.path == "WTF/Config.wtf")
            .unwrap();
        let blob = backups
            .dir()
            .join("objects")
            .join(&config.blake3[..2])
            .join(&config.blake3[2..]);
        std::fs::write(&blob, b"not zstd").unwrap();

        let report = verify(&backups, &manifest);
        assert_eq!(report.corrupt, ["WTF/Config.wtf"]);
        assert!(report.missing.is_empty(), "there but damaged, not missing");
        let err = restore(&t, &id, &everything(), RestoreMode::Mirror).unwrap_err();
        assert!(
            matches!(&err, AppError::BackupCorrupt { files, missing, unreadable }
                if files == &["WTF/Config.wtf"] && missing.is_empty() && unreadable.is_empty()),
            "{err}"
        );
        assert_eq!(tree(&t), mutated);
    }

    #[test]
    fn a_missing_stored_copy_is_reported_as_missing() {
        let t = setup();
        let id = snapshot(&t);
        let backups = t.core.backups().unwrap();
        let manifest = backups.manifest(&id).unwrap();
        let blob_of = |path: &str| {
            let f = manifest.files.iter().find(|f| f.path == path).unwrap();
            backups
                .dir()
                .join("objects")
                .join(&f.blake3[..2])
                .join(&f.blake3[2..])
        };
        // One copy deleted, another damaged: the dialog tells them apart.
        std::fs::remove_file(blob_of("WTF/Config.wtf")).unwrap();
        let macros = format!("{THRANDOR}/macros-cache.txt");
        std::fs::write(blob_of(&macros), b"not zstd").unwrap();

        let report = verify(&backups, &manifest);
        assert_eq!(report.missing, ["WTF/Config.wtf"]);
        assert!(report.corrupt.contains(&"WTF/Config.wtf".to_string()));
        assert!(report.corrupt.contains(&macros));

        // A restore only reads the copies it needs: Config.wtf differs now.
        mutate(&t);
        let err = restore(&t, &id, &everything(), RestoreMode::Overlay).unwrap_err();
        assert!(
            matches!(&err, AppError::BackupCorrupt { missing, .. } if missing == &["WTF/Config.wtf"]),
            "{err}"
        );
    }

    #[test]
    fn verify_reports_a_healthy_snapshot() {
        let t = setup();
        let id = snapshot(&t);
        let backups = t.core.backups().unwrap();
        let report = verify(&backups, &backups.manifest(&id).unwrap());
        assert!(report.corrupt.is_empty());
        assert!(report.files > 0);
    }

    /// The deletions a user would confirm in the recovery dialog.
    fn recovery_confirmed(t: &T, journal: &Journal, how: Recovery) -> Vec<RelPath> {
        with_restorer(&t.core, |r| r.recovery_plan(journal, how))
            .unwrap()
            .delete
            .iter()
            .map(|d| RelPath::new(d).unwrap())
            .collect()
    }

    /// Previews a recovery, then runs it with that preview's deletions confirmed.
    fn recover(t: &T, journal: &Journal, how: Recovery) -> AppResult<RestoreReport> {
        let ok = recovery_confirmed(t, journal, how);
        with_restorer(&t.core, |r| {
            r.recover(journal, how, &ok, &mut |_, _| Ok(()))
        })
    }

    /// Runs a restore that "crashes" after its first change.
    fn interrupted(t: &T, id: &str) {
        let ok = confirmed(t, id, &everything(), RestoreMode::Mirror);
        let err = with_restorer(&t.core, |r| {
            r.run(
                id,
                &everything(),
                RestoreMode::Mirror,
                &ok,
                &mut |done, _| {
                    if done == 1 {
                        Err(AppError::Io("simulated crash".into()))
                    } else {
                        Ok(())
                    }
                },
            )
        })
        .unwrap_err();
        assert!(err.to_string().contains("simulated crash"));
    }

    #[test]
    fn interrupted_restore_can_be_rolled_back() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        let mutated = tree(&t);
        interrupted(&t, &id);

        let journal = journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .expect("journal left behind");
        assert_eq!(journal.source_snapshot, id);
        assert_ne!(tree(&t), mutated);

        // Rolling back restores the pre-restore state, including removing
        // the deleted WeakAuras.lua the restore had put back.
        recover(&t, &journal, Recovery::RollBack).unwrap();
        assert_eq!(tree(&t), mutated);
        assert!(journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .is_none());
    }

    #[test]
    fn recovery_deletions_beyond_the_confirmed_list_are_refused() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        interrupted(&t, &id);
        let journal = journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .unwrap();
        let seen = recovery_confirmed(&t, &journal, Recovery::Finish);

        // The user plays before deciding; WoW saves a new addon's settings,
        // which finishing the mirror restore would remove.
        let new_sv = format!("{ACCT}/SavedVariables/FromPlaying.lua");
        write(&t, &new_sv, "FromPlayingDB = {}\n");
        let sv = path(&t, &new_sv);
        let before = tree(&t);
        let snapshots = t.core.backups().unwrap().list().unwrap().len();

        let err = with_restorer(&t.core, |r| {
            r.recover(&journal, Recovery::Finish, &seen, &mut |_, _| Ok(()))
        })
        .unwrap_err();
        assert!(
            matches!(&err, AppError::DeletionsChanged { paths }
                if paths.iter().any(|p| p.ends_with("FromPlaying.lua"))),
            "{err}"
        );
        // Nothing changed, no safety snapshot taken, and the journal stays
        // so the user can still decide.
        assert_eq!(tree(&t), before);
        assert_eq!(t.core.backups().unwrap().list().unwrap().len(), snapshots);
        assert_eq!(
            journal::read(&t.core.paths.local_data_dir).unwrap(),
            Some(journal.clone())
        );

        // Once the new list is seen, it goes ahead.
        recover(&t, &journal, Recovery::Finish).unwrap();
        assert!(!sv.exists());
    }

    #[test]
    fn interrupted_restore_can_be_finished() {
        let t = setup();
        let original = tree(&t);
        let id = snapshot(&t);
        mutate(&t);
        interrupted(&t, &id);

        let journal = journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .unwrap();
        recover(&t, &journal, Recovery::Finish).unwrap();
        assert_eq!(tree(&t), original);
        assert!(journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_new_restore_is_refused_while_one_is_pending() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        interrupted(&t, &id);
        let half_restored = tree(&t);
        let journal_before = journal::read(&t.core.paths.local_data_dir).unwrap();

        let err = restore(&t, &id, &everything(), RestoreMode::Overlay).unwrap_err();
        assert!(matches!(err, AppError::RestorePending), "{err}");
        assert_eq!(tree(&t), half_restored);
        assert_eq!(
            journal::read(&t.core.paths.local_data_dir).unwrap(),
            journal_before
        );

        // An unreadable journal blocks restores too, until it's discarded.
        let dir = &t.core.paths.local_data_dir;
        std::fs::write(dir.join(journal::FILE_NAME), b"{ damaged").unwrap();
        let err = restore(&t, &id, &everything(), RestoreMode::Overlay).unwrap_err();
        assert!(matches!(err, AppError::RestorePending), "{err}");
        with_restorer(&t.core, |r| r.discard()).unwrap();
        restore(&t, &id, &everything(), RestoreMode::Overlay).unwrap();
    }

    #[test]
    fn an_interrupted_recovery_still_rolls_back_to_the_original() {
        let t = setup();
        let id = snapshot(&t);
        mutate(&t);
        let mutated = tree(&t);
        interrupted(&t, &id);
        let first = journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .unwrap();
        assert_eq!(first.original_pre_restore, first.pre_restore_snapshot);

        // "Finish" is itself interrupted after one more change.
        let ok = recovery_confirmed(&t, &first, Recovery::Finish);
        let err = with_restorer(&t.core, |r| {
            r.recover(&first, Recovery::Finish, &ok, &mut |done, _| {
                if done == 1 {
                    Err(AppError::Io("crashed again".into()))
                } else {
                    Ok(())
                }
            })
        })
        .unwrap_err();
        assert!(err.to_string().contains("crashed again"));
        let second = journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .unwrap();
        assert_eq!(second.original_pre_restore, first.original_pre_restore);
        assert_eq!(second.source_snapshot, first.source_snapshot);
        assert_ne!(second.pre_restore_snapshot, first.pre_restore_snapshot);

        // Rolling back now still returns to the state before the user's restore.
        recover(&t, &second, Recovery::RollBack).unwrap();
        assert_eq!(tree(&t), mutated);
        assert!(journal::read(&t.core.paths.local_data_dir)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_snapshot_of_another_flavor_is_refused() {
        let t = setup();
        let id = snapshot(&t);
        let root = t.flavor.parent().unwrap().to_path_buf();
        crate::install::set(&t.core.settings, &root, Some("_classic_era_")).unwrap();
        let err = restore(&t, &id, &everything(), RestoreMode::Overlay).unwrap_err();
        assert!(matches!(err, AppError::InvalidInstall(_)), "{err}");
    }

    #[test]
    fn scope_items_round_trip_through_json() {
        let s = sel(vec![
            ScopeItem::Everything,
            ScopeItem::AddonData {
                addon: "Details".into(),
                target: AddonTarget::Everywhere,
            },
            ScopeItem::Paths {
                paths: vec![RelPath::new("WTF/Config.wtf").unwrap()],
            },
        ]);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(
            json["items"][0],
            serde_json::json!({ "kind": "Everything" })
        );
        assert_eq!(json["items"][1]["target"]["kind"], "Everywhere");
        assert_eq!(serde_json::from_value::<RestoreSelection>(json).unwrap(), s);
        // RelPath validation applies to restore paths from the UI too.
        let escape = serde_json::json!({ "items": [{ "kind": "Paths", "paths": ["../x"] }] });
        assert!(serde_json::from_value::<RestoreSelection>(escape).is_err());
    }
}
