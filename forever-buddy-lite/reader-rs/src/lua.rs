//! A parser for the subset of Lua the WoW client writes to SavedVariables files:
//!
//! ```text
//! Name = { ["key"] = value, [1] = value, value, ... }
//! ```
//!
//! with strings, numbers, booleans and nil, `--` comments and `--[[ ]]` blocks.
//!
//! Strings are kept as bytes, as Lua does. That matters for Auctionator, which stores each realm's
//! prices as a CBOR blob inside a Lua string (`\ddd` escapes for the non-printable bytes): the bytes
//! go to the CBOR decoder untouched. Everything else is text and is read as UTF-8 at the JSON boundary.

use crate::json;
use serde_json::Value as Json;
use std::fmt;

/// A parsed Lua value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    /// Raw bytes: Lua strings aren't necessarily text.
    Str(Vec<u8>),
    Table(Table),
}

/// A Lua table, its entries in source order. A repeated key replaces the value in place.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub entries: Vec<(Key, Value)>,
    /// Where each key sits in `entries` (big tables, like a full auction scan, have thousands).
    position: std::collections::HashMap<(u8, String), usize>,
}

/// A table key. The client writes `["name"]`, `[3]` or a bare `name`.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    Number(f64),
    Str(Vec<u8>),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
    pub context: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at offset {}: {:?}", self.message, self.offset, self.context)
    }
}

impl std::error::Error for ParseError {}

impl Table {
    /// The value under a string key, if any.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| matches!(k, Key::Str(s) if s == key.as_bytes())).map(|(_, v)| v)
    }

    fn set(&mut self, key: Key, value: Value) {
        let kind = match key {
            Key::Number(_) => 0,
            Key::Str(_) => 1,
            Key::Bool(_) => 2,
        };
        let id = (kind, key.to_property());
        match self.position.get(&id) {
            Some(&at) => self.entries[at].1 = value,
            None => {
                self.position.insert(id, self.entries.len());
                self.entries.push((key, value));
            }
        }
    }
}

impl Key {
    /// The key as a JSON property ("16", "-1", "name").
    pub fn to_property(&self) -> String {
        match self {
            Key::Number(n) => json::number_key(*n),
            Key::Str(s) => String::from_utf8_lossy(s).into_owned(),
            Key::Bool(b) => b.to_string(),
        }
    }
}

impl Value {
    /// The value as JSON: tables keyed exactly 1..n become arrays, others objects; strings are read
    /// as UTF-8 (invalid bytes become U+FFFD); nil becomes null.
    pub fn to_json(&self) -> Json {
        match self {
            Value::Nil => Json::Null,
            Value::Bool(b) => Json::Bool(*b),
            Value::Number(n) => json::number(*n),
            Value::Str(s) => Json::String(String::from_utf8_lossy(s).into_owned()),
            Value::Table(t) => t.to_json(),
        }
    }
}

impl Table {
    pub fn to_json(&self) -> Json {
        let entries = self.entries.iter().map(|(k, v)| (k.to_property(), v.to_json()));
        // Only number keys make a list: `["1"] = x` is a string key.
        if self.entries.iter().all(|(k, _)| matches!(k, Key::Number(_))) {
            json::table(entries)
        } else {
            Json::Object(entries.collect())
        }
    }
}

/// Parses a whole SavedVariables file: its top-level `Name = value` assignments, in order.
pub fn parse_saved_variables(src: &[u8]) -> Result<Vec<(String, Value)>, ParseError> {
    let mut p = Parser { src, i: 0 };
    let mut out: Vec<(String, Value)> = Vec::new();
    loop {
        p.skip_space_and_comments();
        if p.at_end() {
            break;
        }
        let name = p.identifier().ok_or_else(|| p.error("expected a name"))?;
        p.skip_space_and_comments();
        p.expect(b'=')?;
        let value = p.value()?;
        match out.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 = value,
            None => out.push((name, value)),
        }
    }
    Ok(out)
}

/// Parses a file and returns one top-level variable, if the file assigns it.
pub fn global(src: &[u8], name: &str) -> Result<Option<Value>, ParseError> {
    Ok(parse_saved_variables(src)?.into_iter().find(|(n, _)| n == name).map(|(_, v)| v))
}

