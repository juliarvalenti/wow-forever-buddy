//! The bridge, app → addon (docs/specs/bridge-v0.4.md §2): fixed data files
//! ("slots") inside our own addon folder that the addon loads at login or
//! `/reload`.
//!
//! WoW runs these files as Lua, so the app only ever writes data: one global
//! table of strings, numbers and booleans, serialized by `sv::write_globals`
//! and parsed back with the data-only `sv` parser before every write
//! (`check`). The list of slots is a constant; no path ever comes from the
//! UI or an agent. Writing goes through `AppCore::send_to_game`, which holds
//! the jobs lock around `WriteGate::write_slots`.
//!
//! The first producer is the tooltip index (`tooltip`, spec §5).

pub mod briefing;
pub mod tooltip;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::addon;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::fsx::read::safe_read;
use crate::fsx::relpath::RelPath;
use crate::game::gate::{MutationTarget, WriteGate};
use crate::state::AppCore;
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
    /// Sent quest plans, one per character at most (P1, INGAME §7).
    Plan,
    /// The login briefing's app-side facts: alts' waiting mail and each
    /// character's login note (B1, INGAME §9).
    Briefing,
}

pub const SLOTS: [Slot; 4] = [Slot::Tooltip1, Slot::Tooltip2, Slot::Plan, Slot::Briefing];

