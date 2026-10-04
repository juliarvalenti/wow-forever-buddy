//! Recursive-descent parser for the SavedVariables subset of Lua.
//!
//! WoW writes `Name = <expr>` statements where expressions are table
//! constructors, strings, numbers, booleans and `nil`. That is all we accept:
//! anything else (calls, operators, identifiers as values) is a parse error
//! with a line and column. Lua is never executed.

use super::value::{Globals, LuaTable, LuaValue};
use memchr::{memchr, memchr3};
use std::fmt;

/// Deeper nesting than this is rejected rather than risking the stack.
/// Real addon data nests a few dozen levels at most.
pub const MAX_DEPTH: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line.
    pub line: u32,
    /// 1-based column, in bytes.
    pub col: u32,
    pub msg: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, col {}: {}", self.line, self.col, self.msg)
    }
}

impl std::error::Error for ParseError {}

type R<T> = Result<T, ParseError>;

/// Parse a whole SavedVariables file.
pub fn parse(src: &[u8]) -> R<Globals> {
    Parser::new(src).globals()
}

/// Parse a single expression, e.g. `{ 1, 2 }`.
pub fn parse_value(src: &[u8]) -> R<LuaValue> {
    let mut p = Parser::new(src);
    p.skip_trivia()?;
    let v = p.value()?;
    p.skip_trivia()?;
    match p.peek() {
        None => Ok(v),
        Some(_) => p.err("unexpected input after value"),
    }
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    depth: usize,
    // Shared scratch stacks for table fields. Each table drains its own
    // fields off the top into exactly sized Vecs, which keeps memory close to
    // the data instead of paying for Vec growth slack in every table.
    arr_stack: Vec<LuaValue>,
    hash_stack: Vec<(LuaValue, LuaValue)>,
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

impl<'a> Parser<'a> {
    fn new(src: &'a [u8]) -> Self {
        let pos = if src.starts_with(b"\xEF\xBB\xBF") {
            3
        } else {
            0
        };
        Parser {
            src,
            pos,
            depth: 0,
            arr_stack: Vec::new(),
            hash_stack: Vec::new(),
        }
    }

    fn error_at(&self, pos: usize, msg: impl Into<String>) -> ParseError {
        let before = &self.src[..pos.min(self.src.len())];
        let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
        let line_start = before
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        ParseError {
            line: line as u32,
            col: (before.len() - line_start + 1) as u32,
            msg: msg.into(),
        }
    }

    fn err<T>(&self, msg: impl Into<String>) -> R<T> {
        Err(self.error_at(self.pos, msg))
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, off: usize) -> Option<u8> {
        self.src.get(self.pos + off).copied()
    }

    fn expect(&mut self, c: u8) -> R<()> {
        match self.peek() {
            Some(got) if got == c => {
                self.pos += 1;
                Ok(())
            }
            Some(got) => self.err(format!(
                "expected `{}`, found `{}`",
                c as char,
                got.escape_ascii()
            )),
            None => self.err(format!("expected `{}`, found end of file", c as char)),
        }
    }

    fn ident(&mut self) -> &'a [u8] {
        let start = self.pos;
        while self.peek().is_some_and(is_ident_char) {
            self.pos += 1;
        }
        &self.src[start..self.pos]
    }

    fn digits(&mut self, pred: fn(&u8) -> bool) {
        while self.peek().as_ref().is_some_and(pred) {
            self.pos += 1;
        }
    }

    /// Skips whitespace and comments.
    fn skip_trivia(&mut self) -> R<()> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) => self.pos += 1,
                Some(b'-') if self.peek_at(1) == Some(b'-') => {
                    let start = self.pos;
                    self.pos += 2;
                    if let Some(level) = self.long_bracket_open() {
                        self.long_bracket_body(start, level)?;
                    } else {
                        match memchr(b'\n', &self.src[self.pos..]) {
                            Some(i) => self.pos += i + 1,
                            None => self.pos = self.src.len(),
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    /// At `[`, consumes `[` `=`* `[` and returns the level, or consumes
    /// nothing.
    fn long_bracket_open(&mut self) -> Option<usize> {
        if self.peek() != Some(b'[') {
            return None;
        }
        let mut i = self.pos + 1;
        while self.src.get(i) == Some(&b'=') {
            i += 1;
        }
        if self.src.get(i) == Some(&b'[') {
            let level = i - self.pos - 1;
            self.pos = i + 1;
            Some(level)
        } else {
            None
        }
    }

    /// Reads up to the matching `]` `=`*level `]`. Like Lua, skips a newline
    /// right after the opening bracket and turns every newline sequence into
    /// `\n`.
    fn long_bracket_body(&mut self, open: usize, level: usize) -> R<Vec<u8>> {
        match (self.peek(), self.peek_at(1)) {
            (Some(b'\r'), Some(b'\n')) | (Some(b'\n'), Some(b'\r')) => self.pos += 2,
            (Some(b'\r' | b'\n'), _) => self.pos += 1,
            _ => {}
        }
        let start = self.pos;
        let mut search = start;
        let close = loop {
            let Some(i) = memchr(b']', &self.src[search..]) else {
                return Err(self.error_at(open, "unfinished long string or comment"));
            };
            let at = search + i;
            let eqs = self.src[at + 1..]
                .iter()
                .take_while(|&&b| b == b'=')
                .count();
            if eqs == level && self.src.get(at + 1 + level) == Some(&b']') {
                break at;
            }
            search = at + 1;
        };
        self.pos = close + level + 2;
        Ok(normalize_newlines(&self.src[start..close]))
    }

    fn globals(mut self) -> R<Globals> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia()?;
            match self.peek() {
                None => return Ok(out),
                Some(b';') => self.pos += 1,
                Some(c) if is_ident_start(c) => {
                    let name = self.ident();
                    self.skip_trivia()?;
                    self.expect(b'=')?;
                    self.skip_trivia()?;
                    let v = self.value()?;
                    self.statement_end()?;
                    // Identifiers are ASCII, so this never replaces anything.
                    out.push((String::from_utf8_lossy(name).into_owned(), v));
                }
                Some(_) => return self.err("expected `Name = value`"),
            }
        }
    }

    /// WoW ends every statement with a newline, so a file that stops after a
    /// value without one was cut off mid-write (`Version = 31` read as
    /// `Version = 3`). Consumes nothing up to and including the newline.
    fn statement_end(&mut self) -> R<()> {
        let end = self.pos;
        let truncated = |p: &Self| {
            Err(p.error_at(
                end,
                "file ends right after this value with no newline (truncated?)",
            ))
        };
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | 0x0b | 0x0c | b';') => self.pos += 1,
                Some(b'-') if self.peek_at(1) == Some(b'-') => {
                    let start = self.pos;
                    self.pos += 2;
                    if let Some(level) = self.long_bracket_open() {
                        self.long_bracket_body(start, level)?;
                    } else if memchr(b'\n', &self.src[self.pos..]).is_some() {
                        self.pos = start;
                        return Ok(());
                    } else {
                        return truncated(self);
                    }
                }
                None => return truncated(self),
                // A newline, or another statement on the same line.
                Some(_) => return Ok(()),
            }
        }
    }

    /// Parses one expression. Trivia before it must already be skipped.
    fn value(&mut self) -> R<LuaValue> {
        match self.peek() {
            None => self.err("expected a value, found end of file"),
            Some(b'{') => self.table(),
            Some(q @ (b'"' | b'\'')) => self.short_string(q).map(LuaValue::Str),
            Some(b'[') => {
                let open = self.pos;
                match self.long_bracket_open() {
                    Some(level) => Ok(LuaValue::Str(self.long_bracket_body(open, level)?.into())),
                    None => self.err("expected a value, found `[`"),
                }
            }
            Some(b'-') => {
                self.pos += 1;
                self.skip_trivia()?;
                let v = self.number(true)?;
                self.division(v)
            }
            Some(c) if is_ident_start(c) => {
                let start = self.pos;
                match self.ident() {
                    b"true" => Ok(LuaValue::Bool(true)),
                    b"false" => Ok(LuaValue::Bool(false)),
                    b"nil" => Ok(LuaValue::Nil),
                    _ => {
                        // Maybe an inf/nan spelling.
                        self.pos = start;
                        let v = self.number(false)?;
                        self.division(v)
                    }
                }
            }
            Some(_) => {
                let v = self.number(false)?;
                self.division(v)
            }
        }
    }

    /// `a/b` of two numeric literals, which some serializers emit for
    /// inf/nan (`1/0`, `-1/0`, `0/0`). No other operators are accepted.
    fn division(&mut self, lhs: LuaValue) -> R<LuaValue> {
        // Look past trivia for a `/`, but leave the position at the end of
        // the number otherwise, so `statement_end` sees the real line end.
        let end = self.pos;
        self.skip_trivia()?;
        if self.peek() != Some(b'/') {
            self.pos = end;
            return Ok(lhs);
        }
        self.pos += 1;
        self.skip_trivia()?;
        let neg = self.peek() == Some(b'-');
        if neg {
            self.pos += 1;
            self.skip_trivia()?;
        }
        // Negate as a float so `1/-0` is -inf, as in Lua.
        let b = self.number(false)?.as_f64().unwrap();
        let b = if neg { -b } else { b };
        Ok(LuaValue::Num(lhs.as_f64().unwrap() / b))
    }

    /// A numeric literal, including the inf/nan spellings C runtimes print
    /// (`inf`, `nan`, `-nan(ind)`, `1.#INF`, `1.#QNAN`).
    fn number(&mut self, neg: bool) -> R<LuaValue> {
        let start = self.pos;
        let signed = |v: f64| LuaValue::Num(if neg { -v } else { v });

        if self.peek().is_some_and(is_ident_start) {
            let id = self.ident();
            let v = if id.eq_ignore_ascii_case(b"inf") || id.eq_ignore_ascii_case(b"infinity") {
                f64::INFINITY
            } else if id.eq_ignore_ascii_case(b"nan") {
                f64::NAN
            } else {
                return Err(self.error_at(
                    start,
                    format!(
                        "unexpected identifier `{}`; only data is allowed",
                        String::from_utf8_lossy(id)
                    ),
                ));
            };
            // glibc/MSVC payload suffix: nan(ind), nan(snan), nan(0x...).
            if self.peek() == Some(b'(') {
                self.pos += 1;
                self.ident();
                self.expect(b')')?;
            }
            return Ok(signed(v));
        }

        if self.peek() == Some(b'0') && matches!(self.peek_at(1), Some(b'x' | b'X')) {
            self.pos += 2;
            let digits_start = self.pos;
            self.digits(u8::is_ascii_hexdigit);
            let hex = &self.src[digits_start..self.pos];
            if hex.is_empty() || self.peek().is_some_and(is_ident_char) {
                return Err(self.error_at(start, "malformed number"));
            }
            let mut int: Option<u64> = Some(0);
            let mut float = 0f64;
            for &h in hex {
                let d = (h as char).to_digit(16).unwrap();
                int = int.and_then(|v| v.checked_mul(16)?.checked_add(d as u64));
                float = float * 16.0 + d as f64;
            }
            return Ok(match int.and_then(|v| int_with_sign(v, neg)) {
                Some(i) => LuaValue::Int(i),
                None => signed(float),
            });
        }

        let mut is_float = false;
        self.digits(u8::is_ascii_digit);
        if self.peek() == Some(b'.') {
            if self.peek_at(1) == Some(b'#') {
                // MSVC: 1.#INF, 1.#IND, 1.#QNAN, 1.#SNAN, optionally followed
                // by digits (1.#INF00).
                self.pos += 2;
                let id = self.ident().to_ascii_uppercase();
                return if id.starts_with(b"INF") {
                    Ok(signed(f64::INFINITY))
                } else if id.starts_with(b"IND")
                    || id.starts_with(b"QNAN")
                    || id.starts_with(b"SNAN")
                {
                    Ok(signed(f64::NAN))
                } else {
                    Err(self.error_at(start, "malformed number"))
                };
            }
            self.pos += 1;
            self.digits(u8::is_ascii_digit);
            is_float = true;
        }
        let mantissa = &self.src[start..self.pos];
        if mantissa.is_empty() || mantissa == b"." {
            return match self.peek() {
                Some(c) => self.err(format!("unexpected `{}`", c.escape_ascii())),
                None => self.err("expected a value, found end of file"),
            };
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            self.digits(u8::is_ascii_digit);
            if self.pos == exp_start {
                return Err(self.error_at(start, "malformed number"));
            }
            is_float = true;
        }
        if self.peek().is_some_and(|c| is_ident_char(c) || c == b'.') {
            return Err(self.error_at(start, "malformed number"));
        }
        let text = &self.src[start..self.pos];
        if !is_float {
            let mut v: Option<u64> = Some(0);
            for &d in text {
                v = v.and_then(|v| v.checked_mul(10)?.checked_add((d - b'0') as u64));
            }
            if let Some(i) = v.and_then(|v| int_with_sign(v, neg)) {
                return Ok(LuaValue::Int(i));
            }
        }
        // Only ASCII digits, `.`, `e`, `+`, `-` were consumed.
        let text = std::str::from_utf8(text).unwrap();
        match text.parse::<f64>() {
            Ok(v) => Ok(signed(v)),
            Err(_) => Err(self.error_at(start, "malformed number")),
        }
    }

    fn short_string(&mut self, quote: u8) -> R<Box<[u8]>> {
        let src = self.src;
        let open = self.pos;
        self.pos += 1;
        // Only allocate a buffer once an escape forces us to.
        let mut buf: Option<Vec<u8>> = None;
        let mut seg = self.pos;
        loop {
            let Some(i) = memchr3(quote, b'\\', b'\n', &src[self.pos..]) else {
                return Err(self.error_at(open, "unfinished string"));
            };
            let at = self.pos + i;
            match src[at] {
                b'\\' => {
                    if memchr(b'\r', &src[seg..at]).is_some() {
                        return Err(self.error_at(open, "unfinished string"));
                    }
                    let b = buf.get_or_insert_with(Vec::new);
                    b.extend_from_slice(&src[seg..at]);
                    self.pos = at + 1;
                    self.escape(b)?;
                    seg = self.pos;
                }
                b'\n' => return Err(self.error_at(open, "unfinished string")),
                _ => {
                    // Lua also ends a short string at a raw `\r`. Rare, so it's
                    // checked once per segment rather than in the hot search.
                    if memchr(b'\r', &src[seg..at]).is_some() {
                        return Err(self.error_at(open, "unfinished string"));
                    }
                    self.pos = at + 1;
                    return Ok(match buf {
                        None => src[seg..at].into(),
                        Some(mut b) => {
                            b.extend_from_slice(&src[seg..at]);
                            b.into_boxed_slice()
                        }
                    });
                }
            }
        }
    }

    /// Decodes the escape after a `\`. Lua 5.1 escapes plus the 5.2+ forms
    /// (`\x`, `\z`, `\u{}`); an unknown escape keeps the character, as 5.1
    /// does.
    fn escape(&mut self, out: &mut Vec<u8>) -> R<()> {
        let esc = self.pos - 1;
        let Some(c) = self.peek() else {
            return self.err("unfinished string");
        };
        self.pos += 1;
        match c {
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            b'\n' | b'\r' => {
                out.push(b'\n');
                if let Some(n @ (b'\n' | b'\r')) = self.peek() {
                    if n != c {
                        self.pos += 1;
                    }
                }
            }
            b'0'..=b'9' => {
                let mut v = (c - b'0') as u32;
                for _ in 0..2 {
                    match self.peek() {
                        Some(d @ b'0'..=b'9') => {
                            v = v * 10 + (d - b'0') as u32;
                            self.pos += 1;
                        }
                        _ => break,
                    }
                }
                if v > 255 {
                    return Err(self.error_at(esc, "decimal escape too large"));
                }
                out.push(v as u8);
            }
            b'x' => {
                // Check the digits first: from_str_radix would accept "+f".
                let hex = self
                    .src
                    .get(self.pos..self.pos + 2)
                    .filter(|h| h.iter().all(u8::is_ascii_hexdigit));
                match hex.and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        self.pos += 2;
                    }
                    None => return Err(self.error_at(esc, "hexadecimal digit expected")),
                }
            }
            b'z' => {
                while matches!(
                    self.peek(),
                    Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
                ) {
                    self.pos += 1;
                }
            }
            b'u' => {
                self.expect(b'{')?;
                let start = self.pos;
                self.digits(u8::is_ascii_hexdigit);
                let digits = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
                let ch = u32::from_str_radix(digits, 16)
                    .ok()
                    .and_then(char::from_u32);
                let Some(ch) = ch else {
                    return Err(self.error_at(esc, "invalid unicode escape"));
                };
                self.expect(b'}')?;
                out.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
            }
            other => out.push(other),
        }
        Ok(())
    }

    fn table(&mut self) -> R<LuaValue> {
        let open = self.pos;
        self.pos += 1;
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error_at(open, format!("tables nested deeper than {MAX_DEPTH}")));
        }
        let arr_base = self.arr_stack.len();
        let hash_base = self.hash_stack.len();
        loop {
            self.skip_trivia()?;
            match self.peek() {
                None => return Err(self.unfinished_table(open)),
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                Some(b'[') if !matches!(self.peek_at(1), Some(b'[' | b'=')) => {
                    self.pos += 1;
                    self.skip_trivia()?;
                    let k = self.value()?;
                    self.skip_trivia()?;
                    self.expect(b']')?;
                    self.skip_trivia()?;
                    self.expect(b'=')?;
                    self.skip_trivia()?;
                    let v = self.value()?;
                    self.hash_stack.push((k, v));
                }
                Some(c) if is_ident_start(c) => {
                    let start = self.pos;
                    let name = self.ident();
                    self.skip_trivia()?;
                    if self.peek() == Some(b'=') && self.peek_at(1) != Some(b'=') {
                        self.pos += 1;
                        self.skip_trivia()?;
                        let v = self.value()?;
                        self.hash_stack.push((LuaValue::str(name), v));
                    } else {
                        self.pos = start;
                        let v = self.value()?;
                        self.arr_stack.push(v);
                    }
                }
                Some(_) => {
                    let v = self.value()?;
                    self.arr_stack.push(v);
                }
            }
            self.skip_trivia()?;
            match self.peek() {
                Some(b',' | b';') => self.pos += 1,
                Some(b'}') => {}
                None => return Err(self.unfinished_table(open)),
                Some(c) => {
                    return self.err(format!(
                        "expected `,` or `}}`, found `{}`",
                        c.escape_ascii()
                    ))
                }
            }
        }
        self.depth -= 1;
        let array = self.arr_stack.drain(arr_base..).collect();
        let hash = self.hash_stack.drain(hash_base..).collect();
        Ok(LuaValue::Table(Box::new(LuaTable { array, hash })))
    }

    fn unfinished_table(&self, open: usize) -> ParseError {
        let opened = self.error_at(open, "");
        self.error_at(
            self.src.len(),
            format!(
                "end of file inside the table opened at line {}, col {} (file truncated?)",
                opened.line, opened.col
            ),
        )
    }
}

