//! QuestieDB's data for Forever, from `QuestieDB_Forever.toc` (read only).
//!
//! The .toc carries the database as metadata lines, `## X-<Kind>-<id>-<field>: <base64 CBOR>`, for
//! items, quests, NPCs and objects. A long value is split: the line holds `~N~` and the parts follow
//! as `<key>-1` .. `<key>-N`. Fields used here:
//! - `X-Item-<id>-S`: a summary record: 1 name, 12 item class, 13 subclass;
//! - `X-Item-<id>-14`: the NPCs that sell the item; `X-Item-<id>-6`: quests that reward it;
//! - `X-Quest-<id>-...`: quest records (see `questxp`).

use crate::cbor;
use base64::Engine;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// The raw metadata, in file order.
#[derive(Debug, Default, Clone)]
pub struct QuestieDb {
    entries: Vec<(String, String)>,
    index: HashMap<String, usize>,
}

/// What QuestieDB says about one item.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct QuestieItem {
    pub name: Option<String>,
    /// The game's item class (7 trade goods, 4 armor...) and subclass.
    pub class: Option<i64>,
    pub subclass: Option<i64>,
}

/// Items: names and kinds, and which ones vendors sell without limit.
#[derive(Debug, Default, Clone)]
pub struct QuestieItems {
    pub items: HashMap<u32, QuestieItem>,
    /// Items sold by at least `vendor_cutoff` NPCs.
    pub vendor_items: HashSet<u32>,
}

impl QuestieItems {
    pub fn name(&self, id: u32) -> Option<&str> {
        self.items.get(&id).and_then(|i| i.name.as_deref())
    }
}

impl QuestieDb {
    /// Reads the .toc in `dir`; None when QuestieDB isn't installed there.
    pub fn load(dir: &Path) -> Option<QuestieDb> {
        std::fs::read_to_string(dir.join("QuestieDB_Forever.toc")).ok().map(|t| QuestieDb::parse(&t))
    }

    pub fn parse(toc: &str) -> QuestieDb {
        let mut db = QuestieDb::default();
        for line in toc.lines() {
            let Some((key, value)) = line.strip_prefix("## ").and_then(|l| l.split_once(": ")) else { continue };
            if !key.starts_with("X-") {
                continue;
            }
            db.index.insert(key.to_string(), db.entries.len());
            db.entries.push((key.to_string(), value.to_string()));
        }
        db
    }

    /// Every key, in file order ("X-Quest-123-S", ...).
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    /// A value, with a split one joined back together.
    pub fn raw(&self, key: &str) -> Option<String> {
        let value = &self.entries[*self.index.get(key)?].1;
        let parts = value.strip_prefix('~').and_then(|v| v.strip_suffix('~')).and_then(|n| n.parse::<usize>().ok());
        match parts {
            Some(n) => (1..=n).map(|i| self.entries.get(*self.index.get(&format!("{key}-{i}"))?).map(|e| e.1.clone())).collect(),
            None => Some(value.clone()),
        }
    }

    /// One field of a record, decoded: `field("Quest", 123, "S")` reads `X-Quest-123-S`.
    pub fn field(&self, kind: &str, id: u32, field: &str) -> Option<cbor::Value> {
        let text = self.raw(&format!("X-{kind}-{id}-{field}"))?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(text.trim()).ok()?;
        cbor::decode(&bytes).ok()
    }

    /// Ids of every record of a kind that has `field`, in file order.
    pub fn ids_with(&self, kind: &str, field: &str) -> Vec<u32> {
        let prefix = format!("X-{kind}-");
        let suffix = format!("-{field}");
        self.keys().filter_map(|k| k.strip_prefix(&prefix)?.strip_suffix(&suffix)?.parse().ok()).collect()
    }

    /// Item names and kinds, and the unlimited vendor goods.
    ///
    /// `vendor_cutoff`: how many vendors make an item "unlimited". Limited-supply sellers (Darkmoon
    /// Faire leather, a herbalist's few Peacebloom) are one to a handful of NPCs; goods every vendor
    /// stocks without limit (water, thread, vials, arrows, spices) are sold by dozens. Those have a
    /// fixed price ceiling, so the auction house can't be traded on them.
    pub fn items(&self, vendor_cutoff: usize) -> QuestieItems {
        let mut out = QuestieItems::default();
        for id in self.ids_with("Item", "S") {
            let Some(record) = self.field("Item", id, "S") else { continue };
            let number = |k: &str| record.get(k).and_then(cbor::Value::as_f64).map(|n| n as i64);
            let name = record.get("1").and_then(cbor::Value::as_text);
            out.items.insert(id, QuestieItem { name, class: number("12"), subclass: number("13") });
        }
        for id in self.ids_with("Item", "14") {
            if matches!(self.field("Item", id, "14"), Some(cbor::Value::Array(sellers)) if sellers.len() >= vendor_cutoff) {
                out.vendor_items.insert(id);
            }
        }
        out
    }
}

/// Items from the QuestieDB in `dir` (empty when it isn't installed).
pub fn load_items(dir: &Path, vendor_cutoff: usize) -> QuestieItems {
    QuestieDb::load(dir).map(|db| db.items(vendor_cutoff)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    #[test]
    fn names_kinds_and_vendor_counts() {
        // { [1] = "Worn Shortsword", [12] = 2, [13] = 7 }, the name as a byte string like the game writes it.
        let mut name = vec![0xa3, 0x01, 0x4f];
        name.extend(b"Worn Shortsword");
        name.extend([0x0c, 0x02, 0x0d, 0x07]);
        let few_sellers = [0x82, 0x01, 0x02];
        let mut many_sellers = vec![0x8f];
        many_sellers.extend(1..=15u8);
        let toc = format!(
            "## Title: QuestieDB\n## X-Item-25-S: {}\n## X-Item-85-14: {}\n## X-Item-159-14: {}\n## X-Item-9-S: not base64!\n",
            enc(&name),
            enc(&few_sellers),
            enc(&many_sellers)
        );
        let q = QuestieDb::parse(&toc).items(15);
        assert_eq!(q.items[&25], QuestieItem { name: Some("Worn Shortsword".into()), class: Some(2), subclass: Some(7) });
        assert!(!q.vendor_items.contains(&85)); // two sellers: limited supply
        assert!(q.vendor_items.contains(&159)); // fifteen: anyone sells it
        assert!(q.name(9).is_none());
    }

    #[test]
    fn split_values_join() {
        let whole = enc(&[0x63, b'a', b'b', b'c']);
        let (a, b) = whole.split_at(3);
        let toc = format!("## X-Quest-7-S: ~2~\n## X-Quest-7-S-1: {a}\n## X-Quest-7-S-2: {b}\n");
        let db = QuestieDb::parse(&toc);
        assert_eq!(db.field("Quest", 7, "S"), Some(cbor::Value::Text("abc".into())));
        assert_eq!(db.ids_with("Quest", "S"), [7]);
    }
}
