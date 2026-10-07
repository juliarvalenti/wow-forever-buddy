//! The ForeverBuddy companion addon (spec v0.2-addon §3): bundled into the
//! app, installed into `Interface/AddOns/ForeverBuddy/` through the write
//! gate, and removed the same way.
//!
//! Only the bundled files are ever written or deleted, by name. Removal
//! deletes those files and then the folder only if it's empty; a folder that
//! is a link to somewhere else is refused by the gate's path check rather
//! than followed. The SavedVariables are the player's data and stay.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::fsx::read::safe_read;
use crate::fsx::relpath::{GameRoot, RelPath};
use crate::game::gate::{MutationTarget, WriteGate};
use crate::sessions;

/// Where the addon lives, relative to the flavor folder.
pub const FOLDER: &str = "Interface/AddOns/ForeverBuddy";
const NAME: &str = "ForeverBuddy";
const TOC: &str = "ForeverBuddy.toc";

/// The bundled files, written in this order. The TOC goes last: until it's
/// there WoW doesn't see the folder, so a half-finished install is ignored.
/// `Data/` holds the bridge's slot stubs (`bridge::Slot::stub`), which the
/// app replaces with generated data.
const FILES: [(&str, &[u8]); 6] = [
    (
        "ForeverBuddy.lua",
        include_bytes!("../resources/addon/ForeverBuddy/ForeverBuddy.lua"),
    ),
    (
        "Data/Tooltip1.lua",
        include_bytes!("../resources/addon/ForeverBuddy/Data/Tooltip1.lua"),
    ),
    (
        "Data/Tooltip2.lua",
        include_bytes!("../resources/addon/ForeverBuddy/Data/Tooltip2.lua"),
    ),
    (
        "Data/Plan.lua",
        include_bytes!("../resources/addon/ForeverBuddy/Data/Plan.lua"),
    ),
    (
        "Data/Briefing.lua",
        include_bytes!("../resources/addon/ForeverBuddy/Data/Briefing.lua"),
    ),
    (
        TOC,
        include_bytes!("../resources/addon/ForeverBuddy/ForeverBuddy.toc"),
    ),
];
const DATA: &str = "Data";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct AddonStatus {
    /// From the installed TOC; `None` when it isn't installed (or has no
    /// TOC, which WoW treats the same).
    pub installed_version: Option<String>,
    /// The version this app carries.
    pub bundled_version: String,
    /// Installed, and older than the bundled one.
    pub update_available: bool,
    /// Character folders where it's enabled. A new addon is enabled
    /// unless a character's `AddOns.txt` says otherwise.
    pub enabled_on: Vec<String>,
    /// Character folders whose `AddOns.txt` turns it off. Reported, never
    /// changed by the app.
    pub disabled_on: Vec<String>,
}

fn rel(file: &str) -> RelPath {
    RelPath::new(&format!("{FOLDER}/{file}")).expect("constant paths are valid")
}

