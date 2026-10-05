//! The only way to change a game file (spec §4).
//!
//! `WriteGate::begin` refuses while WoW is running (asked synchronously, not
//! from the cached status), resolves every path the operation will touch,
//! snapshots those files, and records the operation in `write_audit`. Only
//! then does it hand out a `MutationGuard`, and the guard's `write`/`remove`
//! are the only functions that touch game files. They accept only the paths
//! named up front and re-check that WoW isn't running before each change.

use std::path::PathBuf;
use std::sync::Arc;

use crate::bridge::{self, Slot};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::fsx::atomic::atomic_replace;
use crate::fsx::relpath::{GameRoot, RelPath};
use crate::game::process::{GameWatcher, ProbeTarget};

/// Takes the pre-write snapshot. Implemented by the backup store (T7); a
/// trait so the gate can be tested on its own.
pub trait PreWriteSnapshot: Send + Sync {
    /// Snapshots `paths` under `game` as they are now, recording paths that
    /// don't exist yet as absent so undo can remove them. Returns the
    /// snapshot id. If this fails, the write must not happen. `op` is the
    /// operation's name ("restore" makes it a pre-restore snapshot).
    fn snapshot_before_write(
        &self,
        game: &GameRoot,
        op: &str,
        paths: &[RelPath],
        label: &str,
    ) -> AppResult<String>;
}

/// What a mutation writes into: the game folder, and how to recognize a
/// running game for it.
#[derive(Debug, Clone)]
pub struct MutationTarget {
    pub game: GameRoot,
    pub probe: ProbeTarget,
}

pub struct WriteGate {
    watcher: Arc<GameWatcher>,
    snapshots: Arc<dyn PreWriteSnapshot>,
    db: Db,
}

impl WriteGate {
    pub fn new(watcher: Arc<GameWatcher>, snapshots: Arc<dyn PreWriteSnapshot>, db: Db) -> Self {
        Self {
            watcher,
            snapshots,
            db,
        }
    }

    /// Starts a mutation named `op` (e.g. "restore") that will touch exactly
    /// `paths`. `label` describes it for the safety snapshot, e.g. "Before
    /// restoring Velyra".
    pub fn begin(
        &self,
        op: &str,
        target: &MutationTarget,
        paths: &[RelPath],
        label: &str,
    ) -> AppResult<MutationGuard<'_>> {
        if let Some(blocker) = self.watcher.blocking_now(&target.probe) {
            return Err(AppError::GameRunning(blocker.to_string()));
        }
        let allowed = paths
            .iter()
            .map(|p| Ok((p.clone(), p.resolve(&target.game)?)))
            .collect::<AppResult<Vec<(RelPath, PathBuf)>>>()?;

        let snapshot_id = self
            .snapshots
            .snapshot_before_write(&target.game, op, paths, label)?;
        let audit_id = self.audit_start(op, paths, &snapshot_id)?;

        Ok(MutationGuard {
            gate: self,
            target: target.clone(),
            allowed,
            snapshot_id,
            audit_id,
            committed: false,
        })
    }

    /// Writes our addon's data slots (bridge spec §3): no snapshot, since
    /// they're the app's own generated data. Every file is checked first
    /// (`bridge::check`: data only, size cap) and every path resolved
    /// (a linked `ForeverBuddy` or `Data` folder is refused), so one bad
    /// slot leaves the whole set as it was. Then each is replaced
    /// atomically. Takes `Slot`s from the constant list, never a path.
    ///
    /// Refused while WoW runs, like every other write, until probe run 4
    /// shows `/reload` re-reads a changed slot (spec §8, B4).
    pub fn write_slots(&self, target: &MutationTarget, slots: &[(Slot, Vec<u8>)]) -> AppResult<()> {
        let mut resolved = Vec::with_capacity(slots.len());
        for (slot, bytes) in slots {
            bridge::check(*slot, bytes)?;
            resolved.push((slot.path(), slot.path().resolve(&target.game)?, bytes));
        }
        if let Some(blocker) = self.watcher.blocking_now(&target.probe) {
            return Err(AppError::GameRunning(blocker.to_string()));
        }
        let paths: Vec<RelPath> = resolved.iter().map(|(p, _, _)| p.clone()).collect();
        let audit_id = self.audit_start("bridge_slots", &paths, "")?;
        for (_, abs, bytes) in &resolved {
            if let Err(e) = atomic_replace(abs, bytes) {
                self.audit_finish(audit_id, "failed")?;
                return Err(e);
            }
        }
        self.audit_finish(audit_id, "committed")
    }

    fn audit_start(&self, op: &str, paths: &[RelPath], snapshot_id: &str) -> AppResult<i64> {
        let paths: Vec<String> = paths.iter().map(RelPath::as_string).collect();
        let paths = serde_json::to_string(&paths).expect("strings serialize");
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO write_audit (at, op, paths, snapshot_id, result)
                 VALUES (?1, ?2, ?3, ?4, 'started')",
                (chrono::Utc::now().to_rfc3339(), op, paths, snapshot_id),
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    fn audit_finish(&self, audit_id: i64, result: &str) -> AppResult<()> {
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE write_audit SET result = ?1 WHERE id = ?2",
                (result, audit_id),
            )?;
            Ok(())
        })
    }
}

