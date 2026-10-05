//! The bridge, app → addon (docs/specs/bridge-v0.4.md §2): fixed data files
//! ("slots") inside our own addon folder that the addon loads at login or
//! `/reload`.
//!
//! WoW runs these files as Lua, so the app only ever writes data: one global
//! table of strings, numbers and booleans, serialized by `sv::write_globals`
//! and parsed back with the data-only `sv` parser before every write
//! (`check`). The list of slots is a constant; no path ever comes from the
//! UI or an agent. Writing goes through `WriteGate::write_slots`.
//!
//! B1 adds the slots and the writer; the first producer (the tooltip index,
//! T-TIP) is the next change, so `header`, `render` and `stub` have only
//! tests as callers until then.
#![allow(dead_code)]

use crate::error::{AppError, AppResult};
use crate::fsx::relpath::RelPath;
use crate::sv::{self, LuaTable, LuaValue};

/// The largest slot file, so no producer can bloat the client's load time.
pub const MAX_SLOT_BYTES: usize = 1024 * 1024;
/// Deeper than any slot needs; a deeper table is refused.
const MAX_DEPTH: usize = 8;
/// The `schema` every slot carries; the addon ignores ones it doesn't know.
pub const SCHEMA: i64 = 1;

/// A data slot. Each is listed in the addon's TOC, so adding one needs an
/// addon release (and a client restart), never a runtime change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    /// The tooltip index, item ids with id % 2 == 0 (spec §5).
    Tooltip1,
    /// The tooltip index, odd item ids.
    Tooltip2,
}

pub const SLOTS: [Slot; 2] = [Slot::Tooltip1, Slot::Tooltip2];

impl Slot {
    pub fn name(self) -> &'static str {
        match self {
            Slot::Tooltip1 => "Tooltip1",
            Slot::Tooltip2 => "Tooltip2",
        }
    }

    /// The one global the file sets, e.g. `ForeverBuddyData_Tooltip1`.
    pub fn global(self) -> String {
        format!("ForeverBuddyData_{}", self.name())
    }

    /// Relative to the flavor folder.
    pub fn path(self) -> RelPath {
        RelPath::new(&format!(
            "{}/Data/{}.lua",
            crate::addon::FOLDER,
            self.name()
        ))
        .expect("constant paths are valid")
    }

    /// The file the addon ships before the app has written anything.
    pub fn stub(self) -> Vec<u8> {
        format!("{} = nil\n", self.global()).into_bytes()
    }
}

/// The header every slot starts with: `schema`, `stamp` (when the app
/// generated it, echoed back in the addon's receipt) and the app version.
pub fn header(stamp: i64) -> LuaTable {
    LuaTable {
        array: Vec::new(),
        hash: vec![
            (LuaValue::str("schema"), LuaValue::Int(SCHEMA)),
            (LuaValue::str("stamp"), LuaValue::Int(stamp)),
            (
                LuaValue::str("app"),
                LuaValue::str(env!("CARGO_PKG_VERSION")),
            ),
        ],
    }
}

/// Serializes a slot's table and checks the bytes (`check`), including that
/// they parse back to exactly `body`.
pub fn render(slot: Slot, body: LuaTable) -> AppResult<Vec<u8>> {
    let value = LuaValue::Table(Box::new(body));
    let bytes = sv::write_globals(&[(slot.global(), value.clone())]);
    let parsed = check(slot, &bytes)?;
    if parsed != value {
        return Err(refused(slot, "doesn't read back as written"));
    }
    Ok(bytes)
}

/// The runtime check every slot passes before it's written (security's
/// condition 3): at most `MAX_SLOT_BYTES`, and exactly one global, the
/// slot's own, holding a table of strings, finite numbers and booleans at
/// most `MAX_DEPTH` deep. The parser accepts data only, so code can't get
/// through it. Returns the parsed table.
pub fn check(slot: Slot, bytes: &[u8]) -> AppResult<LuaValue> {
    if bytes.len() > MAX_SLOT_BYTES {
        return Err(refused(
            slot,
            &format!("{} KB is over the 1 MB cap", bytes.len() / 1024),
        ));
    }
    let mut globals = sv::parse(bytes).map_err(|e| refused(slot, &format!("unreadable: {e}")))?;
    if globals.len() != 1 || globals[0].0 != slot.global() {
        return Err(refused(slot, "must set exactly its own global"));
    }
    let (_, value) = globals.remove(0);
    if !matches!(value, LuaValue::Table(_)) {
        return Err(refused(slot, "must be a table"));
    }
    data_only(&value, 0).map_err(|why| refused(slot, why))?;
    Ok(value)
}

