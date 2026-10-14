//! H5: the end-to-end contract between the addon and the app, as one test.
//!
//! 1. The addon writes SavedVariables in its harness (tools/addon-test, the
//!    fixtures in tests/fixtures/addon/, checked by the harness's --check).
//! 2. The app ingests one of those files, adds what only the app makes (a
//!    list, a plan, a cleanup mark, a login note), and builds every bridge
//!    slot exactly as `send_to_game` does (`bridge::build`).
//! 3. The slots are written to tests/fixtures/bridge/, and the harness runs
//!    (in Lua 5.1, through mlua) with its `contract` scenario loading those
//!    bytes as the game would and checking the addon reads each one.
//!
//! CI runs this on Windows and macOS and then fails if tests/fixtures/bridge/
//! changed, so the app's output and the addon's reading of it can't drift
//! apart unnoticed: a change on either side has to update the fixtures, and
//! the other side's checks run against them.

use std::path::{Path, PathBuf};

use crate::bridge::{self, Slot, SLOTS};
use crate::db::Db;
use crate::ingest::{ingest_bytes, target_for, Outcome};

const FLAVOR: &str = "_classic_beta_";
/// The slots' stamp: fixed, so the fixtures only change when the data does.
/// (The harness's clock starts at 1790964000.)
pub const STAMP: i64 = 1_790_960_000;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The app's side: the harness's `lists` file ingested for Thrandor, plus
/// one of everything the app adds itself.
fn seeded() -> Db {
    let db = Db::open_in_memory().unwrap();
    let file = std::fs::read(root().join("src-tauri/tests/fixtures/addon/lists.lua")).unwrap();
    let target = target_for(
        FLAVOR,
        "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
    )
    .unwrap();
    assert!(matches!(
        ingest_bytes(&db, &target, &file).unwrap(),
        Outcome::Applied(_)
    ));
    let me: u32 = db
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT id FROM characters WHERE name = 'Thrandor'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();

    let list = crate::lists::create_list(&db, FLAVOR, "Mats", Some(me), "app").unwrap();
    crate::lists::add_item(&db, list, &crate::lists::NewItem::Id(2589), 20).unwrap();
    crate::plans::set_plan(
        &db,
        me,
        "Hogger",
        &[
            crate::plans::Step {
                text: "Take Wanted: Hogger".into(),
                quest_id: Some(176),
                zone: Some("Elwynn Forest".into()),
                kind: Some(crate::plans::StepKind::Accept),
            },
            crate::plans::Step {
                text: "Hand it in".into(),
                quest_id: Some(176),
                zone: Some("Elwynn Forest".into()),
                kind: None,
            },
        ],
        "app",
    )
    .unwrap();
    crate::cleanup::mark(&db, me, 2589, crate::cleanup::Mark::Sell).unwrap();
    crate::notes::add(
        &db,
        FLAVOR,
        &crate::notes::NewNote {
            character_id: me,
            text: "Train Tailoring".into(),
            once: true,
            until: None,
        },
        crate::notes::Author::You,
        STAMP,
    )
    .unwrap();
    db
}

fn write_fixtures(dir: &Path) {
    let db = seeded();
    let built = bridge::build(&db, FLAVOR, STAMP, &SLOTS).unwrap();
    assert!(!built.too_large);
    assert_eq!(built.slots.len(), SLOTS.len(), "every slot");
    std::fs::create_dir_all(dir).unwrap();
    // The header's app version would change the fixtures on every release;
    // the addon only reports it back, so it's pinned here.
    let app = format!("[\"app\"] = \"{}\"", env!("CARGO_PKG_VERSION"));
    for (slot, bytes) in &built.slots {
        bridge::check(*slot, bytes).unwrap_or_else(|e| panic!("{}: {e}", slot.name()));
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(
            text.contains(&app),
            "{}: the header's app version",
            slot.name()
        );
        let text = text.replacen(&app, "[\"app\"] = \"test\"", 1);
        std::fs::write(dir.join(format!("{}.lua", slot.name())), text).unwrap();
    }
}

/// Runs tools/addon-test/run.lua --check in Lua 5.1, as CI's addon job
/// does, but here on every OS the Rust tests run on. Its output is printed;
/// a failure (os.exit(1) in the harness) fails the test.
fn run_harness() {
    use mlua::{Lua, LuaOptions, StdLib};
    // The harness uses debug.traceback for its failure messages, and mlua
    // only loads `debug` unsafely. It's our own test script, in a test.
    let lua =
        unsafe { Lua::unsafe_new_with(StdLib::ALL_SAFE | StdLib::DEBUG, LuaOptions::default()) };
    let script = root().join("tools/addon-test/run.lua");
    let src = std::fs::read_to_string(&script).unwrap();
    let path = script.to_string_lossy().replace('\\', "/");
    // H4's chaos seeds: a sample here (the addon job on Linux runs them
    // all), unless CHAOS_SEEDS or CHAOS_ONLY says otherwise.
    lua.load(format!(
        r#"arg = {{ [0] = {path:?}, "--check" }}
        os.exit = function(code) error("the harness failed (exit " .. tostring(code) .. ")", 0) end
        local getenv = os.getenv
        os.getenv = function(k)
            local v = getenv(k)
            if k == "CHAOS_SEEDS" and v == nil and getenv("CHAOS_ONLY") == nil then
                return "40"
            end
            return v
        end"#
    ))
    .exec()
    .unwrap();
    if let Err(e) = lua.load(&src).set_name("@run.lua").exec() {
        panic!("{e}");
    }
}

#[test]
fn the_addon_reads_what_the_app_writes() {
    let dir = root().join("src-tauri/tests/fixtures/bridge");
    write_fixtures(&dir);
    // Every slot the TOC lists has a fixture, and nothing else is there.
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut want: Vec<String> = SLOTS
        .iter()
        .map(|s: &Slot| format!("{}.lua", s.name()))
        .collect();
    want.sort();
    assert_eq!(names, want);
    run_harness();
}
