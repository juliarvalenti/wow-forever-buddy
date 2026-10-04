//! Knowing whether WoW is running (spec §2).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Game executables we treat as WoW when a process's path can't be read.
/// Forever's beta ships `WowB.exe` / `WowB-arm64.exe`. Users can add more in
/// settings (`process_names_extra`).
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

/// What a probe needs to know to recognize "our" game.
#[derive(Debug, Clone, Default)]
pub struct ProbeTarget {
    /// The WoW root. A process whose exe lives anywhere under it counts.
    pub root: Option<PathBuf>,
    /// Extra names from settings, on top of `KNOWN_EXES`.
    pub extra_names: Vec<String>,
}

/// A seam so tests can say "WoW is running" without a real process.
pub trait ProcessProbe: Send + Sync {
    /// PIDs of running WoW processes, sorted.
    fn wow_pids(&self, target: &ProbeTarget) -> Vec<u32>;
}

/// Real process probe. A process counts as WoW if its exe path is under the
/// install root (case-insensitive), or, when the path can't be read (access
/// denied), if its file name is a known WoW exe.
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
    fn wow_pids(&self, target: &ProbeTarget) -> Vec<u32> {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};

        let mut system = self.system.lock().expect("probe lock poisoned");
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .without_tasks(),
        );
        let mut pids: Vec<u32> = system
            .processes()
            .iter()
            .filter(|(_, p)| is_wow(p.exe(), &p.name().to_string_lossy(), target))
            .map(|(pid, _)| pid.as_u32())
            .collect();
        pids.sort_unstable();
        pids
    }
}

/// The match rule, separate from sysinfo so it can be tested directly.
fn is_wow(exe: Option<&Path>, name: &str, target: &ProbeTarget) -> bool {
    match (exe, &target.root) {
        (Some(exe), Some(root)) if path_starts_with(exe, root) => true,
        // A readable path outside the root is someone else's WoW (or not WoW).
        (Some(_), Some(_)) => false,
        // No path (access denied) or no install configured: go by name.
        _ => {
            KNOWN_EXES.iter().any(|n| n.eq_ignore_ascii_case(name))
                || target
                    .extra_names
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(name))
        }
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

    /// Asks the probe right now, bypassing the cached status. The write gate
    /// uses this so a write is never allowed on a stale "not running".
    #[allow(dead_code)] // the write gate's only production caller arrives with T7
    pub fn is_running_now(&self, target: &ProbeTarget) -> bool {
        !self.probe.wow_pids(target).is_empty()
    }

    /// One poll: updates the cached status and returns a transition, if any.
    pub fn poll(&self, target: &ProbeTarget) -> Option<Transition> {
        let pids = self.probe.wow_pids(target);
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
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Test probe: "running" is whatever the test last set.
    #[derive(Default)]
    pub struct FakeProbe {
        running: AtomicBool,
    }

    impl FakeProbe {
        pub fn set_running(&self, running: bool) {
            self.running.store(running, Ordering::SeqCst);
        }
    }

    impl ProcessProbe for FakeProbe {
        fn wow_pids(&self, _target: &ProbeTarget) -> Vec<u32> {
            if self.running.load(Ordering::SeqCst) {
                vec![4242]
            } else {
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeProbe;
    use super::*;

    fn target(root: &str) -> ProbeTarget {
        ProbeTarget {
            root: Some(PathBuf::from(root)),
            extra_names: vec!["MyWow.exe".into()],
        }
    }

    #[test]
    fn matches_exe_under_root_case_insensitively() {
        let t = target("/Games/World of Warcraft");
        let exe = Path::new("/games/world of warcraft/_classic_beta_/WowB.exe");
        assert!(is_wow(Some(exe), "WowB.exe", &t));
        // A known name but somewhere else entirely: another install, not ours.
        let elsewhere = Path::new("/other/World of Warcraft/_retail_/Wow.exe");
        assert!(!is_wow(Some(elsewhere), "Wow.exe", &t));
        // Something else that happens to live under the root still counts:
        // the root is WoW's, and erring towards "running" is the safe side.
        let tool = Path::new("/Games/World of Warcraft/Utils/Repair.exe");
        assert!(is_wow(Some(tool), "Repair.exe", &t));
    }

    #[test]
    fn falls_back_to_names_without_a_path() {
        let t = target("/Games/World of Warcraft");
        assert!(is_wow(None, "WowB.exe", &t));
        assert!(is_wow(None, "wowb-arm64.EXE", &t));
        assert!(is_wow(None, "MyWow.exe", &t), "extra name from settings");
        assert!(!is_wow(None, "explorer.exe", &t));

        let no_install = ProbeTarget::default();
        assert!(is_wow(
            Some(Path::new("/anywhere/Wow.exe")),
            "Wow.exe",
            &no_install
        ));
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
    fn real_probe_runs() {
        // Smoke test against this machine's process list; WoW isn't running in CI.
        let probe = SysinfoProbe::new();
        let pids = probe.wow_pids(&target("/definitely/not/a/wow/root"));
        assert!(pids.is_empty());
    }
}
