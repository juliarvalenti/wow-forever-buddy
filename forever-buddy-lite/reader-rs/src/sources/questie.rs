//! QuestieDB's item records, from `QuestieDB_Forever.toc`: item names, and which items vendors sell.
//!
//! The .toc carries its data as metadata lines, one per item and field, each a base64 CBOR value:
//! - `## X-Item-<id>-S: ...` a summary record whose entry 1 is the item's name;
//! - `## X-Item-<id>-14: ...` the list of NPCs that sell the item.

use crate::cbor;
use base64::Engine;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct QuestieItems {
    pub names: HashMap<u32, String>,
    /// Items sold by at least `vendor_cutoff` NPCs.
    pub vendor_items: HashSet<u32>,
}

/// Reads the .toc in `dir`; a missing file gives empty data (names then fall back to other sources).
///
/// `vendor_cutoff`: how many vendors make an item "unlimited". Limited-supply sellers (Darkmoon Faire
/// leather, a herbalist's few Peacebloom) are one to a handful of NPCs; goods every vendor stocks
/// without limit (water, thread, vials, arrows, spices) are sold by dozens. Those have a fixed price
/// ceiling, so the auction house can't be traded on them.
pub fn load(dir: &Path, vendor_cutoff: usize) -> QuestieItems {
    match std::fs::read_to_string(dir.join("QuestieDB_Forever.toc")) {
        Ok(text) => parse(&text, vendor_cutoff),
        Err(_) => QuestieItems::default(),
    }
}

pub fn parse(toc: &str, vendor_cutoff: usize) -> QuestieItems {
    let mut out = QuestieItems::default();
    for line in toc.lines() {
        let Some((id, field, payload)) = item_line(line) else { continue };
        let Some(value) = decode(payload) else { continue };
        match field {
            "S" => {
                if let Some(name) = value.get("1").and_then(cbor::Value::as_text) {
                    out.names.insert(id, name);
                }
            }
            "14" => {
                if matches!(&value, cbor::Value::Array(sellers) if sellers.len() >= vendor_cutoff) {
                    out.vendor_items.insert(id);
                }
            }
            _ => {}
        }
    }
    out
}

/// "## X-Item-25-S: <base64>" -> (25, "S", "<base64>").
fn item_line(line: &str) -> Option<(u32, &str, &str)> {
    let rest = line.strip_prefix("## X-Item-")?;
    let (head, payload) = rest.split_once(": ")?;
    let (id, field) = head.split_once('-')?;
    Some((id.parse().ok()?, field, payload))
}

fn decode(payload: &str) -> Option<cbor::Value> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload.trim()).ok()?;
    cbor::decode(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_vendor_counts() {
        let enc = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        // { [1] = "Worn Shortsword" } as a byte string, like the game writes it.
        let mut name = vec![0xa1, 0x01, 0x4f];
        name.extend(b"Worn Shortsword");
        let few_sellers = [0x82, 0x01, 0x02];
        let mut many_sellers = vec![0x8f];
        many_sellers.extend(1..=15u8);
        let toc = format!(
            "## Title: QuestieDB\n## X-Item-25-S: {}\n## X-Item-85-14: {}\n## X-Item-159-14: {}\n## X-Item-9-S: not base64!\n",
            enc(&name),
            enc(&few_sellers),
            enc(&many_sellers)
        );
        let q = parse(&toc, 15);
        assert_eq!(q.names.get(&25).map(String::as_str), Some("Worn Shortsword"));
        assert!(!q.vendor_items.contains(&85)); // two sellers: limited supply
        assert!(q.vendor_items.contains(&159)); // fifteen: anyone sells it
        assert!(!q.names.contains_key(&9));
    }
}
