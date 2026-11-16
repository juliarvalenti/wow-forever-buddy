//! A minimal CBOR (RFC 8949) decoder: what the game's `C_EncodingUtil.SerializeCBOR` writes. Used for
//! Auctionator's price database, the addon's packed auction scan and QuestieDB's item records.
//!
//! Definite lengths only (the game never writes indefinite ones). Tags are skipped, keeping the value.

use crate::json;
use serde_json::Value as Json;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Unsigned(u64),
    /// A negative integer, stored as its value (-1 - n).
    Negative(i128),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    /// Entries in the order they were written.
    Map(Vec<(Value, Value)>),
    Bool(bool),
    Null,
    Float(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeError(pub String);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CBOR: {}", self.0)
    }
}

impl std::error::Error for DecodeError {}

/// Decodes one CBOR item from the start of `bytes` (anything after it is ignored).
pub fn decode(bytes: &[u8]) -> Result<Value, DecodeError> {
    Decoder { bytes, i: 0 }.item()
}

struct Decoder<'a> {
    bytes: &'a [u8],
    i: usize,
}

impl<'a> Decoder<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.i.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or_else(|| DecodeError(format!("truncated at byte {}", self.i)))?;
        let s = &self.bytes[self.i..end];
        self.i = end;
        Ok(s)
    }

    /// The argument of an item head: the count, length or integer value.
    fn argument(&mut self, info: u8) -> Result<u64, DecodeError> {
        let size = match info {
            0..=23 => return Ok(info as u64),
            24 => 1,
            25 => 2,
            26 => 4,
            27 => 8,
            _ => return Err(DecodeError("indefinite lengths aren't supported".into())),
        };
        Ok(self.take(size)?.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64))
    }

    fn length(&mut self, info: u8) -> Result<usize, DecodeError> {
        let n = self.argument(info)?;
        // Every element takes at least a byte, so a longer length than what's left is corrupt.
        if n > (self.bytes.len() - self.i) as u64 {
            return Err(DecodeError(format!("length {n} runs past the end")));
        }
        Ok(n as usize)
    }

    fn item(&mut self) -> Result<Value, DecodeError> {
        let head = self.take(1)?[0];
        let (major, info) = (head >> 5, head & 31);
        match major {
            0 => Ok(Value::Unsigned(self.argument(info)?)),
            1 => Ok(Value::Negative(-1 - self.argument(info)? as i128)),
            2 => {
                let n = self.length(info)?;
                Ok(Value::Bytes(self.take(n)?.to_vec()))
            }
            3 => {
                let n = self.length(info)?;
                Ok(Value::Text(String::from_utf8_lossy(self.take(n)?).into_owned()))
            }
            4 => {
                let n = self.length(info)?;
                (0..n).map(|_| self.item()).collect::<Result<_, _>>().map(Value::Array)
            }
            5 => {
                let n = self.length(info)?;
                let mut entries = Vec::with_capacity(n);
                for _ in 0..n {
                    let key = self.item()?;
                    entries.push((key, self.item()?));
                }
                Ok(Value::Map(entries))
            }
            6 => {
                self.argument(info)?; // the tag number: keep only the value
                self.item()
            }
            _ => self.simple(info),
        }
    }

    fn simple(&mut self, info: u8) -> Result<Value, DecodeError> {
        match info {
            20 => Ok(Value::Bool(false)),
            21 => Ok(Value::Bool(true)),
            22 | 23 => Ok(Value::Null), // null, undefined
            25 => {
                let b = self.take(2)?;
                Ok(Value::Float(half_to_f64(u16::from_be_bytes([b[0], b[1]]))))
            }
            26 => {
                let b = self.take(4)?;
                Ok(Value::Float(f32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f64))
            }
            27 => {
                let b = self.take(8)?;
                Ok(Value::Float(f64::from_be_bytes(b.try_into().unwrap())))
            }
            _ => Err(DecodeError(format!("unknown simple value {info}"))),
        }
    }
}

fn half_to_f64(h: u16) -> f64 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 31) as i32;
    let frac = (h & 1023) as f64;
    sign * match exp {
        0 => frac * 2f64.powi(-24),
        31 if frac != 0.0 => f64::NAN,
        31 => f64::INFINITY,
        _ => (1.0 + frac / 1024.0) * 2f64.powi(exp - 15),
    }
}

