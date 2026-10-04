//! Data model for parsed SavedVariables.
//!
//! The model is syntactic: positional fields go in `array`, keyed fields
//! (`[k] = v` and `name = v`) go in `hash`, both in source order. Nothing is
//! deduplicated or normalized, so output is deterministic and round-trips.

use std::fmt;

/// A Lua value as it appears in a SavedVariables file.
///
/// Strings stay as bytes: WoW does not guarantee UTF-8. Decode lossily at
/// display time with [`LuaValue::to_string_lossy`].
#[derive(Clone, Debug)]
pub enum LuaValue {
    Nil,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(Box<[u8]>),
    Table(Box<LuaTable>),
}

/// A table constructor's fields, in source order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LuaTable {
    /// Positional fields (`{ "a", "b" }`).
    pub array: Vec<LuaValue>,
    /// Keyed fields (`{ ["k"] = v, name = v }`); `name = v` keys are `Str`.
    pub hash: Vec<(LuaValue, LuaValue)>,
}

/// The top-level `Name = value` assignments of a file, in source order.
pub type Globals = Vec<(String, LuaValue)>;

impl LuaValue {
    pub fn str(s: impl AsRef<[u8]>) -> Self {
        LuaValue::Str(s.as_ref().into())
    }

    pub fn as_table(&self) -> Option<&LuaTable> {
        match self {
            LuaValue::Table(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            LuaValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Numeric value of `Int` or `Num`.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            LuaValue::Int(i) => Some(i as f64),
            LuaValue::Num(n) => Some(n),
            _ => None,
        }
    }

    /// Display-time decoding of a string value.
    pub fn to_string_lossy(&self) -> Option<String> {
        self.as_bytes()
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
}

impl LuaTable {
    /// The value Lua sees at string key `key`. With duplicate keys
    /// (`{ a = 1, a = 2 }`), the last one wins, as in Lua.
    pub fn get(&self, key: &str) -> Option<&LuaValue> {
        self.hash
            .iter()
            .rev()
            .find(|(k, _)| k.as_bytes() == Some(key.as_bytes()))
            .map(|(_, v)| v)
    }

    /// The value Lua sees at integer index `i` (1-based). WoW runs Lua 5.1,
    /// where every number is a double, so `[2]`, `[2.0]` and the second
    /// positional field are all the same slot. Positional fields are stored
    /// after keyed ones by the constructor, so they win; among keyed fields
    /// the last wins.
    pub fn get_index(&self, i: i64) -> Option<&LuaValue> {
        if let Some(v) = usize::try_from(i - 1).ok().and_then(|n| self.array.get(n)) {
            return Some(v);
        }
        self.hash
            .iter()
            .rev()
            .find(|(k, _)| match *k {
                LuaValue::Int(k) => k == i,
                LuaValue::Num(k) => k == i as f64,
                _ => false,
            })
            .map(|(_, v)| v)
    }
}

/// Structural equality. Unlike `f64`, two NaNs compare equal and `0.0` and
/// `-0.0` do not, so a parse → serialize → parse round-trip can be checked
/// with `==`.
impl PartialEq for LuaValue {
    fn eq(&self, other: &Self) -> bool {
        use LuaValue::*;
        match (self, other) {
            (Nil, Nil) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Num(a), Num(b)) => a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
            (Str(a), Str(b)) => a == b,
            (Table(a), Table(b)) => a == b,
            _ => false,
        }
    }
}

/// Serialize to Lua source in the shape WoW writes (tab indented, trailing
/// commas). Output parses back to an equal value.
pub fn write_globals(globals: &[(String, LuaValue)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, value) in globals {
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b" = ");
        write_value(&mut out, value, 0);
        out.push(b'\n');
    }
    out
}

fn write_value(out: &mut Vec<u8>, v: &LuaValue, indent: usize) {
    match v {
        LuaValue::Nil => out.extend_from_slice(b"nil"),
        LuaValue::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        LuaValue::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        LuaValue::Num(n) => write_num(out, *n),
        LuaValue::Str(s) => write_str(out, s),
        LuaValue::Table(t) => {
            out.extend_from_slice(b"{\n");
            for item in &t.array {
                tabs(out, indent + 1);
                write_value(out, item, indent + 1);
                out.extend_from_slice(b",\n");
            }
            for (k, item) in &t.hash {
                tabs(out, indent + 1);
                out.push(b'[');
                write_value(out, k, indent + 1);
                out.extend_from_slice(b"] = ");
                write_value(out, item, indent + 1);
                out.extend_from_slice(b",\n");
            }
            tabs(out, indent);
            out.push(b'}');
        }
    }
}

fn tabs(out: &mut Vec<u8>, n: usize) {
    out.extend(std::iter::repeat_n(b'\t', n));
}

fn write_num(out: &mut Vec<u8>, n: f64) {
    if n.is_nan() {
        out.extend_from_slice(b"0/0");
    } else if n.is_infinite() {
        out.extend_from_slice(if n > 0.0 { b"1/0" } else { b"-1/0" });
    } else {
        // `{:?}` is the shortest exact representation and always contains
        // a `.` or an exponent, so it parses back as `Num`, not `Int`.
        out.extend_from_slice(format!("{n:?}").as_bytes());
    }
}

fn write_str(out: &mut Vec<u8>, s: &[u8]) {
    out.push(b'"');
    for &b in s {
        match b {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            // Always three digits so a following digit isn't absorbed.
            0..=31 | 127 => out.extend_from_slice(format!("\\{b:03}").as_bytes()),
            _ => out.push(b),
        }
    }
    out.push(b'"');
}

impl fmt::Display for LuaValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = Vec::new();
        write_value(&mut out, self, 0);
        f.write_str(&String::from_utf8_lossy(&out))
    }
}
