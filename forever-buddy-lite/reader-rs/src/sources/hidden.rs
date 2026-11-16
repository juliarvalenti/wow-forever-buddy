//! Items you've hidden from the market picks (the dashboard's Hide button). A JSON file:
//! `{ "items": { "<id>": { "name": "...", "at": "<ISO time>" } } }`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HiddenItem {
    pub name: Option<String>,
    pub at: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HiddenFile {
    #[serde(default)]
    items: BTreeMap<String, HiddenItem>,
}

/// Hidden items by id, in id order. A missing or unreadable file hides nothing.
pub fn load(path: &Path) -> BTreeMap<u32, HiddenItem> {
    let Ok(text) = std::fs::read_to_string(path) else { return BTreeMap::new() };
    let Ok(file) = serde_json::from_str::<HiddenFile>(&text) else { return BTreeMap::new() };
    file.items.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect()
}

/// Hides (`on`) or shows an item again, and saves the list.
pub fn set(path: &Path, id: u32, name: Option<&str>, on: bool, now_iso: &str) -> std::io::Result<BTreeMap<u32, HiddenItem>> {
    let mut hidden = load(path);
    if on {
        hidden.insert(id, HiddenItem { name: name.map(String::from), at: Some(now_iso.to_string()) });
    } else {
        hidden.remove(&id);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = HiddenFile { items: hidden.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() };
    std::fs::write(path, serde_json::to_string_pretty(&file)?)?;
    Ok(hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_and_show() {
        let dir = std::env::temp_dir().join(format!("fbl-rs-hidden-{}", std::process::id()));
        let path = dir.join("hidden.json");
        assert!(load(&path).is_empty());
        set(&path, 2589, Some("Linen Cloth"), true, "2026-11-16T00:00:00.000Z").unwrap();
        set(&path, 25, None, true, "2026-11-16T00:00:00.000Z").unwrap();
        let h = load(&path);
        assert_eq!(h.keys().copied().collect::<Vec<_>>(), [25, 2589]);
        assert_eq!(h[&2589].name.as_deref(), Some("Linen Cloth"));
        set(&path, 2589, None, false, "").unwrap();
        assert_eq!(load(&path).len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
