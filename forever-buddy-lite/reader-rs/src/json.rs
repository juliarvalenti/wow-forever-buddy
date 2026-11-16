//! The JSON boundary: how parsed Lua tables and CBOR maps become `serde_json::Value`, and the few
//! JavaScript conventions the original reader's output follows (so the two can be compared).
//!
//! Lua tables and CBOR maps arrive with arbitrary keys. The JS reader turned them into JS objects,
//! whose keys are ordered "array-index-like integers first, ascending; then everything else in
//! insertion order". Doing the same here keeps iteration order (bags 0..4 before the bank's -1, gear
//! slots ascending) identical, which matters for list output.

use serde_json::{Map, Number, Value};

/// Largest integer a double holds exactly; beyond it numbers stay floats.
const MAX_SAFE_INT: f64 = 9_007_199_254_740_991.0;

/// A number as JSON: whole numbers as integers (so `20`, not `20.0`), others as floats.
/// NaN and infinities have no JSON form and become null, as `JSON.stringify` does.
pub fn number(x: f64) -> Value {
    if !x.is_finite() {
        return Value::Null;
    }
    if x.fract() == 0.0 && x.abs() <= MAX_SAFE_INT {
        return Value::Number(Number::from(x as i64));
    }
    Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null)
}

/// A number as JavaScript prints it in a string (`String(x)`), for keys and messages.
pub fn number_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x.fract() == 0.0 && x.abs() < 1e21 {
        return format!("{}", x as i128);
    }
    // Rust's Display is the shortest round-trip form, like JS, for everyday magnitudes.
    format!("{x}")
}

/// Whether a key is one JavaScript sorts first in an object ("0", "1", ... up to 2^32 - 2).
pub fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || key.len() > 10 || (key.len() > 1 && key.starts_with('0')) {
        return None;
    }
    if !key.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    key.parse::<u64>().ok().filter(|&n| n < u32::MAX as u64).map(|n| n as u32)
}

/// Builds an object from entries the way a JS object would hold them: a repeated key keeps its first
/// position and takes the last value; integer keys come first, ascending.
pub fn js_object(entries: impl IntoIterator<Item = (String, Value)>) -> Map<String, Value> {
    let mut ordered: Vec<(String, Value)> = Vec::new();
    let mut position: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (key, value) in entries {
        match position.get(&key) {
            Some(&at) => ordered[at].1 = value,
            None => {
                position.insert(key.clone(), ordered.len());
                ordered.push((key, value));
            }
        }
    }
    // Integer keys first, ascending; a stable sort keeps the others in insertion order.
    ordered.sort_by_key(|(k, _)| array_index(k).map_or((1, 0), |n| (0, n)));
    ordered.into_iter().collect()
}

/// A Lua sequence (keys exactly 1..n, in order) becomes an array; anything else stays an object.
pub fn object_or_sequence(map: Map<String, Value>) -> Value {
    let is_sequence = !map.is_empty() && map.keys().enumerate().all(|(i, k)| *k == (i + 1).to_string());
    if is_sequence {
        Value::Array(map.into_iter().map(|(_, v)| v).collect())
    } else {
        Value::Object(map)
    }
}

/// JavaScript truthiness of a JSON value: null, false, 0, NaN and "" are false; tables are true.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|x| x != 0.0 && !x.is_nan()).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// The values of a Lua table, whether it came out as an array or as an object (`list` in the JS).
pub fn list(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(Value::Object(o)) => o.values().collect(),
        _ => Vec::new(),
    }
}

/// Key/value pairs of a Lua table; an array's keys are 1-based (`entries` in the JS).
pub fn entries(v: Option<&Value>) -> Vec<(String, &Value)> {
    match v {
        Some(Value::Array(a)) => a.iter().enumerate().map(|(i, x)| ((i + 1).to_string(), x)).collect(),
        Some(Value::Object(o)) => o.iter().map(|(k, x)| (k.clone(), x)).collect(),
        _ => Vec::new(),
    }
}

/// Makes whole floats into integers throughout, so printed numbers look like the JS output.
pub fn tidy_numbers(v: Value) -> Value {
    match v {
        Value::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(_), _) => Value::Number(n),
            (None, Some(x)) => number(x),
            _ => Value::Number(n),
        },
        Value::Array(a) => Value::Array(a.into_iter().map(tidy_numbers).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, tidy_numbers(v))).collect()),
        other => other,
    }
}

/// `JSON.stringify(v, null, 1)`: one space of indent per level.
pub fn pretty(v: &Value) -> String {
    let mut out = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
    serde::Serialize::serialize(v, &mut ser).expect("JSON values always serialize");
    String::from_utf8(out).expect("serde_json writes UTF-8")
}

/// JavaScript's `Math.round`: halves round up (towards +infinity), unlike Rust's `round`.
pub fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// JavaScript's `toFixed(digits)`: the exact binary value, rounded half up at that digit.
pub fn to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return number_string(x);
    }
    // Plenty of extra digits, then round the decimal string by hand (Rust rounds exact ties to even).
    let long = format!("{:.*}", digits + 30, x.abs());
    let (int_part, frac_part) = long.split_once('.').unwrap_or((&long, ""));
    let mut kept: Vec<u8> = int_part.bytes().chain(frac_part.bytes().take(digits)).collect();
    let round_up = frac_part.as_bytes().get(digits).is_some_and(|&d| d >= b'5');
    if round_up {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let int_len = kept.len() - digits;
    let mut s = String::from_utf8(kept[..int_len].to_vec()).unwrap();
    if digits > 0 {
        s.push('.');
        s.push_str(std::str::from_utf8(&kept[int_len..]).unwrap());
    }
    // JS keeps the sign of any negative number, even one that rounds to zero ("-0.0").
    if x < 0.0 {
        s.insert(0, '-');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn integer_keys_sort_first_like_a_js_object() {
        let m = js_object([("b".into(), json!(1)), ("10".into(), json!(2)), ("-1".into(), json!(3)), ("2".into(), json!(4))]);
        assert_eq!(m.keys().collect::<Vec<_>>(), ["2", "10", "b", "-1"]);
    }

    #[test]
    fn a_repeated_key_keeps_its_place_and_takes_the_last_value() {
        let m = js_object([("a".into(), json!(1)), ("b".into(), json!(2)), ("a".into(), json!(3))]);
        assert_eq!(Value::Object(m), json!({"a": 3, "b": 2}));
    }

    #[test]
    fn sequences_become_arrays() {
        let seq = js_object([("2".into(), json!("b")), ("1".into(), json!("a"))]);
        assert_eq!(object_or_sequence(seq), json!(["a", "b"]));
        let gap = js_object([("1".into(), json!("a")), ("3".into(), json!("c"))]);
        assert!(object_or_sequence(gap).is_object());
        assert_eq!(object_or_sequence(Map::new()), json!({}));
    }

    #[test]
    fn rounding_and_fixed_digits_follow_javascript() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(to_fixed(0.125, 2), "0.13"); // Rust's {:.2} gives 0.12
        assert_eq!(to_fixed(12.3456, 2), "12.35");
        assert_eq!(to_fixed(9.999, 2), "10.00");
        assert_eq!(to_fixed(-0.001, 1), "-0.0");
        assert_eq!(to_fixed(-1.25, 1), "-1.3");
        assert_eq!(number_string(5.0), "5");
        assert_eq!(number_string(0.5), "0.5");
    }
}