/// Proof that a mutation was started safely. Dropped without `commit`, the
/// audit row is marked `aborted`; the safety snapshot is there to undo it.
pub struct MutationGuard<'g> {
    gate: &'g WriteGate,
    target: MutationTarget,
    allowed: Vec<(RelPath, PathBuf)>,
    snapshot_id: String,
    audit_id: i64,
    committed: bool,
}

impl MutationGuard<'_> {
    /// The safety snapshot taken before any change.
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    /// Atomically replaces `path` with `bytes`.
    pub fn write(&self, path: &RelPath, bytes: &[u8]) -> AppResult<()> {
        let target = self.check(path)?;
        atomic_replace(&target, bytes)
    }

    /// Deletes `path`. Already gone counts as success.
    pub fn remove(&self, path: &RelPath) -> AppResult<()> {
        let target = self.check(path)?;
        match std::fs::remove_file(&target) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Deletes the folder `dir` if it's empty, and only if it holds one of
    /// this mutation's paths. Never recursive, and checked like any path, so
    /// a folder that's a link to somewhere else is refused rather than
    /// followed. Returns false if something else is still in it.
    pub fn remove_empty_dir(&self, dir: &RelPath) -> AppResult<bool> {
        if !self
            .allowed
            .iter()
            .any(|(p, _)| p.parent().as_ref() == Some(dir))
        {
            return Err(AppError::PathEscape(format!(
                "{dir} is not part of this change"
            )));
        }
        if let Some(blocker) = self.gate.watcher.blocking_now(&self.target.probe) {
            return Err(AppError::GameRunning(blocker.to_string()));
        }
        let target = dir.resolve(&self.target.game)?;
        match std::fs::remove_dir(&target) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(_) if std::fs::read_dir(&target).is_ok_and(|mut d| d.next().is_some()) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn commit(mut self) -> AppResult<()> {
        self.committed = true;
        self.gate.audit_finish(self.audit_id, "committed")
    }

    /// Every change: the path must be one this mutation declared, WoW must
    /// still not be running, and the path must still resolve safely (a link
    /// re-pointed since `begin` is caught here).
    fn check(&self, path: &RelPath) -> AppResult<PathBuf> {
        if !self.allowed.iter().any(|(p, _)| p == path) {
            return Err(AppError::PathEscape(format!(
                "{path} is not part of this change"
            )));
        }
        if let Some(blocker) = self.gate.watcher.blocking_now(&self.target.probe) {
            return Err(AppError::GameRunning(blocker.to_string()));
        }
        path.resolve(&self.target.game)
    }
}

impl Drop for MutationGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.gate.audit_finish(self.audit_id, "aborted");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::process::fake::FakeProbe;
    use crate::test_support::fixture_copy;
    use std::sync::Mutex;

    /// Records snapshot requests; can be told to fail.
    #[derive(Default)]
    struct FakeSnapshots {
        taken: Mutex<Vec<(Vec<String>, String)>>,
        fail: bool,
    }

    impl PreWriteSnapshot for FakeSnapshots {
        fn snapshot_before_write(
            &self,
            _game: &GameRoot,
            _op: &str,
            paths: &[RelPath],
            label: &str,
        ) -> AppResult<String> {
            if self.fail {
                return Err(AppError::Io("disk full".into()));
            }
            let mut taken = self.taken.lock().unwrap();
            taken.push((paths.iter().map(RelPath::as_string).collect(), label.into()));
            Ok(format!("snap-{}", taken.len()))
        }
    }

    struct Setup {
        _dir: tempfile::TempDir,
        flavor: PathBuf,
        probe: Arc<FakeProbe>,
        snapshots: Arc<FakeSnapshots>,
        gate: WriteGate,
        target: MutationTarget,
        db: Db,
    }

    fn setup_with(snapshots: FakeSnapshots) -> Setup {
        let (dir, root) = fixture_copy();
        let flavor = root.join("_classic_beta_");
        let probe = Arc::new(FakeProbe::default());
        let snapshots = Arc::new(snapshots);
        let db = Db::open_in_memory().unwrap();
        let gate = WriteGate::new(
            Arc::new(GameWatcher::new(probe.clone())),
            snapshots.clone(),
            db.clone(),
        );
        let target = MutationTarget {
            game: GameRoot::new(&flavor).unwrap(),
            probe: ProbeTarget::default(),
        };
        Setup {
            _dir: dir,
            flavor,
            probe,
            snapshots,
            gate,
            target,
            db,
        }
    }

    fn setup() -> Setup {
        setup_with(FakeSnapshots::default())
    }

    fn rel(s: &str) -> RelPath {
        RelPath::new(s).unwrap()
    }

    fn audit_rows(db: &Db) -> Vec<(String, String, Option<String>, String)> {
        db.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT op, paths, snapshot_id, result FROM write_audit ORDER BY id")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
    }

    const MACROS: &str = "WTF/Account/ACCOUNT1/macros-cache.txt";

    #[test]
    fn refuses_while_wow_runs_and_touches_nothing() {
        let s = setup();
        s.probe.set_running(true);
        let before = std::fs::read(s.flavor.join(MACROS)).unwrap();

        let result = s
            .gate
            .begin("macro_edit", &s.target, &[rel(MACROS)], "Before macro edit");
        assert!(matches!(result, Err(AppError::GameRunning(_))));
        assert!(
            s.snapshots.taken.lock().unwrap().is_empty(),
            "no snapshot taken"
        );
        assert!(audit_rows(&s.db).is_empty(), "nothing recorded");
        assert_eq!(std::fs::read(s.flavor.join(MACROS)).unwrap(), before);
    }

    #[test]
    fn snapshots_then_writes_then_commits() {
        let s = setup();
        let new_file = "WTF/Account/ACCOUNT1/SavedVariables/New.lua";
        let guard = s
            .gate
            .begin(
                "macro_edit",
                &s.target,
                &[rel(MACROS), rel(new_file)],
                "Before macro edit",
            )
            .unwrap();
        assert_eq!(guard.snapshot_id(), "snap-1");
        assert_eq!(
            s.snapshots.taken.lock().unwrap()[0],
            (
                vec![MACROS.to_string(), new_file.to_string()],
                "Before macro edit".to_string()
            )
        );

        guard
            .write(&rel(MACROS), b"MACRO 1 \"New\"\nEND\n")
            .unwrap();
        guard.write(&rel(new_file), b"NewDB = {}\n").unwrap();
        guard.commit().unwrap();

        assert_eq!(
            std::fs::read(s.flavor.join(MACROS)).unwrap(),
            b"MACRO 1 \"New\"\nEND\n"
        );
        assert_eq!(
            std::fs::read(s.flavor.join(new_file)).unwrap(),
            b"NewDB = {}\n"
        );
        let rows = audit_rows(&s.db);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "macro_edit");
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&rows[0].1).unwrap(),
            [MACROS, new_file]
        );
        assert_eq!(rows[0].2.as_deref(), Some("snap-1"));
        assert_eq!(rows[0].3, "committed");
    }

    #[test]
    fn failed_snapshot_means_no_write() {
        let s = setup_with(FakeSnapshots {
            fail: true,
            ..Default::default()
        });
        assert!(s
            .gate
            .begin("macro_edit", &s.target, &[rel(MACROS)], "x")
            .is_err());
        assert!(audit_rows(&s.db).is_empty());
    }

    #[test]
    fn only_declared_paths_can_be_changed() {
        let s = setup();
        let guard = s
            .gate
            .begin("macro_edit", &s.target, &[rel(MACROS)], "x")
            .unwrap();
        let other = rel("WTF/Config.wtf");
        assert!(matches!(
            guard.write(&other, b"nope"),
            Err(AppError::PathEscape(_))
        ));
        assert!(matches!(guard.remove(&other), Err(AppError::PathEscape(_))));
        assert!(s.flavor.join("WTF/Config.wtf").exists());
    }

    #[test]
    fn escaping_paths_are_refused_before_any_snapshot() {
        let s = setup();
        let outside = s.flavor.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        crate::test_support::link_dir(&outside, &s.flavor.join("WTF/Account/link"));

        let result = s.gate.begin(
            "macro_edit",
            &s.target,
            &[rel("WTF/Account/link/evil.txt")],
            "x",
        );
        assert!(matches!(result, Err(AppError::PathEscape(_))));
        assert!(s.snapshots.taken.lock().unwrap().is_empty());
    }

    #[test]
    fn wow_starting_mid_change_stops_further_writes() {
        let s = setup();
        let config = "WTF/Config.wtf";
        let guard = s
            .gate
            .begin("restore", &s.target, &[rel(MACROS), rel(config)], "x")
            .unwrap();
        guard.write(&rel(MACROS), b"first").unwrap();

        s.probe.set_running(true);
        assert!(matches!(
            guard.write(&rel(config), b"second"),
            Err(AppError::GameRunning(_))
        ));
        assert!(matches!(
            guard.remove(&rel(config)),
            Err(AppError::GameRunning(_))
        ));
        drop(guard);

        assert_ne!(std::fs::read(s.flavor.join(config)).unwrap(), b"second");
        assert_eq!(audit_rows(&s.db)[0].3, "aborted");
    }

    #[test]
    fn remove_deletes_and_tolerates_missing() {
        let s = setup();
        let bak = "WTF/Account/ACCOUNT1/SavedVariables/Details.lua.bak";
        let gone = "WTF/Account/ACCOUNT1/SavedVariables/Gone.lua";
        let guard = s
            .gate
            .begin("restore", &s.target, &[rel(bak), rel(gone)], "x")
            .unwrap();
        guard.remove(&rel(bak)).unwrap();
        guard.remove(&rel(gone)).unwrap();
        guard.commit().unwrap();
        assert!(!s.flavor.join(bak).exists());
    }
}
