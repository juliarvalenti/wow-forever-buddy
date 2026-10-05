//! Knowing whether WoW is running (spec §2).
//!
//! Two questions, two answers:
//! - **Display** ("WoW is running" in the UI): a process that is *our*
//!   install's game. Exe paths are canonicalized before comparing with the
//!   install root, so a game launched through a junction, `subst` drive or
//!   other alias still matches.
//! - **Write gate** (`blocking_now`): fails closed. Anything the display
//!   rule matches, plus any process with a WoW client name (the flavor
//!   table, Blizzard's naming pattern, or a name from settings) wherever it
//!   lives.
//!   A second WoW install blocks our writes while it runs, which is the safe
//!   direction. A process list that doesn't even include this app means
//!   enumeration failed: that's "unknown", and it blocks writes too.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::install::layout::known_exe_names;

/// The macOS client's process name (no `.exe`).
const MAC_APP_NAME: &str = "World of Warcraft";

pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// What we need to know to recognize "our" game.
#[derive(Debug, Clone, Default)]
pub struct ProbeTarget {
    /// The WoW root. A process whose exe lives anywhere under it counts.
    pub root: Option<PathBuf>,
    /// Extra names from settings, on top of the built-in ones.
    pub extra_names: Vec<String>,
}

/// Whether `name` is a known WoW game exe: every name in the flavor table
/// (`KNOWN_FLAVORS`, arm64 builds included) and the macOS app. This is what
/// the displayed "WoW is running" goes by.
pub fn is_wow_exe(name: &str) -> bool {
    known_exe_names()
        .iter()
        .any(|n| n.eq_ignore_ascii_case(name))
        || name.eq_ignore_ascii_case(MAC_APP_NAME)
}

/// Blizzard's client naming pattern, `^wow(classic)?[a-z]?(-64|-arm64)?\.exe$`
/// (case-insensitive): `Wow.exe`, `WowB-arm64.exe`, `WowClassicT.exe`, …,
/// so a flavor the table doesn't know yet still counts. Tools that merely
/// start with "wow" (WowUp, WowUp-CF, this app's `wow-forever-buddy.exe`)
/// don't fit it, so no denylist is needed; a renamed build goes in settings
/// (`process_names_extra`). Only the write gate uses this, so an unknown
/// name blocks writes (safe) without being shown as "WoW is running" or
/// starting a game-exit backup when it closes.
fn looks_like_wow(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(rest) = lower
        .strip_suffix(".exe")
        .and_then(|s| s.strip_prefix("wow"))
    else {
        return false;
    };
    let rest = rest.strip_prefix("classic").unwrap_or(rest);
    let rest = rest
        .strip_suffix("-arm64")
        .or_else(|| rest.strip_suffix("-64"))
        .unwrap_or(rest);
    rest.is_empty() || (rest.len() == 1 && rest.as_bytes()[0].is_ascii_lowercase())
}

/// The write gate's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameCheck {
    NotRunning,
    Running,
    /// The process list can't be trusted (it doesn't include this app), so
    /// nobody knows. Writes are blocked; automatic backups still run, flagged
    /// as taken while the game may be running.
    Unknown,
}

/// Why writes are blocked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    /// This process (exe name) counts as WoW.
    Process(String),
    /// The process list couldn't be read.
    Unknown,
}

