//! Knowing whether WoW is running (spec §2).
//!
//! Two questions, two answers:
//! - **Display** ("WoW is running" in the UI): a process that is *our*
//!   install's game. Exe paths are canonicalized before comparing with the
//!   install root, so a game launched through a junction, `subst` drive or
//!   other alias still matches.
//! - **Write gate** (`is_running_now`): fails closed. Anything the display
//!   rule matches, plus any process with a known WoW exe name wherever it
//!   lives. A second WoW install blocks our writes while it runs, which is
//!   the safe direction.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Game executables we treat as WoW. Forever's beta ships `WowB.exe` /
/// `WowB-arm64.exe`. Users can add more in settings (`process_names_extra`).
pub const KNOWN_EXES: &[&str] = &[
    "Wow.exe",
    "Wow-64.exe",
    "Wow-arm64.exe",
    "WowT.exe",
    "WowB.exe",
    "WowB-arm64.exe",
    "WowClassic.exe",
    "WowClassic-arm64.exe",
    "WowClassicT.exe",
    "WowClassicB.exe",
    "World of Warcraft",
];

pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// What we need to know to recognize "our" game.
#[derive(Debug, Clone, Default)]
pub struct ProbeTarget {
    /// The WoW root. A process whose exe lives anywhere under it counts.
    pub root: Option<PathBuf>,
    /// Extra names from settings, on top of `KNOWN_EXES`.
    pub extra_names: Vec<String>,
}

/// One running process, as the OS reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcInfo {
    pub pid: u32,
    /// File name, e.g. "WowB.exe".
    pub name: String,
    /// Full exe path, if readable (access denied gives `None`).
    pub exe: Option<PathBuf>,
}

/// A seam so tests can describe running processes without real ones.
pub trait ProcessProbe: Send + Sync {
    fn processes(&self) -> Vec<ProcInfo>;
}

/// Real process list via sysinfo (names and exe paths only, which is cheap).
pub struct SysinfoProbe {
    system: Mutex<sysinfo::System>,
}

impl SysinfoProbe {
    pub fn new() -> Self {
        Self {
            system: Mutex::new(sysinfo::System::new()),
        }
    }
}

impl ProcessProbe for SysinfoProbe {
    fn processes(&self) -> Vec<ProcInfo> {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};

        let mut system = self.system.lock().expect("probe lock poisoned");
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .without_tasks(),
        );
        system
            .processes()
            .iter()
            .map(|(pid, p)| ProcInfo {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                exe: p.exe().map(Path::to_path_buf),
            })
            .collect()
    }
}

/// A `ProbeTarget` prepared for matching: the root canonicalized once.
struct Matcher<'a> {
    root: Option<PathBuf>,
    extra_names: &'a [String],
}

impl<'a> Matcher<'a> {
    fn new(target: &'a ProbeTarget) -> Self {
        Self {
            root: target
                .root
                .as_ref()
                .map(|r| dunce::canonicalize(r).unwrap_or_else(|_| r.clone())),
            extra_names: &target.extra_names,
        }
    }

    fn known_name(&self, name: &str) -> bool {
        KNOWN_EXES.iter().any(|n| n.eq_ignore_ascii_case(name))
            || self
                .extra_names
                .iter()
                .any(|n| n.eq_ignore_ascii_case(name))
    }

    /// Our install's game: exe under the root (canonicalized, so aliases
    /// match). With no readable path or no install set, go by name.
    fn is_ours(&self, p: &ProcInfo) -> bool {
        let Some(root) = &self.root else {
            return self.known_name(&p.name);
        };
        match &p.exe {
            Some(exe) if path_starts_with(exe, root) => true,
            // Only candidates are canonicalized, to keep each poll cheap.
            Some(exe) if self.known_name(&p.name) => {
                dunce::canonicalize(exe).is_ok_and(|canon| path_starts_with(&canon, root))
            }
            Some(_) => false,
            None => self.known_name(&p.name),
        }
    }

    /// The write gate's rule: ours, or any known WoW exe anywhere.
    fn blocks_writes(&self, p: &ProcInfo) -> bool {
        self.is_ours(p) || self.known_name(&p.name)
    }
}

fn path_starts_with(path: &Path, prefix: &Path) -> bool {
    let mut path = path.components();
    prefix.components().all(|p| {
        path.next().is_some_and(|c| {
            c.as_os_str().to_string_lossy().to_lowercase()
                == p.as_os_str().to_string_lossy().to_lowercase()
        })
    })
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct GameStatus {
    pub running: bool,
    pub pids: Vec<u32>,
    /// When the game was first seen running (RFC 3339, UTC), for "session 1h 42m".
    pub since: Option<String>,
}

/// Polls the probe, keeps the latest status, and reports transitions.
pub struct GameWatcher {
    probe: Arc<dyn ProcessProbe>,
    status: RwLock<GameStatus>,
}

/// What changed in one poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Started,
    Stopped,
}