fn data_only(v: &LuaValue, depth: usize) -> Result<(), &'static str> {
    match v {
        LuaValue::Bool(_) | LuaValue::Int(_) | LuaValue::Str(_) => Ok(()),
        LuaValue::Num(n) if n.is_finite() => Ok(()),
        LuaValue::Num(_) => Err("numbers must be finite"),
        LuaValue::Nil => Err("no nil values"),
        LuaValue::Table(_) if depth >= MAX_DEPTH => Err("nested too deep"),
        LuaValue::Table(t) => {
            for item in &t.array {
                data_only(item, depth + 1)?;
            }
            for (k, item) in &t.hash {
                if !matches!(k, LuaValue::Str(_) | LuaValue::Int(_)) {
                    return Err("keys must be strings or integers");
                }
                data_only(item, depth + 1)?;
            }
            Ok(())
        }
    }
}

fn refused(slot: Slot, why: &str) -> AppError {
    AppError::SlotRefused(format!("{}: {why}", slot.name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(entries: Vec<(LuaValue, LuaValue)>) -> LuaTable {
        let mut t = header(1_759_698_240);
        t.hash.extend(entries);
        t
    }

    /// Hostile strings round-trip as plain data: they can't end the string
    /// early, open a comment or run code. (`|H` and `|T` are inert here;
    /// the addon's `plain()` doubles `|` at display.)
    #[test]
    fn hostile_strings_stay_data() {
        let hostile: Vec<&[u8]> = vec![
            b"]]",
            b"\"); os.exit() --",
            b"back\\slash\\",
            b"line\nbreak\r\n",
            b"\x00\x01\x1f\x7f",
            b"\xff\xfe not utf-8",
            b"--[[ comment",
            b"|Hitem:19019::::::::60:::::|h[Thunderfury]|h",
            b"|TInterface\\Icons\\INV_Misc_QuestionMark:0|t",
            b"|cffff0000red|r",
        ];
        let entries = hostile
            .iter()
            .enumerate()
            .map(|(i, s)| (LuaValue::Int(i as i64 + 1), LuaValue::str(s)))
            .collect();
        let bytes = render(Slot::Tooltip1, body(entries)).unwrap();
        let LuaValue::Table(t) = check(Slot::Tooltip1, &bytes).unwrap() else {
            panic!("a table");
        };
        for (i, s) in hostile.iter().enumerate() {
            assert_eq!(
                t.get_index(i as i64 + 1).and_then(|v| v.as_bytes()),
                Some(*s)
            );
        }
    }

    #[test]
    fn only_its_own_global_holding_data() {
        let slot = Slot::Tooltip2;
        for (src, why) in [
            (
                &b"ForeverBuddyData_Tooltip1 = {}\n"[..],
                "another slot's global",
            ),
            (
                b"ForeverBuddyData_Tooltip2 = {}\nX = 1\n",
                "a second global",
            ),
            (b"ForeverBuddyData_Tooltip2 = 1\n", "not a table"),
            (b"ForeverBuddyData_Tooltip2 = { 1/0 }\n", "infinite"),
            (b"ForeverBuddyData_Tooltip2 = { nil }\n", "nil"),
            (b"ForeverBuddyData_Tooltip2 = { [true] = 1 }\n", "bool key"),
            (
                b"ForeverBuddyData_Tooltip2 = {{{{{{{{{}}}}}}}}}\n",
                "too deep",
            ),
            (b"ForeverBuddyData_Tooltip2 = os.exit()\n", "code"),
            (
                b"ForeverBuddyData_Tooltip2 = { f = function() end }\n",
                "a function",
            ),
        ] {
            assert!(
                matches!(check(slot, src), Err(AppError::SlotRefused(_))),
                "{why}"
            );
        }
        assert!(check(
            slot,
            b"ForeverBuddyData_Tooltip2 = { 1, \"a\", true, 2.5 }\n"
        )
        .is_ok());
    }

    #[test]
    fn over_the_cap_is_refused() {
        let big = LuaValue::str(vec![b'x'; MAX_SLOT_BYTES]);
        let err = render(Slot::Tooltip1, body(vec![(LuaValue::str("big"), big)])).unwrap_err();
        assert!(matches!(err, AppError::SlotRefused(m) if m.contains("1 MB cap")));
    }

    #[test]
    fn slots_are_constant_files_in_our_data_folder() {
        assert_eq!(
            SLOTS.map(|s| s.path().as_string()),
            [
                "Interface/AddOns/ForeverBuddy/Data/Tooltip1.lua",
                "Interface/AddOns/ForeverBuddy/Data/Tooltip2.lua",
            ]
        );
        for slot in SLOTS {
            let stub = sv::parse(&slot.stub()).unwrap();
            assert_eq!(stub.len(), 1);
            assert_eq!(stub[0].0, slot.global());
        }
    }
}
