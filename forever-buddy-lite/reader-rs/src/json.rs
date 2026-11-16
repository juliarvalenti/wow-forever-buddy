//! The JSON boundary: how parsed Lua tables and CBOR maps become `serde_json::Value`.
//!
//! Lua has one structure, the table. A table keyed exactly 1..n is a list and becomes an array;
//! anything else becomes an object, its keys in the order the file wrote them.

use serde_json::{Map, Number, Value};

/// Largest integer a double holds exactly; beyond it numbers stay floats.
const MAX_SAFE_INT: f64 = 9_007_199_254_740_991.0;

/// A number as JSON: whole numbers as integers (so `20`, not `20.0`), others as floats.
/// NaN and infinities have no JSON form and become null.
pub fn number(x: f64) -> Value {
    if !x.is_finite() {
        return Value::Null;
    }
    if x.fract() == 0.0 && x.abs() <= MAX_SAFE_INT {
        return Value::Number(Number::from(x as i64));
    }
    Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null)
}

/// A number as a table key or in text: `16`, `-1`, `1.5`.
pub fn number_key(x: f64) -> String {
    x.to_string()
}

/// Builds a table from its entries: a list (keys exactly 1..n, in any order) becomes an array,
/// anything else an object. A repeated key keeps its first position and takes the last value.
pub fn table(entries: impl IntoIterator<Item = (String, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert(key, value);
    }
    let n = map.len();
    let is_list = n > 0 && (1..=n).all(|i| map.contains_key(&i.to_string()));
    if is_list {
        Value::Array((1..=n).map(|i| map.remove(&i.to_string()).unwrap()).collect())
    } else {
        Value::Object(map)
    }
}

/// Truthiness for flags saved as any type: null, false, 0, NaN and "" are false; tables are true.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// The values of a Lua table, whether it came out as an array or as an object.
pub fn list(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(Value::Object(o)) => o.values().collect(),
        _ => Vec::new(),
    }
}

/// Key/value pairs of a Lua table; an array's keys are 1-based, as in Lua.
pub fn entries(v: Option<&Value>) -> Vec<(String, &Value)> {
    match v {
        Some(Value::Array(a)) => a.iter().enumerate().map(|(i, x)| ((i + 1).to_string(), x)).collect(),
        Some(Value::Object(o)) => o.iter().map(|(k, x)| (k.clone(), x)).collect(),
        _ => Vec::new(),
    }
}

/// Makes whole floats into integers throughout, so `20.0` prints as `20`.
pub fn tidy_numbers(v: Value) -> Value {
    match v {
        Value::Number(n) if n.as_i64().is_none() && n.as_u64().is_none() => n.as_f64().map_or(Value::Number(n), number),
        Value::Array(a) => Value::Array(a.into_iter().map(tidy_numbers).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, tidy_numbers(v))).collect()),
        other => other,
    }
}

/// Pretty JSON with one space of indent per level (compact enough to read in a terminal).
pub fn pretty(v: &Value) -> String {
    let mut out = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
    serde::Serialize::serialize(v, &mut ser).expect("JSON values always serialize");
    String::from_utf8(out).expect("serde_json writes UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lists_become_arrays_whatever_order_they_were_written_in() {
        let seq = table([("2".into(), json!("b")), ("1".into(), json!("a"))]);
        assert_eq!(seq, json!(["a", "b"]));
        let gap = table([("1".into(), json!("a")), ("3".into(), json!("c"))]);
        assert!(gap.is_object());
        assert_eq!(table([]), json!({}));
    }

    #[test]
    fn objects_keep_the_written_order_and_the_last_value() {
        let t = table([("b".into(), json!(1)), ("10".into(), json!(2)), ("-1".into(), json!(3)), ("b".into(), json!(4))]);
        assert_eq!(t.as_object().unwrap().keys().collect::<Vec<_>>(), ["b", "10", "-1"]);
        assert_eq!(t["b"], 4);
    }

    #[test]
    fn numbers() {
        assert_eq!(number(20.0), json!(20));
        assert_eq!(number(0.5), json!(0.5));
        assert_eq!(number(f64::NAN), Value::Null);
        assert_eq!(number_key(16.0), "16");
        assert_eq!(number_key(-1.0), "-1");
        assert_eq!(tidy_numbers(json!({"a": [2.0, 2.5]})), json!({"a": [2, 2.5]}));
    }
}
