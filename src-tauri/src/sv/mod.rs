//! SavedVariables: parsed as data, never executed. See docs/specs/core-fs.md §3.

mod parse;
mod value;

pub use parse::{parse, parse_value, ParseError, MAX_DEPTH};
pub use value::{write_globals, Globals, LuaTable, LuaValue};
