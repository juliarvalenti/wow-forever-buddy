//! The files the ForeverBuddy addon writes, as produced by its Lua harness
//! (`tools/addon-test/run.lua`), against the `sv` parser and the integrity
//! contract ingest relies on (spec v0.2-addon §2): exactly one global, and
//! `_meta.counts` matching a recount of the tables they describe.

use std::path::PathBuf;
use wow_forever_buddy_lib::sv::{self, LuaTable, LuaValue};

fn fixtures() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/addon");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "lua"))
        .collect();
    files.sort();
    files
}

/// Entries in a table, positional and keyed: what the addon's `count` gives.
fn entries(v: Option<&LuaValue>) -> i64 {
    v.and_then(LuaValue::as_table)
        .map_or(0, |t| (t.array.len() + t.hash.len()) as i64)
}

fn values(t: &LuaTable) -> impl Iterator<Item = &LuaValue> {
    t.array.iter().chain(t.hash.iter().map(|(_, v)| v))
}

fn int(v: Option<&LuaValue>) -> Option<i64> {
    match v {
        Some(LuaValue::Int(i)) => Some(*i),
        _ => None,
    }
}

#[test]
fn every_fixture_parses_and_its_counts_add_up() {
    let files = fixtures();
    assert!(
        files.len() >= 5,
        "expected the harness's fixtures, found {}",
        files.len()
    );
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy();
        let globals =
            sv::parse(&std::fs::read(&path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(globals.len(), 1, "{name}: one global");
        assert_eq!(globals[0].0, "ForeverBuddyDB", "{name}");
        let db = globals[0].1.as_table().expect("a table");
        let meta = db.get("_meta").and_then(LuaValue::as_table).expect("_meta");
        assert_eq!(int(meta.get("schema")), Some(1), "{name}: schema");

        let sessions = db.get("sessions").and_then(LuaValue::as_table);
        let events: i64 = sessions
            .into_iter()
            .flat_map(values)
            .map(|s| entries(s.as_table().and_then(|s| s.get("events"))))
            .sum();
        let bag_items: i64 = db
            .get("snapshot")
            .and_then(LuaValue::as_table)
            .and_then(|s| s.get("bags"))
            .and_then(LuaValue::as_table)
            .into_iter()
            .flat_map(values)
            .map(|b| entries(b.as_table().and_then(|b| b.get("items"))))
            .sum();

        let counts = meta
            .get("counts")
            .and_then(LuaValue::as_table)
            .expect("counts");
        // `quests_done` is counted only in files that have the list.
        let quests_done = db
            .get("snapshot")
            .and_then(LuaValue::as_table)
            .and_then(|s| s.get("quests_done"));
        let expected = if quests_done.is_some() { 5 } else { 4 };
        assert_eq!(
            counts.array.len() + counts.hash.len(),
            expected,
            "{name}: counts"
        );
        for (key, want) in [
            ("sessions", entries(db.get("sessions"))),
            ("events", events),
            ("items", entries(db.get("items"))),
            ("bag_items", bag_items),
        ] {
            assert_eq!(int(counts.get(key)), Some(want), "{name}: counts.{key}");
        }
        if quests_done.is_some() {
            assert_eq!(
                int(counts.get("quests_done")),
                Some(entries(quests_done)),
                "{name}: counts.quests_done"
            );
        }
    }
}