impl Value {
    /// The value as a number, if it is one.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Unsigned(n) => Some(*n as f64),
            Value::Negative(n) => Some(*n as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// The value as text: text strings, and byte strings read as UTF-8 (the game writes Lua strings
    /// as byte strings).
    pub fn as_text(&self) -> Option<String> {
        match self {
            Value::Text(s) => Some(s.clone()),
            Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    }

    /// A map key as a property name: numbers as digits, strings as themselves. A Lua table serialized
    /// to CBOR has number keys for its sequence part and string keys for the rest.
    pub fn key_string(&self) -> String {
        match self {
            Value::Unsigned(n) => n.to_string(),
            Value::Negative(n) => n.to_string(),
            Value::Float(f) => json::number_string(*f),
            Value::Text(s) => s.clone(),
            Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".into(),
            Value::Array(_) | Value::Map(_) => "[object]".into(),
        }
    }

    /// The entry under a property name, in a map (or an array, by 0-based index).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries.iter().rev().find(|(k, _)| k.key_string() == key).map(|(_, v)| v),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => None,
        }
    }

    /// Map entries with property-name keys, in JS object order (integer keys first, ascending).
    pub fn map_entries(&self) -> Vec<(String, &Value)> {
        let Value::Map(entries) = self else { return Vec::new() };
        let mut out: Vec<(String, &Value)> = Vec::with_capacity(entries.len());
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (k, v) in entries {
            let key = k.key_string();
            match seen.get(&key) {
                Some(&at) => out[at].1 = v,
                None => {
                    seen.insert(key.clone(), out.len());
                    out.push((key, v));
                }
            }
        }
        // Integer keys first, ascending (a stable sort keeps the others in written order).
        out.sort_by_key(|(k, _)| json::array_index(k).map_or((1, 0), |n| (0, n)));
        out
    }

