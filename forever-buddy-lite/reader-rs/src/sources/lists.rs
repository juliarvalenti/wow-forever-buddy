//! Serde helpers for SavedVariables shapes.
//!
//! A Lua table written as `{ a, b, c }` parses to a JSON array, but the same list with a gap (or keyed
//! from 0, like bag slots) parses to an object. These helpers accept either. They're lenient on
//! purpose: one malformed entry (an addon bug, an older schema) drops that entry, not the character.

use serde::de::{DeserializeOwned, Deserializer};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// A value that only matters as true/false, read loosely (`1`, `"yes"` and tables
/// count as true; `0`, `""` and nil as false).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Truthy(pub bool);

impl<'de> Deserialize<'de> for Truthy {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Truthy(crate::json::truthy(&Value::deserialize(d)?)))
    }
}

/// Converts a JSON value to `T`, or `T::default()` if it doesn't fit.
pub fn or_default<T: DeserializeOwned + Default>(v: Option<&Value>) -> T {
    v.and_then(|v| T::deserialize(v).ok()).unwrap_or_default()
}

/// The values of a table (array or object), each converted to `T`; entries that don't fit are dropped.
pub fn values<T: DeserializeOwned>(v: Option<&Value>) -> Vec<T> {
    crate::json::list(v).into_iter().filter_map(|x| T::deserialize(x).ok()).collect()
}

/// The entries of a table with their keys ("1".. for an array), each converted to `T`.
pub fn keyed<T: DeserializeOwned>(v: Option<&Value>) -> Vec<(String, T)> {
    crate::json::entries(v).into_iter().filter_map(|(k, x)| T::deserialize(x).ok().map(|t| (k, t))).collect()
}

/// `#[serde(deserialize_with = "lua_list")]`: a list field written as an array or a table.
pub fn lua_list<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(values(Some(&v)))
}

/// `#[serde(deserialize_with = "lua_list_opt")]`: like `lua_list`, but remembers whether the field
/// was there at all (an item with a `quests` table, even an empty one, is a quest reward).
pub fn lua_list_opt<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<Vec<T>>, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(if v.is_null() { None } else { Some(values(Some(&v))) })
}

/// `#[serde(deserialize_with = "lua_keyed")]`: a table keyed by slot or bag number.
pub fn lua_keyed<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<(String, T)>, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(keyed(Some(&v)))
}

/// A map that remembers insertion order, for merging several accounts' files: a key seen again keeps
/// its first position and may take the new value.
#[derive(Debug, Clone)]
pub struct OrderedMap<V> {
    entries: Vec<(String, V)>,
    index: HashMap<String, usize>,
}

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        OrderedMap { entries: Vec::new(), index: HashMap::new() }
    }
}

impl<V> OrderedMap<V> {
    pub fn get(&self, key: &str) -> Option<&V> {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    pub fn insert(&mut self, key: String, value: V) {
        match self.index.get(&key) {
            Some(&i) => self.entries[i].1 = value,
            None => {
                self.index.insert(key.clone(), self.entries.len());
                self.entries.push((key, value));
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &V)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lists_come_as_arrays_or_tables() {
        assert_eq!(values::<String>(Some(&json!(["a", "b"]))), ["a", "b"]);
        assert_eq!(values::<String>(Some(&json!({"1": "a", "3": "c"}))), ["a", "c"]);
        assert_eq!(values::<String>(Some(&json!(["a", 5, "c"]))), ["a", "c"]); // 5 isn't text: dropped
        assert!(values::<String>(None).is_empty());
        assert_eq!(keyed::<i32>(Some(&json!([7, 8]))), [("1".to_string(), 7), ("2".to_string(), 8)]);
    }

    #[test]
    fn truthiness() {
        let t: Vec<Truthy> = serde_json::from_value(json!([true, 1, "x", {}, false, 0, "", null])).unwrap();
        assert_eq!(t.iter().map(|x| x.0).collect::<Vec<_>>(), [true, true, true, true, false, false, false, false]);
    }

    #[test]
    fn ordered_map_keeps_first_position() {
        let mut m = OrderedMap::default();
        m.insert("b".into(), 1);
        m.insert("a".into(), 2);
        m.insert("b".into(), 3);
        assert_eq!(m.iter().map(|(k, v)| (k.as_str(), *v)).collect::<Vec<_>>(), [("b", 3), ("a", 2)]);
    }
}