struct Parser<'a> {
    src: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn at_end(&self) -> bool {
        self.i >= self.src.len()
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.i).copied()
    }

    fn starts_with(&self, s: &[u8]) -> bool {
        self.src[self.i.min(self.src.len())..].starts_with(s)
    }

    fn error(&self, message: &str) -> ParseError {
        let end = (self.i + 40).min(self.src.len());
        ParseError {
            message: message.to_string(),
            offset: self.i,
            context: String::from_utf8_lossy(&self.src[self.i.min(end)..end]).into_owned(),
        }
    }

    fn expect(&mut self, b: u8) -> Result<(), ParseError> {
        if self.peek() != Some(b) {
            return Err(self.error(&format!("expected {}", b as char)));
        }
        self.i += 1;
        Ok(())
    }

    /// Whitespace, `-- line comments` and `--[[ block comments ]]`. The client writes `-- [3]`
    /// after array entries.
    fn skip_space_and_comments(&mut self) {
        loop {
            while self.peek().is_some_and(|c| c.is_ascii_whitespace() || c == 0x0b) {
                self.i += 1;
            }
            if self.starts_with(b"--[[") {
                self.i = find(self.src, b"]]", self.i).map_or(self.src.len(), |end| end + 2);
            } else if self.starts_with(b"--") {
                while self.peek().is_some_and(|c| c != b'\n') {
                    self.i += 1;
                }
            } else {
                return;
            }
        }
    }

    fn identifier(&mut self) -> Option<String> {
        let start = self.i;
        if !self.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') {
            return None;
        }
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            self.i += 1;
        }
        Some(String::from_utf8_lossy(&self.src[start..self.i]).into_owned())
    }

    fn value(&mut self) -> Result<Value, ParseError> {
        self.skip_space_and_comments();
        match self.peek() {
            Some(b'{') => self.table().map(Value::Table),
            Some(b'"') | Some(b'\'') => Ok(Value::Str(self.quoted_string())),
            Some(b'[') if self.starts_with(b"[[") => self.long_string().map(Value::Str),
            _ => self.scalar(),
        }
    }

    /// A quoted string with Lua's escapes. `\ddd` is a byte (up to three decimal digits); a backslash
    /// before a newline keeps the newline; any other escaped character stands for itself.
    fn quoted_string(&mut self) -> Vec<u8> {
        let quote = self.src[self.i];
        self.i += 1;
        let mut s = Vec::new();
        while let Some(c) = self.peek() {
            if c == quote {
                break;
            }
            if c != b'\\' {
                s.push(c);
                self.i += 1;
                continue;
            }
            self.i += 1;
            let Some(e) = self.peek() else { break };
            if e.is_ascii_digit() {
                let mut code: u32 = 0;
                let mut digits = 0;
                while digits < 3 && self.peek().is_some_and(|d| d.is_ascii_digit()) {
                    code = code * 10 + (self.src[self.i] - b'0') as u32;
                    self.i += 1;
                    digits += 1;
                }
                // Lua rejects codes over 255; keep the low byte rather than fail on one bad escape.
                s.push((code & 0xff) as u8);
                continue;
            }
            s.push(match e {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                other => other, // \\, \", \', and a backslash-newline
            });
            self.i += 1;
        }
        self.i += 1; // the closing quote
        s
    }

    fn long_string(&mut self) -> Result<Vec<u8>, ParseError> {
        let end = find(self.src, b"]]", self.i + 2).ok_or_else(|| self.error("unterminated [[string"))?;
        let s = self.src[self.i + 2..end].to_vec();
        self.i = end + 2;
        Ok(s)
    }

    /// true, false, nil or a number: decimal, float, exponent, hex, or the client's inf/nan.
    fn scalar(&mut self) -> Result<Value, ParseError> {
        for (word, value) in [("true", Value::Bool(true)), ("false", Value::Bool(false)), ("nil", Value::Nil)] {
            if self.starts_with(word.as_bytes()) {
                self.i += word.len();
                return Ok(value);
            }
        }
        let start = self.i;
        let negative = self.peek() == Some(b'-');
        if negative {
            self.i += 1;
        }
        let magnitude = self.unsigned_number().ok_or_else(|| {
            self.i = start;
            self.error("unexpected value")
        })?;
        Ok(Value::Number(if negative { -magnitude } else { magnitude }))
    }

    fn unsigned_number(&mut self) -> Option<f64> {
        let rest = &self.src[self.i..];
        if rest.starts_with(b"inf") {
            self.i += 3;
            return Some(f64::INFINITY);
        }
        if rest.starts_with(b"nan") {
            self.i += 3;
            return Some(f64::NAN);
        }
        if rest.starts_with(b"0x") || rest.starts_with(b"0X") {
            let digits = rest[2..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            if digits > 0 {
                let text = std::str::from_utf8(&rest[2..2 + digits]).ok()?;
                self.i += 2 + digits;
                return Some(u128::from_str_radix(text, 16).map(|n| n as f64).unwrap_or(f64::INFINITY));
            }
        }
        // digits [. digits] [e [+-] digits], or . digits
        let mut n = 0;
        let int_digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
        n += int_digits;
        if rest.get(n) == Some(&b'.') {
            let frac = rest[n + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
            if int_digits == 0 && frac == 0 {
                return None;
            }
            n += 1 + frac;
        } else if int_digits == 0 {
            return None;
        }
        if int_digits > 0 && matches!(rest.get(n), Some(b'e') | Some(b'E')) {
            let mut m = n + 1;
            if matches!(rest.get(m), Some(b'+') | Some(b'-')) {
                m += 1;
            }
            let exp = rest[m..].iter().take_while(|c| c.is_ascii_digit()).count();
            if exp > 0 {
                n = m + exp;
            }
        }
        let text = std::str::from_utf8(&rest[..n]).ok()?;
        self.i += n;
        let text = text.strip_suffix('.').unwrap_or(text);
        let text = if text.starts_with('.') { format!("0{text}") } else { text.to_string() };
        text.parse::<f64>().ok()
    }

    /// `{ [key] = value, name = value, value, ... }`. Positional values take keys 1, 2, ... in order.
    /// A nil value isn't stored (Lua drops it too).
    fn table(&mut self) -> Result<Table, ParseError> {
        self.i += 1; // {
        let mut table = Table::default();
        let mut next_index = 1.0;
        loop {
            self.skip_space_and_comments();
            match self.peek() {
                None => return Err(self.error("unterminated table")),
                Some(b'}') => {
                    self.i += 1;
                    return Ok(table);
                }
                _ => {}
            }
            let key = self.explicit_key()?;
            let value = self.value()?;
            let key = match key {
                Some(k) => k,
                None => {
                    let k = Key::Number(next_index);
                    next_index += 1.0;
                    k
                }
            };
            if value != Value::Nil {
                table.set(key, value);
            }
            self.skip_space_and_comments();
            if matches!(self.peek(), Some(b',') | Some(b';')) {
                self.i += 1;
            }
        }
    }

    /// `[expr] =` or `name =` before a value; None for a positional value.
    fn explicit_key(&mut self) -> Result<Option<Key>, ParseError> {
        if self.peek() == Some(b'[') && !self.starts_with(b"[[") {
            self.i += 1;
            let key = match self.value()? {
                Value::Number(n) => Key::Number(n),
                Value::Str(s) => Key::Str(s),
                Value::Bool(b) => Key::Bool(b),
                _ => return Err(self.error("unsupported table key")),
            };
            self.skip_space_and_comments();
            self.expect(b']')?;
            self.skip_space_and_comments();
            self.expect(b'=')?;
            return Ok(Some(key));
        }
        // A bare name followed by `=` (but not `==`).
        let start = self.i;
        if let Some(name) = self.identifier() {
            while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                self.i += 1;
            }
            if self.peek() == Some(b'=') && self.src.get(self.i + 1) != Some(&b'=') {
                self.i += 1;
                return Ok(Some(Key::Str(name.into_bytes())));
            }
        }
        self.i = start;
        Ok(None)
    }
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_json(src: &str) -> Json {
        let vars = parse_saved_variables(src.as_bytes()).unwrap();
        Json::Object(vars.into_iter().map(|(k, v)| (k, v.to_json())).collect())
    }

    #[test]
    fn parses_what_the_client_writes() {
        // Escapes, an explicit index, a nested list with a comment, two globals.
        let db = parse_json("X = {\n[\"a\"] = \"q\\\"uote\\\\\",\n[3] = true,\n{ 1, 2, -- [2]\n},\n}\nY = 5");
        assert_eq!(db["X"]["a"], "q\"uote\\");
        assert_eq!(db["X"]["3"], true);
        assert_eq!(db["X"]["1"], json!([1, 2]));
        assert_eq!(db["Y"], 5);
    }

    #[test]
    fn string_escapes() {
        let db = parse_json(
            r#"S = { "a\nb", 'it\'s', "tab\tx", "\65\066\0671", "back\\slash", "line\
two" }"#,
        );
        assert_eq!(db["S"], json!(["a\nb", "it's", "tab\tx", "ABC1", "back\\slash", "line\ntwo"]));
    }

    #[test]
    fn decimal_escapes_are_bytes() {
        let vars = parse_saved_variables(b"B = \"\\0\\255\\195\\169\"").unwrap();
        assert_eq!(vars[0].1, Value::Str(vec![0, 255, 195, 169]));
        // UTF-8 bytes read back as text.
        let v = parse_saved_variables(b"T = \"caf\\195\\169\"").unwrap();
        assert_eq!(v[0].1.to_json(), json!("café"));
    }

    #[test]
    fn numbers() {
        let db = parse_json("N = { -5, 1.5, -0.25, 1e3, 2.5E-1, 0x1F, .5, 7., -inf, 10 }");
        assert_eq!(db["N"][0], -5);
        assert_eq!(db["N"][1], 1.5);
        assert_eq!(db["N"][2], -0.25);
        assert_eq!(db["N"][3], 1000);
        assert_eq!(db["N"][4], 0.25);
        assert_eq!(db["N"][5], 31);
        assert_eq!(db["N"][6], 0.5);
        assert_eq!(db["N"][7], 7);
        assert_eq!(db["N"][8], Json::Null); // no JSON for infinity
        assert_eq!(db["N"][9], 10);
    }

    #[test]
    fn nested_tables_and_key_kinds() {
        let db = parse_json(
            "DB = {\n\t[\"chars\"] = {\n\t\t[\"A-Realm\"] = { [\"bags\"] = { [0] = { [\"free\"] = 3 }, [-1] = {}, [2] = { [\"free\"] = 1 } } },\n\t},\n\tbare = false,\n\t[true] = 1,\n}",
        );
        let bags = &db["DB"]["chars"]["A-Realm"]["bags"];
        // Keys in the order written.
        assert_eq!(bags.as_object().unwrap().keys().collect::<Vec<_>>(), ["0", "-1", "2"]);
        assert_eq!(bags["0"]["free"], 3);
        assert_eq!(bags["-1"], json!({}));
        assert_eq!(db["DB"]["bare"], false);
        assert_eq!(db["DB"]["true"], 1);
    }

    #[test]
    fn sequences_nils_and_empty_tables() {
        let db = parse_json("A = { [2] = \"b\", [1] = \"a\" }\nB = { nil, 2 }\nC = {}\nD = nil\nE = { [1] = 1, x = 2 }");
        assert_eq!(db["A"], json!(["a", "b"]));
        assert_eq!(db["B"], json!({"2": 2})); // the nil at 1 isn't stored, so it's no sequence
        assert_eq!(db["C"], json!({}));
        assert_eq!(db["D"], Json::Null);
        assert_eq!(db["E"], json!({"1": 1, "x": 2}));
    }

    #[test]
    fn comments_long_strings_and_semicolons() {
        let db = parse_json("--[[ header\n]] X = { [[long\nstring]]; 'b' } -- trailing\n");
        assert_eq!(db["X"], json!(["long\nstring", "b"]));
    }

    #[test]
    fn errors_say_where() {
        let e = parse_saved_variables(b"X = { @ }").unwrap_err();
        assert_eq!(e.message, "unexpected value");
        assert_eq!(e.offset, 6);
        assert!(parse_saved_variables(b"= 5").is_err());
        assert!(parse_saved_variables(b"X = { 1, 2").is_err());
    }
}