impl GameWatcher {
    pub fn new(probe: Arc<dyn ProcessProbe>) -> Self {
        Self {
            probe,
            status: RwLock::new(GameStatus::default()),
        }
    }

    pub fn status(&self) -> GameStatus {
        self.status.read().expect("status lock poisoned").clone()
    }

    /// For the write gate: asks the OS right now (never the cached status)
    /// and fails closed, counting any known WoW exe wherever it runs from.
    #[allow(dead_code)] // the write gate's only production caller arrives with T7
    pub fn is_running_now(&self, target: &ProbeTarget) -> bool {
        let matcher = Matcher::new(target);
        self.probe
            .processes()
            .iter()
            .any(|p| matcher.blocks_writes(p))
    }

    /// One poll for the displayed status: updates it and returns a
    /// transition, if any.
    pub fn poll(&self, target: &ProbeTarget) -> Option<Transition> {
        let matcher = Matcher::new(target);
        let mut pids: Vec<u32> = self
            .probe
            .processes()
            .iter()
            .filter(|p| matcher.is_ours(p))
            .map(|p| p.pid)
            .collect();
        pids.sort_unstable();

        let mut status = self.status.write().expect("status lock poisoned");
        let was_running = status.running;
        let running = !pids.is_empty();
        status.since = match (was_running, running) {
            (false, true) => Some(chrono::Utc::now().to_rfc3339()),
            (true, true) => status.since.take(),
            _ => None,
        };
        status.running = running;
        status.pids = pids;
        match (was_running, running) {
            (false, true) => Some(Transition::Started),
            (true, false) => Some(Transition::Stopped),
            _ => None,
        }
    }

