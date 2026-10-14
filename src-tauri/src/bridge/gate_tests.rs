//! G3: the write gate against a Windows session's process lists
//! (tests/fixtures/processes/windows-session.txt), and the invariant that
//! BUG-LISTS broke: a send the gate held goes out once nothing holds it,
//! and never while something does.

use std::path::PathBuf;
use std::sync::Arc;

use proptest::prelude::*;

use super::{Sent, Slot};
use crate::config::paths::AppPaths;
use crate::game::process::fake::FakeProbe;
use crate::game::process::{Blocker, ProcInfo};
use crate::secrets::MemoryStore;
use crate::state::AppCore;

struct Step {
    name: String,
    procs: Vec<(String, Option<String>)>,
    /// The exe the gate must stop at, or `None` for free.
    held: Option<String>,
}

fn steps() -> Vec<Step> {
    let text = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/processes/windows-session.txt"),
    )
    .unwrap();
    let mut out: Vec<Step> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix("== ") {
            out.push(Step {
                name: name.into(),
                procs: Vec::new(),
                held: None,
            });
            continue;
        }
        let step = out.last_mut().expect("a process before the first step");
        if let Some(expect) = line.strip_prefix("expect: ") {
            step.held = match expect {
                "free" => None,
                held => Some(held.strip_prefix("held ").expect("held <exe>").into()),
            };
            continue;
        }
        let (name, path) = line.split_once('|').expect("name|path");
        step.procs
            .push((name.into(), (!path.is_empty()).then(|| path.into())));
    }
    assert!(out.len() >= 5, "the fixture's steps");
    out
}

struct Session {
    _dir: tempfile::TempDir,
    core: AppCore,
    probe: Arc<FakeProbe>,
    /// The WoW folder as the OS reports it (a temp dir's /var is
    /// /private/var on macOS).
    wow: PathBuf,
    lists: u32,
}

fn session() -> Session {
    let (dir, root) = crate::test_support::fixture_copy();
    let probe = Arc::new(FakeProbe::default());
    let core = AppCore::with_parts(
        AppPaths::under(&dir.path().join("app")),
        Arc::new(MemoryStore::default()),
        probe.clone(),
    )
    .unwrap();
    crate::install::set(&core.settings, &root, Some("_classic_beta_")).unwrap();
    crate::addon::install(
        &core.write_gate().unwrap(),
        &core.mutation_target().unwrap(),
    )
    .unwrap();
    let wow = dunce::canonicalize(&root).unwrap();
    Session {
        _dir: dir,
        core,
        probe,
        wow,
        lists: 0,
    }
}

impl Session {
    fn enter(&self, step: &Step) {
        let wow = self.wow.to_string_lossy().replace('\\', "/");
        let list = step
            .procs
            .iter()
            .enumerate()
            .map(|(i, (name, path))| ProcInfo {
                pid: FakeProbe::wow_pid() + i as u32,
                name: name.clone(),
                exe: path
                    .as_ref()
                    .map(|p| PathBuf::from(p.replace("{WOW}", &wow))),
            })
            .collect();
        self.probe.set_processes(list);
        self.core.game.poll(&self.core.probe_target());
    }

    fn blocker(&self) -> Option<String> {
        match self.core.game.blocking_now(&self.core.probe_target()) {
            None => None,
            Some(Blocker::Process(name)) => Some(name),
            Some(Blocker::Unknown) => Some("(unknown)".into()),
        }
    }

    /// Something the app wants in the game: a new list, so the Lists slot
    /// changes. Returns its name.
    fn change(&mut self) -> String {
        self.lists += 1;
        let name = format!("List {}", self.lists);
        let flavor = self.core.active_game().unwrap().flavor;
        crate::lists::create_list(&self.core.db, &flavor, &name, None, "app").unwrap();
        name
    }

    /// What the game would load now from the Lists slot.
    fn in_game(&self) -> String {
        let game = self.core.mutation_target().unwrap().game;
        let path = Slot::Lists.path().resolve(&game).unwrap();
        std::fs::read_to_string(path).unwrap_or_default()
    }
}

/// Each step of the session: the gate says what the fixture expects, and
/// the screens' `holding` names the same exe.
#[test]
fn the_gate_reads_a_windows_session() {
    let s = session();
    for step in steps() {
        s.enter(&step);
        assert_eq!(s.blocker(), step.held, "{}", step.name);
        assert_eq!(
            s.core.game.status().holding,
            step.held,
            "{}: holding",
            step.name
        );
    }
}

/// Walking the session in order: a change made while the game runs waits,
/// and goes out by itself (the 10-second retry) once the game is gone,
/// even with Blizzard's helpers still running.
#[test]
fn a_held_send_goes_out_when_the_session_frees_the_gate() {
    let mut s = session();
    let steps = steps();
    let mut wanted: Option<String> = None;
    let mut held = false;
    for step in &steps {
        s.enter(step);
        if held && step.held.is_none() {
            // The retry alone delivers what waited, before anything new.
            let retried = s.core.retry_waiting_send();
            assert!(
                matches!(retried, Some(Ok(Sent::Written))),
                "{}: the held send goes out",
                step.name
            );
            assert!(
                s.in_game().contains(wanted.as_deref().unwrap()),
                "{}",
                step.name
            );
        }
        held = step.held.is_some();
        let name = s.change();
        let sent = s.core.send_to_game().unwrap();
        if step.held.is_some() {
            assert_eq!(sent, Sent::Waiting, "{}: held", step.name);
            assert!(
                !s.in_game().contains(&name),
                "{}: nothing written",
                step.name
            );
            assert!(
                s.core.retry_waiting_send().is_none(),
                "{}: not retried while held",
                step.name
            );
        } else {
            assert_eq!(sent, Sent::Written, "{}", step.name);
        }
        wanted = Some(name);
    }
    let last = steps.last().unwrap();
    assert!(last.held.is_none(), "the session ends free");
    assert!(s.in_game().contains(wanted.as_deref().unwrap()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Any order of the session's steps, with or without a change at each:
    /// nothing is written while the gate is held, and whenever it's free
    /// the retry leaves nothing waiting and the game has the newest data.
    #[test]
    fn whatever_the_order_a_held_send_goes_out_once_free(
        walk in prop::collection::vec((0usize..8, any::<bool>()), 1..12)
    ) {
        let steps = steps();
        let mut s = session();
        let mut newest: Option<String> = None;
        for (i, change) in walk {
            let step = &steps[i % steps.len()];
            let before = s.in_game();
            s.enter(step);
            if change {
                newest = Some(s.change());
                let _ = s.core.send_to_game().unwrap();
            }
            let retried = s.core.retry_waiting_send();
            if step.held.is_some() {
                prop_assert!(retried.is_none(), "{}: retried while held", step.name);
                prop_assert_eq!(s.in_game(), before, "{}: written while held", step.name);
            } else {
                if let Some(r) = retried {
                    prop_assert!(r.is_ok());
                }
                prop_assert!(s.core.retry_waiting_send().is_none(), "{}: still waiting while free", step.name);
                if let Some(name) = &newest {
                    prop_assert!(s.in_game().contains(name.as_str()), "{}: the newest change is out", step.name);
                }
            }
        }
    }
}