    /// The value as JSON the way a Lua table reads: byte strings as UTF-8 text, maps keyed 1..n as
    /// arrays, other maps as objects with string keys.
    pub fn to_lua_json(&self) -> Json {
        match self {
            Value::Unsigned(_) | Value::Negative(_) | Value::Float(_) => json::number(self.as_f64().unwrap()),
            Value::Bytes(_) | Value::Text(_) => Json::String(self.as_text().unwrap()),
            Value::Bool(b) => Json::Bool(*b),
            Value::Null => Json::Null,
            Value::Array(items) => Json::Array(items.iter().map(Value::to_lua_json).collect()),
            Value::Map(entries) => {
                json::object_or_sequence(json::js_object(entries.iter().map(|(k, v)| (k.key_string(), v.to_lua_json()))))
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    /// A small CBOR encoder for test data, standing in for `C_EncodingUtil.SerializeCBOR`: Lua strings
    /// go out as byte strings, as the game writes them.
    pub fn encode(v: &Json) -> Vec<u8> {
        fn head(major: u8, n: u64) -> Vec<u8> {
            let m = major << 5;
            match n {
                0..=23 => vec![m | n as u8],
                24..=0xff => vec![m | 24, n as u8],
                0x100..=0xffff => [vec![m | 25], (n as u16).to_be_bytes().to_vec()].concat(),
                0x1_0000..=0xffff_ffff => [vec![m | 26], (n as u32).to_be_bytes().to_vec()].concat(),
                _ => [vec![m | 27], n.to_be_bytes().to_vec()].concat(),
            }
        }
        match v {
            Json::Number(n) => match n.as_u64() {
                Some(u) => head(0, u),
                None => {
                    let f = n.as_f64().unwrap();
                    if f < 0.0 && f.fract() == 0.0 {
                        head(1, (-1.0 - f) as u64)
                    } else {
                        [vec![0xfb], f.to_be_bytes().to_vec()].concat()
                    }
                }
            },
            Json::String(s) => [head(2, s.len() as u64), s.as_bytes().to_vec()].concat(),
            Json::Bool(b) => vec![if *b { 0xf5 } else { 0xf4 }],
            Json::Null => vec![0xf6],
            Json::Array(a) => {
                let mut out = head(4, a.len() as u64);
                a.iter().for_each(|x| out.extend(encode(x)));
                out
            }
            Json::Object(o) => {
                let mut out = head(5, o.len() as u64);
                for (k, x) in o {
                    // Numeric keys go out as numbers, as a Lua table's would.
                    match k.parse::<u64>() {
                        Ok(n) => out.extend(head(0, n)),
                        Err(_) => out.extend(encode(&Json::String(k.clone()))),
                    }
                    out.extend(encode(x));
                }
                out
            }
        }
    }

    /// The JS test's blob: { version = 2, ["2589"] = { m = 125, h = { ["2509"] = 130 } }, ["5201"] = { m = 2500000 } }.
    pub fn auctionator_blob() -> Vec<u8> {
        let mut b = vec![0xa3, 0x67];
        b.extend(b"version");
        b.extend([0x02, 0x64]);
        b.extend(b"2589");
        b.extend([0xa2, 0x61, 0x6d, 0x18, 0x7d, 0x61, 0x68, 0xa1, 0x64]);
        b.extend(b"2509");
        b.extend([0x18, 0x82, 0x64]);
        b.extend(b"5201");
        b.extend([0xa1, 0x61, 0x6d, 0x1a, 0x00, 0x26, 0x25, 0xa0]);
        b
    }

    #[test]
    fn decodes_auctionator_prices() {
        let v = decode(&auctionator_blob()).unwrap();
        assert_eq!(v.to_lua_json(), json!({"2589": {"m": 125, "h": {"2509": 130}}, "5201": {"m": 2500000}, "version": 2}));
        assert_eq!(v.get("5201").and_then(|e| e.get("m")).and_then(Value::as_f64), Some(2_500_000.0));
    }

    #[test]
    fn integers_of_every_width() {
        assert_eq!(decode(&[0x17]).unwrap(), Value::Unsigned(23));
        assert_eq!(decode(&[0x18, 0xff]).unwrap(), Value::Unsigned(255));
        assert_eq!(decode(&[0x19, 0x01, 0x00]).unwrap(), Value::Unsigned(256));
        assert_eq!(decode(&[0x1a, 0x00, 0x01, 0x00, 0x00]).unwrap(), Value::Unsigned(65536));
        assert_eq!(decode(&[0x1b, 0, 0, 0, 1, 0, 0, 0, 0]).unwrap(), Value::Unsigned(1 << 32));
        assert_eq!(decode(&[0x20]).unwrap(), Value::Negative(-1));
        assert_eq!(decode(&[0x38, 0x63]).unwrap(), Value::Negative(-100));
    }

    #[test]
    fn strings_bytes_and_simple_values() {
        assert_eq!(decode(&[0x63, b'a', b'b', b'c']).unwrap(), Value::Text("abc".into()));
        assert_eq!(decode(&[0x42, 0xc3, 0xa9]).unwrap().as_text().unwrap(), "é");
        assert_eq!(decode(&[0xf4]).unwrap(), Value::Bool(false));
        assert_eq!(decode(&[0xf5]).unwrap(), Value::Bool(true));
        assert_eq!(decode(&[0xf6]).unwrap(), Value::Null);
        assert_eq!(decode(&[0xf9, 0x3c, 0x00]).unwrap(), Value::Float(1.0)); // half
        assert_eq!(decode(&[0xf9, 0xc0, 0x00]).unwrap(), Value::Float(-2.0));
        assert_eq!(decode(&[0xfa, 0x3f, 0xc0, 0x00, 0x00]).unwrap(), Value::Float(1.5)); // single
        assert_eq!(decode(&[0xfb, 0x40, 0x09, 0x21, 0xfb, 0x54, 0x44, 0x2d, 0x18]).unwrap(), Value::Float(std::f64::consts::PI));
        assert_eq!(decode(&[0xc1, 0x05]).unwrap(), Value::Unsigned(5)); // a tag is skipped
    }

    #[test]
    fn arrays_and_maps() {
        let v = decode(&[0x82, 0x01, 0xa1, 0x61, b'k', 0x80]).unwrap();
        assert_eq!(v.to_lua_json(), json!([1, {"k": []}]));
        // A Lua sequence serialized as a map keyed 1..n reads as an array, whatever order it came in.
        let seq = decode(&[0xa2, 0x02, 0x41, b'b', 0x01, 0x41, b'a']).unwrap();
        assert_eq!(seq.to_lua_json(), json!(["a", "b"]));
    }

    #[test]
    fn round_trips_the_test_encoder() {
        let v = json!({"7357:1205": {"id": 7357, "tip": {"1": "+6 Stamina", "2": "+7 Intellect"}, "neg": -3}});
        assert_eq!(
            decode(&encode(&v)).unwrap().to_lua_json(),
            json!({"7357:1205": {"id": 7357, "tip": ["+6 Stamina", "+7 Intellect"], "neg": -3}})
        );
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert!(decode(&[]).is_err());
        assert!(decode(&[0x5f]).is_err()); // indefinite byte string
        assert!(decode(&[0x63, b'a']).is_err()); // truncated text
        assert!(decode(&[0xe0]).is_err()); // simple value 0
    }
}
