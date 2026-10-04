//! SavedVariables: parsed as data, never executed. See docs/specs/core-fs.md §3.
//!
//! # Torn reads: the contract for consumers
//!
//! WoW rewrites SavedVariables files on logout and reload, so a read can
//! catch a file mid-write. What the parser guarantees, and what it can't:
//!
//! - **A cut inside a statement is always an error.** That covers inside a
//!   table, inside a string, and a trailing scalar: WoW ends every statement
//!   with a newline, so `Version = 31` torn to `Version = 3` (no newline) is
//!   rejected as truncated. A parse never returns a wrong value.
//! - **A cut exactly between two statements can't be seen in the bytes.** It
//!   parses as the leading whole statements, with the later globals missing.
//!
//! So consumers must not treat "parsed OK" as "complete". The layers that
//! cover the gap:
//!
//! 1. Only read through `fsx::read::safe_read`, whose stat → read → stat
//!    check rejects a file that changed while it was read.
//! 2. Only read files the debounced WTF watcher reports as settled (size and
//!    mtime unchanged for 2 s).
//! 3. For our own data (v0.2), the companion addon declares a sentinel
//!    SavedVariable last in its TOC (e.g. `ForeverBuddy_EOF = <count or
//!    checksum>`). WoW writes variables in TOC order, so ingest requires the
//!    sentinel, and that it matches, before trusting the file.
//! 4. A `ParseError` means "skip this cycle and retry later", never act on it
//!    (spec §3). Nothing destructive depends on a parse result.
//!
//! # Lua semantics for lookups
//!
//! The model is syntactic (source order, duplicates kept). Use
//! [`LuaTable::get`] and [`LuaTable::get_index`] to see what the game sees:
//! last duplicate key wins, and since Lua 5.1 numbers are all doubles,
//! `Int(2)`, `Num(2.0)` and the second positional field are the same slot.

mod parse;
mod value;

pub use parse::{parse, parse_value, ParseError, MAX_DEPTH};
pub use value::{write_globals, Globals, LuaTable, LuaValue};