    /// Polls every `POLL_INTERVAL` on a background thread until the process
    /// exits. `target` is re-read each time, so a changed install or extra
    /// names apply on the next poll. `on_change` runs after each transition.
    pub fn spawn(
        self: &Arc<Self>,
        target: impl Fn() -> ProbeTarget + Send + 'static,
        on_change: impl Fn(Transition, GameStatus) + Send + 'static,
    ) {
        let watcher = Arc::clone(self);
        std::thread::Builder::new()
            .name("game-watcher".into())
            .spawn(move || loop {
                if let Some(t) = watcher.poll(&target()) {
                    on_change(t, watcher.status());
                }
                std::thread::sleep(POLL_INTERVAL);
            })
            .expect("spawn game watcher thread");
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;

    /// Test probe: reports whatever processes the test set.
    #[derive(Default)]
    pub struct FakeProbe {
        processes: Mutex<Vec<ProcInfo>>,
    }

    impl FakeProbe {
        /// Shorthand: a WoW process with an unreadable path (matches by name).
        pub fn set_running(&self, running: bool) {
            let list = if running {
                vec![ProcInfo {
                    pid: 4242,
                    name: "WowB.exe".into(),
                    exe: None,
                }]
            } else {
                Vec::new()
            };
            self.set_processes(list);
        }

        pub fn set_processes(&self, list: Vec<ProcInfo>) {
            *self.processes.lock().unwrap() = list;
        }
    }

    impl ProcessProbe for FakeProbe {
        fn processes(&self) -> Vec<ProcInfo> {
            self.processes.lock().unwrap().clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeProbe;
    use super::*;

    fn target(root: &Path) -> ProbeTarget {
        ProbeTarget {
            root: Some(root.to_path_buf()),
            extra_names: vec!["MyWow.exe".into()],
        }
    }

    fn proc(pid: u32, name: &str, exe: Option<PathBuf>) -> ProcInfo {
        ProcInfo {
            pid,
            name: name.into(),
            exe,
        }
    }

    fn watcher_with(list: Vec<ProcInfo>) -> GameWatcher {
        let probe = Arc::new(FakeProbe::default());
        probe.set_processes(list);
        GameWatcher::new(probe)
    }

    #[test]
    fn matches_exe_under_root_case_insensitively() {
        let t = target(Path::new("/Games/World of Warcraft"));
        let m = Matcher::new(&t);
        let exe = PathBuf::from("/games/world of warcraft/_classic_beta_/WowB.exe");
        assert!(m.is_ours(&proc(1, "WowB.exe", Some(exe))));
        // Anything under the root counts; erring towards "running" is safe.
        let tool = PathBuf::from("/Games/World of Warcraft/Utils/Repair.exe");
        assert!(m.is_ours(&proc(2, "Repair.exe", Some(tool))));
        let other = PathBuf::from("/usr/bin/editor");
        assert!(!m.is_ours(&proc(3, "editor", Some(other.clone()))));
        assert!(!m.blocks_writes(&proc(3, "editor", Some(other))));
    }

    #[test]
    fn falls_back_to_names_without_a_path_or_install() {
        let t = target(Path::new("/Games/World of Warcraft"));
        let m = Matcher::new(&t);
        assert!(m.is_ours(&proc(1, "WowB.exe", None)));
        assert!(m.is_ours(&proc(2, "wowb-arm64.EXE", None)));
        assert!(
            m.is_ours(&proc(3, "MyWow.exe", None)),
            "extra name from settings"
        );
        assert!(!m.is_ours(&proc(4, "explorer.exe", None)));

        let none = ProbeTarget::default();
        let m = Matcher::new(&none);
        assert!(m.is_ours(&proc(
            5,
            "Wow.exe",
            Some(PathBuf::from("/anywhere/Wow.exe"))
        )));
    }

    /// Review must-fix (1): a known WoW exe outside our root isn't shown as
    /// our game, but it does block writes.
    #[test]
    fn gate_fails_closed_on_known_exe_anywhere() {
        let elsewhere = PathBuf::from("/other/World of Warcraft/_retail_/Wow.exe");
        let w = watcher_with(vec![proc(7, "Wow.exe", Some(elsewhere))]);
        let t = target(Path::new("/Games/World of Warcraft"));

        assert_eq!(w.poll(&t), None, "not our install: no 'WoW is running'");
        assert!(w.is_running_now(&t), "but writes are blocked");
    }

    /// Review must-fix (2): WoW launched through an alias of the install
    /// folder (junction/symlink/subst) matches after canonicalization, for
    /// both the root and the exe.
    #[test]
    fn exe_launched_through_an_alias_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real").join("World of Warcraft");
        std::fs::create_dir_all(real.join("_classic_beta_")).unwrap();
        std::fs::write(real.join("_classic_beta_").join("WowB.exe"), b"stub").unwrap();
        let alias = tmp.path().join("alias");
        crate::test_support::link_dir(&real, &alias);

        let via_alias = alias.join("_classic_beta_").join("WowB.exe");
        let via_real = real.join("_classic_beta_").join("WowB.exe");

        // Game launched through the alias, install saved as the real path.
        let w = watcher_with(vec![proc(9, "WowB.exe", Some(via_alias))]);
        assert_eq!(w.poll(&target(&real)), Some(Transition::Started));

        // Game launched from the real path, install saved through the alias.
        let w = watcher_with(vec![proc(9, "WowB.exe", Some(via_real))]);
        assert_eq!(w.poll(&target(&alias)), Some(Transition::Started));
    }

    #[test]
    fn poll_reports_transitions_and_session_start() {
        let probe = Arc::new(FakeProbe::default());
        let watcher = GameWatcher::new(probe.clone());
        let t = ProbeTarget::default();

        assert_eq!(watcher.poll(&t), None);
        assert!(!watcher.status().running);

        probe.set_running(true);
        assert_eq!(watcher.poll(&t), Some(Transition::Started));
        let since = watcher.status().since.expect("session start recorded");
        assert_eq!(watcher.status().pids, vec![4242]);

        assert_eq!(watcher.poll(&t), None);
        assert_eq!(watcher.status().since.as_deref(), Some(since.as_str()));

        probe.set_running(false);
        assert_eq!(watcher.poll(&t), Some(Transition::Stopped));
        assert_eq!(watcher.status(), GameStatus::default());
    }

    #[test]
    fn is_running_now_ignores_the_cache() {
        let probe = Arc::new(FakeProbe::default());
        let watcher = GameWatcher::new(probe.clone());
        watcher.poll(&ProbeTarget::default());
        probe.set_running(true);
        assert!(!watcher.status().running, "cache is stale");
        assert!(watcher.is_running_now(&ProbeTarget::default()));
    }

    #[test]
    fn real_probe_lists_processes() {
        // Smoke test against this machine; WoW isn't running in CI.
        let probe = SysinfoProbe::new();
        let list = probe.processes();
        assert!(!list.is_empty(), "at least this test process");
        let w = GameWatcher::new(Arc::new(probe));
        assert!(!w.is_running_now(&target(Path::new("/definitely/not/a/wow/root"))));
    }
}