fn int_with_sign(v: u64, neg: bool) -> Option<i64> {
    if neg {
        0i64.checked_sub_unsigned(v)
    } else {
        i64::try_from(v).ok()
    }
}

fn normalize_newlines(s: &[u8]) -> Vec<u8> {
    if memchr(b'\r', s).is_none() {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        match (s[i], s.get(i + 1)) {
            (b'\r', Some(b'\n')) | (b'\n', Some(b'\r')) => {
                out.push(b'\n');
                i += 2;
            }
            (b'\r', _) => {
                out.push(b'\n');
                i += 1;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use LuaValue::*;

    fn v(src: &str) -> LuaValue {
        parse_value(src.as_bytes()).unwrap_or_else(|e| panic!("{src:?}: {e}"))
    }

    fn e(src: &[u8]) -> ParseError {
        match parse(src) {
            Ok(g) => panic!("{:?} parsed: {g:?}", String::from_utf8_lossy(src)),
            Err(e) => e,
        }
    }

    fn s(x: &str) -> LuaValue {
        LuaValue::str(x)
    }

    fn table(array: Vec<LuaValue>, hash: Vec<(LuaValue, LuaValue)>) -> LuaValue {
        Table(Box::new(LuaTable { array, hash }))
    }

    #[test]
    fn scalars() {
        assert_eq!(v("nil"), Nil);
        assert_eq!(v("true"), Bool(true));
        assert_eq!(v("false"), Bool(false));
        assert_eq!(v("42"), Int(42));
        assert_eq!(v("-42"), Int(-42));
        assert_eq!(v("- 7"), Int(-7));
        assert_eq!(v("1.5"), Num(1.5));
        assert_eq!(v("1."), Num(1.0));
        assert_eq!(v(".5"), Num(0.5));
        assert_eq!(v("1e3"), Num(1000.0));
        assert_eq!(v("1.5E-2"), Num(0.015));
        assert_eq!(v("2e+2"), Num(200.0));
        assert_eq!(v("0x1F"), Int(31));
        assert_eq!(v("-0XfF"), Int(-255));
        assert_eq!(v("-0.0"), Num(-0.0));
        assert_ne!(v("-0.0"), Num(0.0));
    }

    #[test]
    fn integer_edges() {
        assert_eq!(v("9223372036854775807"), Int(i64::MAX));
        assert_eq!(v("-9223372036854775808"), Int(i64::MIN));
        assert_eq!(v("9223372036854775808"), Num(9223372036854775808.0));
        assert_eq!(v("0xFFFFFFFFFFFFFFFF"), Num(18446744073709551615.0));
        assert_eq!(
            v("123456789012345678901234567890"),
            Num(1.2345678901234568e29)
        );
    }

    #[test]
    fn inf_and_nan_spellings() {
        for (src, want) in [
            ("1/0", f64::INFINITY),
            ("-1/0", f64::NEG_INFINITY),
            ("1 / -0", f64::NEG_INFINITY),
            ("inf", f64::INFINITY),
            ("-inf", f64::NEG_INFINITY),
            ("INF", f64::INFINITY),
            ("infinity", f64::INFINITY),
            ("1.#INF", f64::INFINITY),
            ("-1.#INF", f64::NEG_INFINITY),
            ("1.#INF00", f64::INFINITY),
        ] {
            assert_eq!(v(src), Num(want), "{src}");
        }
        for src in [
            "0/0",
            "nan",
            "-nan",
            "NaN",
            "-nan(ind)",
            "nan(snan)",
            "1.#IND",
            "-1.#IND",
            "1.#QNAN",
        ] {
            assert!(matches!(v(src), Num(n) if n.is_nan()), "{src}");
        }
        assert_eq!(v("1/4"), Num(0.25));
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(v(r#""plain""#), s("plain"));
        assert_eq!(v(r#"'single "quoted"'"#), s("single \"quoted\""));
        assert_eq!(v(r#""a\"b\\c""#), s("a\"b\\c"));
        assert_eq!(v(r#""\a\b\f\n\r\t\v""#), s("\x07\x08\x0c\n\r\t\x0b"));
        assert_eq!(v(r#""\65\066\0671""#), s("ABC1"));
        assert_eq!(v(r#""\000""#), s("\0"));
        assert_eq!(v(r#""\255""#).as_bytes(), Some(&[255u8][..]));
        assert_eq!(v(r#""\x41\x7a""#), s("Az"));
        assert_eq!(v(r#""\u{48}\u{e9}\u{1F600}""#), s("Hé😀"));
        assert_eq!(v("\"a\\z  \n  b\""), s("ab"));
        assert_eq!(v(r#""\q\'""#), s("q'"));
        // `%q` writes a newline as backslash + real newline.
        assert_eq!(v("\"line1\\\nline2\""), s("line1\nline2"));
        assert_eq!(v("\"line1\\\r\nline2\""), s("line1\nline2"));
        assert_eq!(
            v(r#""|cffff0000Red|r|Hitem:19019::::|h[Thunderfury]|h""#),
            s("|cffff0000Red|r|Hitem:19019::::|h[Thunderfury]|h")
        );
        assert_eq!(v("\"caf\u{e9}\""), s("café"));
    }

    #[test]
    fn non_utf8_bytes_are_kept() {
        let g = parse(b"X = \"\xff\xfe\xc3\"\n").unwrap();
        assert_eq!(g[0].1.as_bytes(), Some(&b"\xff\xfe\xc3"[..]));
        assert_eq!(
            g[0].1.to_string_lossy().unwrap(),
            "\u{fffd}\u{fffd}\u{fffd}"
        );
    }

    #[test]
    fn long_strings() {
        assert_eq!(v("[[abc]]"), s("abc"));
        assert_eq!(v("[[\nfirst newline skipped]]"), s("first newline skipped"));
        assert_eq!(v("[[\r\nfirst CRLF skipped]]"), s("first CRLF skipped"));
        assert_eq!(
            v("[==[has ]] and ]=] inside]==]"),
            s("has ]] and ]=] inside")
        );
        assert_eq!(v("[[a\r\nb\rc]]"), s("a\nb\nc"));
        assert_eq!(v("[[no \\n escapes]]"), s("no \\n escapes"));
    }

    #[test]
    fn comments() {
        let g = parse(b"-- header\nA = 1 -- trailing\n--[[ block\n B = 2 ]]\n--[==[ ]] ]==] C = { -- [1]\n 3, --[[x]] 4 }\n").unwrap();
        assert_eq!(
            g,
            vec![
                ("A".into(), Int(1)),
                ("C".into(), table(vec![Int(3), Int(4)], vec![]))
            ]
        );
    }

    #[test]
    fn lookups_follow_lua_semantics() {
        let t = v(
            r#"{ a = 1, ["a"] = 2, "first", "second", [2] = "keyed two", [3.0] = "three", [4] = "x", [4] = "y" }"#,
        );
        let t = t.as_table().unwrap();
        // Duplicate keys: last wins.
        assert_eq!(t.get("a"), Some(&Int(2)));
        // Positional beats keyed for the same slot; Int and Num keys are one slot.
        assert_eq!(t.get_index(1), Some(&s("first")));
        assert_eq!(t.get_index(2), Some(&s("second")));
        assert_eq!(t.get_index(3), Some(&s("three")));
        assert_eq!(t.get_index(4), Some(&s("y")));
        assert_eq!(t.get_index(0), None);
        assert_eq!(t.get_index(-1), None);
        assert_eq!(t.get_index(i64::MIN), None);
        assert_eq!(t.get_index(i64::MAX), None);
        assert_eq!(t.get("missing"), None);
    }

    #[test]
    fn tables() {
        assert_eq!(v("{}"), table(vec![], vec![]));
        assert_eq!(
            v(r#"{ "a", ["k"] = 1, name = true; [3] = "x", "b", }"#),
            table(
                vec![s("a"), s("b")],
                vec![(s("k"), Int(1)), (s("name"), Bool(true)), (Int(3), s("x"))]
            )
        );
        assert_eq!(
            v("{ [ [[long key]] ] = 1 }"),
            table(vec![], vec![(s("long key"), Int(1))])
        );
        assert_eq!(
            v("{ [[long value]] }"),
            table(vec![s("long value")], vec![])
        );
        assert_eq!(
            v("{ true, nil, false }"),
            table(vec![Bool(true), Nil, Bool(false)], vec![])
        );
        assert_eq!(
            v("{ [1.5] = 1, [-2] = 2 }"),
            table(vec![], vec![(Num(1.5), Int(1)), (Int(-2), Int(2))])
        );
        let t = v(r#"{ outer = { inner = { "deep" } } }"#);
        let inner = t
            .as_table()
            .unwrap()
            .get("outer")
            .unwrap()
            .as_table()
            .unwrap()
            .get("inner")
            .unwrap();
        assert_eq!(inner, &table(vec![s("deep")], vec![]));
    }

    #[test]
    fn wow_shaped_file() {
        let src = b"\xEF\xBB\xBF\r\nMyAddonDB = {\r\n\t[\"profileKeys\"] = {\r\n\t\t[\"Thrandor - Forever\"] = \"Default\",\r\n\t},\r\n\t[\"list\"] = {\r\n\t\t\"a\", -- [1]\r\n\t\t\"b\", -- [2]\r\n\t},\r\n}\r\nMyAddonVersion = 3\r\nMyAddonEmpty = nil\r\n";
        let g = parse(src).unwrap();
        assert_eq!(g.len(), 3);
        assert_eq!(g[0].0, "MyAddonDB");
        let db = g[0].1.as_table().unwrap();
        assert_eq!(
            db.get("profileKeys")
                .unwrap()
                .as_table()
                .unwrap()
                .get("Thrandor - Forever"),
            Some(&s("Default"))
        );
        assert_eq!(
            db.get("list").unwrap().as_table().unwrap().array,
            vec![s("a"), s("b")]
        );
        assert_eq!(g[1], ("MyAddonVersion".into(), Int(3)));
        assert_eq!(g[2], ("MyAddonEmpty".into(), Nil));
    }

    #[test]
    fn empty_files() {
        assert_eq!(parse(b"").unwrap(), vec![]);
        assert_eq!(parse(b"\n\n-- nothing\n").unwrap(), vec![]);
        assert_eq!(parse(b"A = 1; B = 2;\n").unwrap().len(), 2);
        assert_eq!(parse(b"A = 1 -- trailing comment\n").unwrap().len(), 1);
        assert_eq!(parse(b"A = 1 --[[ block ]] ;\r\n").unwrap().len(), 1);
    }

    #[test]
    fn a_value_cut_off_before_its_newline_is_truncated() {
        // `Version = 31\n` torn after the 3: must not parse as 3.
        for src in [
            &b"DB = {}\nVersion = 3"[..],
            b"Version = 3 ",
            b"Version = 3;",
            b"Name = \"abc\"",
            b"T = { 1 }",
            b"F = 1/0",
            b"A = 1 -- comment with no newline",
            b"A = 1 --[[ block ]]",
        ] {
            let err = e(src);
            assert!(
                err.msg.contains("truncated"),
                "{:?}: {err}",
                String::from_utf8_lossy(src)
            );
        }
        let err = e(b"DB = {}\nVersion = 3");
        assert_eq!((err.line, err.col), (2, 12));
    }

    #[test]
    fn raw_carriage_return_ends_a_short_string() {
        assert!(e(b"A = \"a\rb\"\n").msg.contains("unfinished string"));
        assert!(e(b"A = \"a\rb\\n\"\n").msg.contains("unfinished string"));
        // An escaped CR (backslash + CRLF) is still a newline escape.
        assert_eq!(v("\"a\\\r\nb\""), s("a\nb"));
    }

    #[test]
    fn rejects_code() {
        for (src, needle) in [
            (&b"A = print(1)"[..], "unexpected identifier `print`"),
            (b"A = B", "unexpected identifier `B`"),
            (b"A = { foo }", "unexpected identifier `foo`"),
            (b"A = 1 + 2", "expected `Name = value`"),
            (b"A = { 1 + 2 }", "expected `,` or `}`"),
            (b"A = function() end", "unexpected identifier `function`"),
            (b"local A = 1", "expected `=`"),
            (b"A.b = 1", "expected `=`"),
            (b"A = #x", "unexpected `#`"),
            (b"A = 1abc", "malformed number"),
            (b"A = 1e", "malformed number"),
            (b"A = 0x", "malformed number"),
            (b"A = 1..2", "malformed number"),
            (b"= 1", "expected `Name = value`"),
        ] {
            let err = e(src);
            assert!(
                err.msg.contains(needle),
                "{:?}: {err}",
                String::from_utf8_lossy(src)
            );
        }
    }

    #[test]
    fn bad_strings() {
        assert!(e(b"A = \"abc").msg.contains("unfinished string"));
        assert!(e(b"A = \"abc\nd\"").msg.contains("unfinished string"));
        assert!(e(b"A = \"\\256\"").msg.contains("decimal escape too large"));
        assert!(e(b"A = \"\\xZZ\"")
            .msg
            .contains("hexadecimal digit expected"));
        assert!(e(b"A = \"\\x+f\"\n")
            .msg
            .contains("hexadecimal digit expected"));
        assert!(e(b"A = \"\\u{110000}\"")
            .msg
            .contains("invalid unicode escape"));
        assert!(e(b"A = [==[ abc ]=]")
            .msg
            .contains("unfinished long string"));
        assert!(e(b"--[[ never closed")
            .msg
            .contains("unfinished long string or comment"));
    }

    #[test]
    fn truncated_file_reports_position() {
        let err = e(b"DB = {\n\t[\"a\"] = {\n\t\t1,\n\t\t2");
        assert_eq!((err.line, err.col), (4, 4));
        assert!(err.msg.contains("table opened at line 2, col 10"), "{err}");
        let err = e(b"DB = {\n\t[\"a\"] = ");
        assert!(err.msg.contains("end of file"), "{err}");
    }

    #[test]
    fn error_line_and_column() {
        let err = e(b"A = 1\nB = {\n  x = oops,\n}");
        assert_eq!((err.line, err.col), (3, 7));
        assert_eq!(
            err.to_string(),
            "line 3, col 7: unexpected identifier `oops`; only data is allowed"
        );
    }

    #[test]
    fn depth_limit() {
        let ok = format!("A = {}{}\n", "{".repeat(MAX_DEPTH), "}".repeat(MAX_DEPTH));
        assert!(parse(ok.as_bytes()).is_ok());
        let deep = format!(
            "A = {}{}",
            "{".repeat(MAX_DEPTH + 1),
            "}".repeat(MAX_DEPTH + 1)
        );
        assert!(e(deep.as_bytes()).msg.contains("nested deeper"));
    }

    #[test]
    fn tables_are_exactly_sized() {
        let t = v("{ 1, 2, 3, 4, 5, a = 1, b = 2, c = 3 }");
        let t = t.as_table().unwrap();
        assert_eq!(t.array.capacity(), 5);
        assert_eq!(t.hash.capacity(), 3);
    }
}