/// `## Version: 0.2.0` from a TOC.
fn toc_version(toc: &[u8]) -> Option<String> {
    String::from_utf8_lossy(toc).lines().find_map(|line| {
        let (key, value) = line.strip_prefix("##")?.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case("Version")
            .then(|| value.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

pub fn bundled_version() -> String {
    toc_version(FILES[FILES.len() - 1].1).expect("the bundled TOC has a version")
}

/// "0.2.0" < "0.10.0": numeric parts compared in order, anything that isn't
/// a number counted as 0.
fn version_key(v: &str) -> Vec<u64> {
    v.split('.')
        .map(|p| p.trim().parse().unwrap_or(0))
        .collect()
}

/// "ForeverBuddy: disabled" in a character's `AddOns.txt`. The name matches
/// case-insensitively; no line means enabled (WoW's default for a new addon).
fn disabled_in(addons_txt: &[u8]) -> bool {
    String::from_utf8_lossy(addons_txt).lines().any(|line| {
        line.split_once(':').is_some_and(|(name, state)| {
            name.trim().eq_ignore_ascii_case(NAME) && state.trim().eq_ignore_ascii_case("disabled")
        })
    })
}

pub fn status(game: &GameRoot) -> AppResult<AddonStatus> {
    let installed_version = rel(TOC)
        .resolve(game)
        .ok()
        .and_then(|path| safe_read(&path).ok())
        .and_then(|toc| toc_version(&toc));
    let bundled_version = bundled_version();
    let update_available = installed_version
        .as_deref()
        .is_some_and(|v| version_key(v) < version_key(&bundled_version));

    let (mut enabled_on, mut disabled_on) = (Vec::new(), Vec::new());
    let wtf = game.base.join("WTF");
    for c in sessions::wtf_characters(&wtf) {
        if c.older {
            continue;
        }
        let r = &c.character;
        let txt = wtf
            .join("Account")
            .join(&r.account)
            .join(&r.realm)
            .join(&r.name)
            .join("AddOns.txt");
        if read_if_there(&txt).is_some_and(|t| disabled_in(&t)) {
            disabled_on.push(r.name.clone());
        } else {
            enabled_on.push(r.name.clone());
        }
    }
    enabled_on.sort();
    disabled_on.sort();
    Ok(AddonStatus {
        installed_version,
        bundled_version,
        update_available,
        enabled_on,
        disabled_on,
    })
}

fn read_if_there(path: &Path) -> Option<Vec<u8>> {
    path.is_file().then(|| safe_read(path).ok()).flatten()
}

/// The bridge slots the installed TOC lists, so the game loads what the app
/// writes there (tooltips from 0.4.0, the plan and briefing from 0.6.0). WoW
/// reads the TOC only at client start, which is why a slot a newer addon adds
/// needs an update and a restart first. Empty when the addon isn't installed.
pub fn listed_slots(game: &GameRoot) -> Vec<crate::bridge::Slot> {
    let Some(toc) = rel(TOC)
        .resolve(game)
        .ok()
        .and_then(|path| read_if_there(&path))
    else {
        return Vec::new();
    };
    let toc = String::from_utf8_lossy(&toc);
    crate::bridge::SLOTS
        .into_iter()
        .filter(|slot| {
            let file = format!("Data/{}.lua", slot.name());
            toc.lines().any(|line| line.trim() == file)
        })
        .collect()
}

/// Installs or updates the addon: every bundled file through the write gate
/// (refused while WoW runs, with a safety snapshot first), the TOC last.
pub fn install(gate: &WriteGate, target: &MutationTarget) -> AppResult<()> {
    let paths: Vec<RelPath> = FILES.iter().map(|(name, _)| rel(name)).collect();
    let guard = gate.begin(
        "addon_install",
        target,
        &paths,
        "Before installing the ForeverBuddy addon",
    )?;
    for (path, (_, bytes)) in paths.iter().zip(FILES) {
        guard.write(path, bytes)?;
    }
    guard.commit()
}

/// Removes the addon: the bundled files (the TOC first, so WoW stops loading
/// it even if the rest fails), then the folder if nothing else is in it.
/// Returns false if the folder stayed because it holds other files.
pub fn remove(gate: &WriteGate, target: &MutationTarget) -> AppResult<bool> {
    let paths: Vec<RelPath> = FILES.iter().rev().map(|(name, _)| rel(name)).collect();
    let guard = gate.begin(
        "addon_remove",
        target,
        &paths,
        "Before removing the ForeverBuddy addon",
    )?;
    for path in &paths {
        guard.remove(path)?;
    }
    guard.remove_empty_dir(&rel(DATA))?;
    let emptied = guard.remove_empty_dir(&RelPath::new(FOLDER)?)?;
    guard.commit()?;
    Ok(emptied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{self, Slot};
    use crate::db::Db;
    use crate::error::AppError;
    use crate::game::gate::PreWriteSnapshot;
    use crate::game::process::fake::FakeProbe;
    use crate::game::process::{GameWatcher, ProbeTarget};
    use crate::sv::LuaValue;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Snapshots(Mutex<Vec<String>>);

    impl PreWriteSnapshot for Snapshots {
        fn snapshot_before_write(
            &self,
            _game: &GameRoot,
            op: &str,
            _paths: &[RelPath],
            _label: &str,
        ) -> AppResult<String> {
            self.0.lock().unwrap().push(op.to_string());
            Ok("snap".into())
        }
    }

    struct Setup {
        _dir: tempfile::TempDir,
        flavor: PathBuf,
        probe: Arc<FakeProbe>,
        snapshots: Arc<Snapshots>,
        gate: WriteGate,
        target: MutationTarget,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let flavor = dir.path().join("_classic_beta_");
        let account = flavor.join("WTF/Account/ACCOUNT1");
        for (folder, addons_txt) in [
            (
                "70/Ellygie-Vargur",
                Some("Details: enabled\nForeverBuddy: enabled\n"),
            ),
            ("70/Brannic", Some("foreverbuddy: disabled\n")),
            ("70/Sela", None),
            (
                "Classic Beta PvP 2/Ellygie",
                Some("ForeverBuddy: disabled\n"),
            ),
        ] {
            std::fs::create_dir_all(account.join(folder)).unwrap();
            if let Some(txt) = addons_txt {
                std::fs::write(account.join(folder).join("AddOns.txt"), txt).unwrap();
            }
        }
        std::fs::create_dir_all(flavor.join("Interface/AddOns")).unwrap();
        let probe = Arc::new(FakeProbe::default());
        let snapshots = Arc::new(Snapshots::default());
        let gate = WriteGate::new(
            Arc::new(GameWatcher::new(probe.clone())),
            snapshots.clone(),
            Db::open_in_memory().unwrap(),
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
        }
    }

    #[test]
    fn reads_versions_from_tocs() {
        assert_eq!(bundled_version(), "0.6.0");
        assert_eq!(
            toc_version(b"## Interface: 16001\r\n##Version:  0.1.9 \r\n"),
            Some("0.1.9".into())
        );
        assert_eq!(toc_version(b"## Title: x\n"), None);
        assert!(version_key("0.2.0") < version_key("0.10.0"));
        assert!(version_key("0.2") < version_key("0.2.1"));
    }

    #[test]
    fn status_before_and_after_install() {
        let t = setup();
        let before = status(&t.target.game).unwrap();
        assert_eq!(before.installed_version, None);
        assert!(!before.update_available);
        // No line or "enabled" is on; "disabled" (any case) is off. The older
        // settings folder isn't a character and isn't listed.
        assert_eq!(before.enabled_on, ["Ellygie-Vargur", "Sela"]);
        assert_eq!(before.disabled_on, ["Brannic"]);

        install(&t.gate, &t.target).unwrap();
        let after = status(&t.target.game).unwrap();
        assert_eq!(after.installed_version.as_deref(), Some("0.6.0"));
        assert!(!after.update_available);
        for (name, bytes) in FILES {
            let path = t.flavor.join(FOLDER).join(name);
            assert_eq!(std::fs::read(path).unwrap(), bytes, "{name}");
        }
        assert_eq!(*t.snapshots.0.lock().unwrap(), ["addon_install"]);
    }

    #[test]
    fn an_older_install_is_updated() {
        let t = setup();
        let dir = t.flavor.join(FOLDER);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(TOC), "## Version: 0.1.0\n").unwrap();
        std::fs::write(dir.join("ForeverBuddy.lua"), "-- old").unwrap();
        assert!(status(&t.target.game).unwrap().update_available);

        install(&t.gate, &t.target).unwrap();
        let s = status(&t.target.game).unwrap();
        assert_eq!(s.installed_version.as_deref(), Some("0.6.0"));
        assert!(!s.update_available);
    }

    #[test]
    fn refused_while_wow_runs() {
        let t = setup();
        t.probe.set_running(true);
        assert!(matches!(
            install(&t.gate, &t.target),
            Err(AppError::GameRunning(_))
        ));
        assert!(!t.flavor.join(FOLDER).exists(), "nothing written");
        assert!(matches!(
            remove(&t.gate, &t.target),
            Err(AppError::GameRunning(_))
        ));
    }

    #[test]
    fn remove_deletes_the_addon_and_keeps_saved_variables() {
        let t = setup();
        install(&t.gate, &t.target).unwrap();
        let sv = t
            .flavor
            .join("WTF/Account/ACCOUNT1/70/Sela/SavedVariables/ForeverBuddy.lua");
        std::fs::create_dir_all(sv.parent().unwrap()).unwrap();
        std::fs::write(&sv, "ForeverBuddyDB = {}\n").unwrap();

        assert!(remove(&t.gate, &t.target).unwrap());
        assert!(!t.flavor.join(FOLDER).exists());
        assert!(sv.exists(), "the player's data stays");
        assert_eq!(status(&t.target.game).unwrap().installed_version, None);
    }

    #[test]
    fn remove_leaves_other_files_and_their_folder() {
        let t = setup();
        install(&t.gate, &t.target).unwrap();
        let mine = t.flavor.join(FOLDER).join("notes.txt");
        std::fs::write(&mine, "the player's").unwrap();

        assert!(!remove(&t.gate, &t.target).unwrap(), "folder kept");
        assert!(mine.exists());
        assert!(!t.flavor.join(FOLDER).join(TOC).exists());
    }

    /// Security blocker for V4: an addon folder that's a link (junction) to
    /// somewhere else is never followed, so files there can't be deleted.
    #[test]
    fn remove_never_follows_a_linked_folder_out() {
        let t = setup();
        let elsewhere = t._dir.path().join("elsewhere");
        std::fs::create_dir_all(elsewhere.join(DATA)).unwrap();
        for (name, _) in FILES {
            std::fs::write(elsewhere.join(name), "not the game's").unwrap();
        }
        crate::test_support::link_dir(&elsewhere, &t.flavor.join(FOLDER));

        assert!(matches!(
            remove(&t.gate, &t.target),
            Err(AppError::PathEscape(_))
        ));
        assert!(matches!(
            install(&t.gate, &t.target),
            Err(AppError::PathEscape(_))
        ));
        for (name, _) in FILES {
            assert_eq!(
                std::fs::read_to_string(elsewhere.join(name)).unwrap(),
                "not the game's"
            );
        }
    }

    // Bridge slots (bridge spec §2-3), written through `WriteGate::write_slots`.

    fn slot_file(slot: Slot, n: i64) -> Vec<u8> {
        let mut t = bridge::header(n);
        t.hash.push((LuaValue::str("items"), LuaValue::Int(n)));
        bridge::render(slot, t).unwrap()
    }

    #[test]
    fn bundled_slot_stubs_match_the_bridge() {
        for slot in bridge::SLOTS {
            let name = format!("Data/{}.lua", slot.name());
            let (_, bytes) = FILES.iter().find(|(n, _)| *n == name).unwrap();
            assert_eq!(*bytes, slot.stub().as_slice(), "{name}");
        }
    }

    #[test]
    fn slots_are_written_into_our_data_folder() {
        let t = setup();
        install(&t.gate, &t.target).unwrap();
        let files: Vec<_> = bridge::SLOTS
            .iter()
            .map(|&s| (s, slot_file(s, 7)))
            .collect();
        t.gate.write_slots(&t.target, &files).unwrap();
        for (slot, bytes) in &files {
            let on_disk = std::fs::read(t.flavor.join(slot.path().as_string())).unwrap();
            assert_eq!(&on_disk, bytes);
        }
        assert_eq!(
            *t.snapshots.0.lock().unwrap(),
            ["addon_install"],
            "no snapshot for our own data"
        );
    }

    /// B1: slots are written only while WoW is closed, until probe run 4
    /// shows `/reload` re-reads them (spec §8, B4).
    #[test]
    fn slots_wait_for_wow_to_close() {
        let t = setup();
        install(&t.gate, &t.target).unwrap();
        t.probe.set_running(true);
        let files = [(Slot::Tooltip1, slot_file(Slot::Tooltip1, 7))];
        assert!(matches!(
            t.gate.write_slots(&t.target, &files),
            Err(AppError::GameRunning(_))
        ));
        let on_disk = std::fs::read(t.flavor.join(Slot::Tooltip1.path().as_string())).unwrap();
        assert_eq!(on_disk, Slot::Tooltip1.stub(), "unchanged");
    }

    /// One bad slot leaves the whole set as it was.
    #[test]
    fn a_bad_slot_stops_the_whole_batch() {
        let t = setup();
        install(&t.gate, &t.target).unwrap();
        let files = [
            (Slot::Tooltip1, slot_file(Slot::Tooltip1, 7)),
            (
                Slot::Tooltip2,
                b"ForeverBuddyData_Tooltip2 = os.exit()\n".to_vec(),
            ),
        ];
        assert!(matches!(
            t.gate.write_slots(&t.target, &files),
            Err(AppError::SlotRefused(_))
        ));
        // A slot's bytes under another slot's name are refused too.
        let swapped = [(Slot::Tooltip2, slot_file(Slot::Tooltip1, 7))];
        assert!(matches!(
            t.gate.write_slots(&t.target, &swapped),
            Err(AppError::SlotRefused(_))
        ));
        for slot in bridge::SLOTS {
            let on_disk = std::fs::read(t.flavor.join(slot.path().as_string())).unwrap();
            assert_eq!(on_disk, slot.stub(), "{} unchanged", slot.name());
        }
    }

    /// Security condition 1: a linked `ForeverBuddy` or `Data` folder is
    /// refused, never followed.
    #[test]
    fn slots_never_follow_a_linked_folder_out() {
        for linked in [FOLDER.to_string(), format!("{FOLDER}/{DATA}")] {
            let t = setup();
            let elsewhere = t._dir.path().join("elsewhere");
            std::fs::create_dir_all(elsewhere.join(DATA)).unwrap();
            let link = t.flavor.join(&linked);
            std::fs::create_dir_all(link.parent().unwrap()).unwrap();
            let target = if linked == FOLDER {
                elsewhere.clone()
            } else {
                elsewhere.join(DATA)
            };
            crate::test_support::link_dir(&target, &link);

            let files = [(Slot::Tooltip1, slot_file(Slot::Tooltip1, 7))];
            assert!(
                matches!(
                    t.gate.write_slots(&t.target, &files),
                    Err(AppError::PathEscape(_))
                ),
                "{linked}"
            );
            assert!(
                !elsewhere.join(DATA).join("Tooltip1.lua").exists(),
                "{linked}"
            );
        }
    }

    /// The tooltip index end to end (bridge spec §5): only into an addon
    /// that lists the slots, rewritten only when the data changes, and held
    /// while WoW runs.
    #[test]
    fn the_tooltip_index_is_sent_when_it_changes() {
        use bridge::Sent;
        let t = setup();
        let db = Db::open_in_memory().unwrap();
        let add = |count: i64| {
            db.with_conn(|c| {
                c.execute(
                    "INSERT OR IGNORE INTO characters (id, flavor, account, group_dir, char_dir,
                                                      name, class, first_seen, last_seen)
                     VALUES (1, '_classic_beta_', 'ACCOUNT1', '70', 'Sela', 'Sela', 'PRIEST', 1, 1)",
                    [],
                )?;
                c.execute(
                    "INSERT INTO char_items (character_id, location, container, slot, item_id,
                                             link, count, as_of)
                     VALUES (1, 'bag', 0, ?1, 14047, '', ?2, 1)",
                    [count, count],
                )?;
                Ok(())
            })
            .unwrap();
        };
        add(5);
        let send = |stamp| bridge::send(&db, &t.gate, &t.target, "_classic_beta_", stamp);

        assert_eq!(send(1).unwrap(), Sent::NoAddon, "not installed");
        install(&t.gate, &t.target).unwrap();
        assert_eq!(send(2).unwrap(), Sent::Written);
        let file = t.flavor.join(Slot::Tooltip2.path().as_string());
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("[14047] = {"), "{written}");
        assert_eq!(send(3).unwrap(), Sent::Unchanged, "same data, new stamp");

        add(7);
        t.probe.set_running(true);
        assert_eq!(send(4).unwrap(), Sent::Waiting);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), written);
        t.probe.set_running(false);
        assert_eq!(send(5).unwrap(), Sent::Written);
        let status: String = db
            .with_conn(|c| {
                Ok(c.query_row(
                    "SELECT status FROM bridge_slots WHERE slot = 'Tooltip2' AND stamp = 5",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(status, "written");
    }

    /// An addon a version behind (0.5.0's TOC lists the tooltips but not
    /// the briefing) keeps getting its tooltips; the slot it can't load
    /// isn't written.
    #[test]
    fn only_the_slots_the_installed_toc_lists_are_sent() {
        use bridge::Sent;
        let t = setup();
        let db = Db::open_in_memory().unwrap();
        let dir = t.flavor.join(FOLDER);
        std::fs::create_dir_all(dir.join(DATA)).unwrap();
        std::fs::write(
            dir.join(TOC),
            "## Version: 0.5.0\nData/Tooltip1.lua\nData/Tooltip2.lua\nForeverBuddy.lua\n",
        )
        .unwrap();
        assert_eq!(
            listed_slots(&t.target.game),
            [Slot::Tooltip1, Slot::Tooltip2]
        );
        assert_eq!(
            bridge::send(&db, &t.gate, &t.target, "_classic_beta_", 1).unwrap(),
            Sent::Written
        );
        assert!(t.flavor.join(Slot::Tooltip1.path().as_string()).exists());
        assert!(!t.flavor.join(Slot::Briefing.path().as_string()).exists());
    }
}