impl std::fmt::Display for Blocker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Blocker::Process(name) => write!(f, "{name} is running"),
            Blocker::Unknown => f.write_str("can't tell which programs are running"),
        }
    }
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
        is_wow_exe(name)
            || self
                .extra_names
                .iter()
                .any(|n| n.eq_ignore_ascii_case(name))
    }

    /// Our install's game: exe under the root (canonicalized, so aliases
    /// match). With no readable path or no install set, go by name.
    fn is_ours(&self, p: &ProcInfo) -> bool {
        // Never this app, even if it's installed under the WoW folder.
        if p.pid == std::process::id() {
            return false;
        }
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

    /// The write gate's rule: ours, or any known or WoW-looking exe anywhere.
    fn blocks_writes(&self, p: &ProcInfo) -> bool {
        p.pid != std::process::id()
            && (self.is_ours(p) || self.known_name(&p.name) || looks_like_wow(&p.name))
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
    /// The last poll couldn't list processes, so `running` is stale. The UI
    /// says it can't tell; restores stay locked.
    pub unknown: bool,
}

/// A process list is trustworthy only if it includes this app.
fn includes_self(list: &[ProcInfo]) -> bool {
    let me = std::process::id();
    list.iter().any(|p| p.pid == me)
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
    /// Process listing stopped working (status is now `unknown`).
    Unknown,
    /// It works again, and the game's state is as before.
    Known,
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

    /// Asks the OS right now (never the cached status), counting any WoW
    /// exe wherever it runs from.
    pub fn check_now(&self, target: &ProbeTarget) -> GameCheck {
        match self.blocking_now(target) {
            None => GameCheck::NotRunning,
            Some(Blocker::Process(_)) => GameCheck::Running,
            Some(Blocker::Unknown) => GameCheck::Unknown,
        }
    }

    /// What blocks writes right now, if anything, so the error can name it
    /// ("WowB.exe is running") and a false positive is diagnosable.
    pub fn blocking_now(&self, target: &ProbeTarget) -> Option<Blocker> {
        let list = self.probe.processes();
        if !includes_self(&list) {
            return Some(Blocker::Unknown);
        }
        let matcher = Matcher::new(target);
        list.iter()
            .find(|p| matcher.blocks_writes(p))
            .map(|p| Blocker::Process(p.name.clone()))
    }

    /// Fails closed, so "unknown" counts as running. (The gate itself uses
    /// `blocking_now`, to name the blocker.)
    #[cfg(test)]
    pub fn is_running_now(&self, target: &ProbeTarget) -> bool {
        self.blocking_now(target).is_some()
    }

    /// One poll for the displayed status: updates it and returns a
    /// transition, if any. A failed enumeration marks the status unknown
    /// and changes nothing else, so it can't fake a "WoW closed" (which
    /// would start a game-exit backup).
    pub fn poll(&self, target: &ProbeTarget) -> Option<Transition> {
        let list = self.probe.processes();
        let mut status = self.status.write().expect("status lock poisoned");
        if !includes_self(&list) {
            let was_unknown = std::mem::replace(&mut status.unknown, true);
            return (!was_unknown).then_some(Transition::Unknown);
        }
        let was_unknown = std::mem::replace(&mut status.unknown, false);
        let matcher = Matcher::new(target);
        let mut pids: Vec<u32> = list
            .iter()
            .filter(|p| matcher.is_ours(p))
            .map(|p| p.pid)
            .collect();
        pids.sort_unstable();

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
            _ => was_unknown.then_some(Transition::Known),
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

    /// Test probe: reports whatever processes the test set, plus this app
    /// itself (as a real listing would), unless enumeration is set to fail.
    #[derive(Default)]
    pub struct FakeProbe {
        processes: Mutex<Vec<ProcInfo>>,
        blind: std::sync::atomic::AtomicBool,
    }

    impl FakeProbe {
        /// Simulates a failed enumeration: an empty list, without this app.
        pub fn set_blind(&self, blind: bool) {
            self.blind.store(blind, std::sync::atomic::Ordering::SeqCst);
        }

        /// The fake WoW's pid: never this test process's own, which the
        /// matcher treats as the app and never as the game. (A fixed pid
        /// failed every "WoW running" test the day the test binary drew it.)
        pub fn wow_pid() -> u32 {
            std::process::id().wrapping_add(1)
        }

        /// Shorthand: a WoW process with an unreadable path (matches by name).
        pub fn set_running(&self, running: bool) {
            let list = if running {
                vec![ProcInfo {
                    pid: Self::wow_pid(),
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
            if self.blind.load(std::sync::atomic::Ordering::SeqCst) {
                return Vec::new();
            }
            let mut list = self.processes.lock().unwrap().clone();
            // Named like the real app on Windows: a wow*.exe that must not
            // count as the game.
            list.push(ProcInfo {
                pid: std::process::id(),
                name: "wow-forever-buddy.exe".into(),
                exe: None,
            });
            list
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
        assert_eq!(watcher.status().pids, vec![FakeProbe::wow_pid()]);

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
        assert!(includes_self(&list), "a real listing includes this process");
        let w = GameWatcher::new(Arc::new(probe));
        let t = target(Path::new("/definitely/not/a/wow/root"));
        assert_eq!(w.check_now(&t), GameCheck::NotRunning);
    }

    /// R2: one exe list (the flavor table, arm64 included) for the display;
    /// the client naming pattern blocks writes only; addon managers and this
    /// app's own `wow-forever-buddy.exe` are never the game.
    #[test]
    fn wow_exe_names_come_from_the_flavor_table() {
        for name in known_exe_names() {
            assert!(is_wow_exe(name) && looks_like_wow(name), "{name}");
        }
        for name in [
            "WowT-arm64.exe",
            "WowClassicT-arm64.exe",
            "World of Warcraft",
        ] {
            assert!(is_wow_exe(name), "{name}");
        }
        // Flavors the table doesn't know yet still fit the pattern.
        for name in ["wowclassicb.EXE", "WowX-64.exe", "WowClassicZ-arm64.exe"] {
            assert!(
                !is_wow_exe(name) && looks_like_wow(name),
                "{name}: gate only"
            );
        }
        for name in [
            "wow-forever-buddy.exe",
            "WoW-Forever-Buddy.EXE",
            "wow-forever-buddy",
            "WowUp.exe",
            "WowUp-CF.exe",
            "WowMatrix.exe",
            "WowForever.exe", // a renamed build goes in process_names_extra
            "WowAB.exe",
            "Wow-32.exe",
            "Battle.net.exe",
            "explorer.exe",
            "wow.txt",
        ] {
            assert!(!is_wow_exe(name) && !looks_like_wow(name), "{name}");
        }
    }

    /// #27 review: a pattern name the table doesn't know blocks writes but
    /// isn't shown as "WoW is running" (so its exit doesn't start a
    /// game-exit backup); WowUp blocks nothing; any exe under the install
    /// root blocks; and the blocker is named.
    #[test]
    fn pattern_names_only_affect_the_gate() {
        let t = target(Path::new("/Games/World of Warcraft"));
        let w = watcher_with(vec![proc(5, "WowX.exe", None)]);
        assert_eq!(w.poll(&t), None, "not displayed as running");
        let blocker = w.blocking_now(&t).expect("writes are blocked");
        assert_eq!(blocker, Blocker::Process("WowX.exe".into()));
        assert_eq!(blocker.to_string(), "WowX.exe is running");

        let w = watcher_with(vec![
            proc(
                6,
                "WowUp-CF.exe",
                Some(PathBuf::from("/Apps/WowUp/WowUp-CF.exe")),
            ),
            proc(7, "WowUp.exe", None),
        ]);
        assert_eq!(w.poll(&t), None);
        assert!(!w.is_running_now(&t), "addon managers never block");

        let tool = PathBuf::from("/Games/World of Warcraft/Tools/Anything.exe");
        let w = watcher_with(vec![proc(8, "Anything.exe", Some(tool))]);
        assert!(
            w.is_running_now(&t),
            "anything under the install root blocks"
        );
    }

    /// R2: this app never blocks its own writes, even installed under the
    /// WoW folder.
    #[test]
    fn this_app_is_never_the_game() {
        let root = PathBuf::from("/Games/World of Warcraft");
        let me = proc(
            std::process::id(),
            "wow-forever-buddy.exe",
            Some(root.join("Tools/wow-forever-buddy.exe")),
        );
        let t = target(&root);
        let m = Matcher::new(&t);
        assert!(!m.is_ours(&me) && !m.blocks_writes(&me));
    }

    /// R2: a listing without this app (empty or failed enumeration) is
    /// "unknown": writes are blocked, the displayed status says so, and it
    /// never fakes a "WoW closed".
    #[test]
    fn failed_enumeration_is_unknown_and_blocks_writes() {
        let probe = Arc::new(FakeProbe::default());
        let w = GameWatcher::new(probe.clone());
        let t = ProbeTarget::default();
        probe.set_running(true);
        assert_eq!(w.poll(&t), Some(Transition::Started));

        probe.set_blind(true);
        assert_eq!(w.check_now(&t), GameCheck::Unknown);
        assert!(w.is_running_now(&t), "the gate fails closed");
        assert_eq!(w.poll(&t), Some(Transition::Unknown));
        assert_eq!(w.poll(&t), None, "reported once");
        let status = w.status();
        assert!(status.unknown && status.running, "no fake 'stopped'");

        probe.set_blind(false);
        assert_eq!(w.poll(&t), Some(Transition::Known));
        assert!(!w.status().unknown);
        probe.set_running(false);
        assert_eq!(w.poll(&t), Some(Transition::Stopped));
        assert_eq!(w.check_now(&t), GameCheck::NotRunning);
    }
}