impl Slot {
    pub fn name(self) -> &'static str {
        match self {
            Slot::Tooltip1 => "Tooltip1",
            Slot::Tooltip2 => "Tooltip2",
            Slot::Plan => "Plan",
            Slot::Briefing => "Briefing",
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

    /// The file the addon ships before the app has written anything (the
    /// bundled `Data/*.lua`; tests check they match).
    #[cfg(test)]
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
    render_capped(slot, body)?.ok_or_else(|| refused(slot, "over the 1 MB cap"))
}

/// `render`, or `None` when the file would be over the cap, so a producer
/// can send a "too large" notice instead of cutting the data short.
pub fn render_capped(slot: Slot, body: LuaTable) -> AppResult<Option<Vec<u8>>> {
    let value = LuaValue::Table(Box::new(body));
    let bytes = sv::write_globals(&[(slot.global(), value.clone())]);
    if bytes.len() > MAX_SLOT_BYTES {
        return Ok(None);
    }
    let parsed = check(slot, &bytes)?;
    if parsed != value {
        return Err(refused(slot, "doesn't read back as written"));
    }
    Ok(Some(bytes))
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

/// Where a character's data in a slot is (bridge spec §4, `bridge.html`'s
/// states): the app's "Sent to the game" line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Delivery {
    /// Not written yet: changed while WoW was running, so it goes after WoW
    /// closes.
    Waiting,
    /// Written (RFC 3339), and this character's addon hasn't loaded it yet:
    /// it shows after a /reload or the next login.
    Pending { written_at: String },
    /// This character's addon loaded it at `since` (RFC 3339).
    Synced { since: String },
    /// The installed addon doesn't list the slot: update it, then restart
    /// WoW once.
    Restart,
    /// The last write failed or was refused; the game keeps the last file.
    Failed,
}

/// `slot`'s delivery to `character_id`, for content that last changed at
/// `changed_at` (RFC 3339). `listed`: whether the installed TOC lists it.
pub fn delivery(
    db: &Db,
    flavor: &str,
    slot: Slot,
    character_id: u32,
    changed_at: &str,
    listed: bool,
) -> AppResult<Delivery> {
    if !listed {
        return Ok(Delivery::Restart);
    }
    db.with_conn(|c| {
        let last: Option<(Option<i64>, Option<String>, String)> = c
            .query_row(
                "SELECT stamp, written_at, status FROM bridge_slots WHERE flavor = ?1 AND slot = ?2",
                params![flavor, slot.name()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((stamp, written_at, status)) = last else {
            return Ok(Delivery::Waiting);
        };
        if status == "failed" || status == "refused" {
            return Ok(Delivery::Failed);
        }
        let written_at = written_at.unwrap_or_default();
        // Changed since the last write (e.g. while WoW ran): not out yet.
        if written_at.as_str() < changed_at {
            return Ok(Delivery::Waiting);
        }
        let receipt: Option<(Option<i64>, i64)> = c
            .query_row(
                "SELECT stamp, seen_at FROM bridge_receipts WHERE character_id = ?1 AND slot = ?2",
                params![character_id, slot.name()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(match (receipt, stamp) {
            (Some((Some(seen_stamp), seen_at)), Some(stamp)) if seen_stamp >= stamp => Delivery::Synced {
                since: chrono::DateTime::from_timestamp(seen_at, 0)
                    .unwrap_or_default()
                    .to_rfc3339(),
            },
            _ => Delivery::Pending { written_at },
        })
    })
}

/// What `send_to_game` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// New data in every slot.
    Written,
    /// Over the cap: every slot now says so instead of holding cut-short
    /// data (spec §5).
    TooLarge,
    /// Nothing changed since the last write.
    Unchanged,
    /// The installed addon lists none of the slots (not installed, or older
    /// than 0.4.0), so the game wouldn't load them.
    NoAddon,
    /// WoW is running; the next ingest after it exits sends it.
    Waiting,
}

impl AppCore {
    /// Regenerates every slot and writes the ones that changed. Holds the
    /// jobs lock, so it never races a restore or an addon install; this is
    /// the only way slots get written.
    pub fn send_to_game(&self) -> AppResult<Sent> {
        let _job = self.jobs.lock().expect("job lock poisoned");
        self.send_to_game_locked()
    }

    /// `send_to_game` for a caller that already holds `jobs` (addon install).
    pub fn send_to_game_locked(&self) -> AppResult<Sent> {
        let game = self.active_game()?;
        let target = self.mutation_target()?;
        send(
            &self.db,
            &self.write_gate()?,
            &target,
            &game.flavor,
            chrono::Utc::now().timestamp(),
        )
    }
}

/// Builds the slots from the db and writes them if anything changed.
pub(crate) fn send(
    db: &Db,
    gate: &WriteGate,
    target: &MutationTarget,
    flavor: &str,
    stamp: i64,
) -> AppResult<Sent> {
    // Only the slots the installed addon's TOC lists: the game wouldn't load
    // the others, and an older addon keeps getting the ones it knows.
    let listed = addon::listed_slots(&target.game);
    let has = |s: Slot| listed.contains(&s);
    if listed.is_empty() {
        return Ok(Sent::NoAddon);
    }
    let mut slots = Vec::new();
    let mut too_large = false;
    if has(Slot::Tooltip1) && has(Slot::Tooltip2) {
        let built = tooltip::build(db, flavor, stamp)?;
        too_large = built.too_large;
        slots.extend(built.slots);
    }
    if has(Slot::Plan) {
        let mut body = header(stamp);
        body.hash.extend(crate::plans::slot_entries(db, flavor)?);
        slots.push((Slot::Plan, render(Slot::Plan, body)?));
    }
    if has(Slot::Briefing) {
        slots.push((Slot::Briefing, briefing::build(db, flavor, stamp)?));
    }
    let built = tooltip::Built { slots, too_large };
    if built
        .slots
        .iter()
        .all(|(slot, bytes)| same_on_disk(target, *slot, bytes))
    {
        return Ok(Sent::Unchanged);
    }
    match gate.write_slots(target, &built.slots) {
        Ok(()) => {}
        Err(AppError::GameRunning(_)) => return Ok(Sent::Waiting),
        Err(e) => {
            let status = if matches!(e, AppError::SlotRefused(_)) {
                "refused"
            } else {
                "failed"
            };
            record(
                db,
                flavor,
                &built.slots,
                stamp,
                status,
                Some(&e.to_string()),
            )?;
            return Err(e);
        }
    }
    let (status, sent) = if built.too_large {
        ("too_large", Sent::TooLarge)
    } else {
        ("written", Sent::Written)
    };
    record(db, flavor, &built.slots, stamp, status, None)?;
    Ok(sent)
}

/// Whether the slot's file already holds this data (its stamp aside).
fn same_on_disk(target: &MutationTarget, slot: Slot, bytes: &[u8]) -> bool {
    let old = slot
        .path()
        .resolve(&target.game)
        .ok()
        .and_then(|p| safe_read(&p).ok())
        .and_then(|b| check(slot, &b).ok());
    match (old, check(slot, bytes)) {
        (Some(old), Ok(new)) => without_stamp(old) == without_stamp(new),
        _ => false,
    }
}

fn without_stamp(v: LuaValue) -> LuaValue {
    match v {
        LuaValue::Table(mut t) => {
            t.hash
                .retain(|(k, _)| k.as_bytes() != Some(b"stamp".as_slice()));
            LuaValue::Table(t)
        }
        other => other,
    }
}

/// Notes the last write of each slot, for the app's "Sent to the game" panel.
fn record(
    db: &Db,
    flavor: &str,
    slots: &[(Slot, Vec<u8>)],
    stamp: i64,
    status: &str,
    error: Option<&str>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|c| {
        for (slot, bytes) in slots {
            c.execute(
                "INSERT OR REPLACE INTO bridge_slots
                   (flavor, slot, stamp, written_at, bytes, status, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    flavor,
                    slot.name(),
                    stamp,
                    now,
                    bytes.len() as i64,
                    status,
                    error
                ],
            )?;
        }
        Ok(())
    })
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

    /// The "Sent to the game" states, as the plan panel shows them.
    #[test]
    fn delivery_states() {
        let db = Db::open_in_memory().unwrap();
        let flavor = "_classic_beta_";
        let changed = "2026-10-06T21:02:00+00:00";
        let state = || delivery(&db, flavor, Slot::Plan, 1, changed, true).unwrap();
        assert_eq!(
            delivery(&db, flavor, Slot::Plan, 1, changed, false).unwrap(),
            Delivery::Restart,
            "an addon without the slot"
        );
        assert_eq!(state(), Delivery::Waiting, "never written");
        let write = |stamp: i64, at: &str, status: &str| {
            db.with_conn(|c| {
                c.execute(
                    "INSERT OR REPLACE INTO bridge_slots (flavor, slot, stamp, written_at, bytes, status)
                     VALUES (?1, 'Plan', ?2, ?3, 10, ?4)",
                    params![flavor, stamp, at, status],
                )?;
                Ok(())
            })
            .unwrap()
        };
        write(100, "2026-10-06T21:00:00+00:00", "written");
        assert_eq!(state(), Delivery::Waiting, "written before the change");
        write(200, "2026-10-06T21:04:00+00:00", "written");
        assert!(
            matches!(state(), Delivery::Pending { .. }),
            "out, not loaded yet"
        );
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO characters (id, flavor, account, group_dir, char_dir, name, first_seen, last_seen)
                 VALUES (1, ?1, 'A', '70', 'T', 'T', 1, 1)",
                [flavor],
            )?;
            c.execute(
                "INSERT INTO bridge_receipts (character_id, slot, stamp, schema, seen_at)
                 VALUES (1, 'Plan', 200, 1, 1791400000)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        assert!(matches!(state(), Delivery::Synced { .. }), "loaded");
        write(300, "2026-10-06T21:10:00+00:00", "failed");
        assert_eq!(state(), Delivery::Failed);
    }

    #[test]
    fn slots_are_constant_files_in_our_data_folder() {
        assert_eq!(
            SLOTS.map(|s| s.path().as_string()),
            [
                "Interface/AddOns/ForeverBuddy/Data/Tooltip1.lua",
                "Interface/AddOns/ForeverBuddy/Data/Tooltip2.lua",
                "Interface/AddOns/ForeverBuddy/Data/Plan.lua",
                "Interface/AddOns/ForeverBuddy/Data/Briefing.lua",
            ]
        );
        for slot in SLOTS {
            let stub = sv::parse(&slot.stub()).unwrap();
            assert_eq!(stub.len(), 1);
            assert_eq!(stub[0].0, slot.global());
        }
    }
}
