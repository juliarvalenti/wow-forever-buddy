//! A snapshot's files grouped the way the restore panel shows them (spec §5):
//! account → character → category, each with size and file count, plus a
//! per-addon view for the "Addons" tab.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::backup::manifest::{Manifest, SkippedFile, SnapshotSummary};
use crate::install::wtf::older_groups;

/// What a file in a character (or account) folder is for. The restore panel
/// lets you pick these per character.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
pub enum Category {
    /// `bindings-cache.wtf`, `macros-cache.txt`
    BindingsMacros,
    /// `SavedVariables/**`, `AddOns.txt`
    AddonSettings,
    /// `chat-cache.txt`, `layout-local.txt`
    ChatLayout,
    /// Anything else, e.g. `config-cache.wtf`
    Other,
}

impl Category {
    /// `rest` is the path below the account or character folder.
    pub fn of(rest: &[&str]) -> Category {
        let first = rest
            .first()
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        match first.as_str() {
            "savedvariables" | "addons.txt" => Category::AddonSettings,
            "bindings-cache.wtf" | "macros-cache.txt" => Category::BindingsMacros,
            "chat-cache.txt" | "layout-local.txt" => Category::ChatLayout,
            _ => Category::Other,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Totals {
    pub files: u32,
    /// f64 so TypeScript can hold it safely.
    pub bytes: f64,
}

impl Totals {
    fn add(&mut self, bytes: u64) {
        self.files += 1;
        self.bytes += bytes as f64;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct CategoryNode {
    pub category: Category,
    pub totals: Totals,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct CharacterNode {
    /// The folder above the character: Forever's opaque group id (`70`) or,
    /// in older folders, the realm. Identity only, never shown.
    pub realm: String,
    /// The character folder as written (`Ellygie-Vargur`), never split.
    pub name: String,
    /// An older settings folder, the layout from before surnames
    /// (`install::wtf::older_groups`): listed apart, fully restorable.
    pub older: bool,
    pub totals: Totals,
    pub categories: Vec<CategoryNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct AccountNode {
    pub name: String,
    pub totals: Totals,
    /// Account-wide files (account SavedVariables, macros, bindings…).
    pub categories: Vec<CategoryNode>,
    pub characters: Vec<CharacterNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct AddonNode {
    /// SavedVariables file stem, e.g. "Details".
    pub name: String,
    /// Across every account and character.
    pub totals: Totals,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SnapshotDetail {
    pub summary: SnapshotSummary,
    pub accounts: Vec<AccountNode>,
    pub addons: Vec<AddonNode>,
    /// Files outside `WTF/Account` (e.g. `WTF/Config.wtf`, AddOns folders).
    pub other: Totals,
    /// Files the snapshot couldn't capture and left out (R1).
    pub skipped: Vec<SkippedFile>,
}

#[derive(Default)]
struct AccountBuild {
    totals: Totals,
    categories: BTreeMap<Category, Totals>,
    characters: BTreeMap<(String, String), (Totals, BTreeMap<Category, Totals>)>,
}

fn categories(map: BTreeMap<Category, Totals>) -> Vec<CategoryNode> {
    map.into_iter()
        .map(|(category, totals)| CategoryNode { category, totals })
        .collect()
}

pub fn detail(manifest: &Manifest) -> SnapshotDetail {
    let mut accounts: BTreeMap<String, AccountBuild> = BTreeMap::new();
    let mut addons: BTreeMap<String, (String, Totals)> = BTreeMap::new();
    let mut other = Totals::default();

    for file in &manifest.files {
        let parts: Vec<&str> = file.path.split('/').collect();
        let n = parts.len();

        if n >= 2 && parts[n - 2].eq_ignore_ascii_case("SavedVariables") {
            let name = parts[n - 1];
            if let Some(stem) = name
                .strip_suffix(".lua.bak")
                .or_else(|| name.strip_suffix(".lua"))
            {
                addons
                    .entry(stem.to_lowercase())
                    .or_insert_with(|| (stem.to_string(), Totals::default()))
                    .1
                    .add(file.size);
            }
        }

        let in_account = n >= 4
            && parts[0].eq_ignore_ascii_case("WTF")
            && parts[1].eq_ignore_ascii_case("Account");
        if !in_account {
            other.add(file.size);
            continue;
        }
        let account = accounts.entry(parts[2].to_string()).or_default();
        account.totals.add(file.size);

        let is_character = n >= 6 && Category::of(&parts[3..]) == Category::Other;
        if is_character {
            let (totals, cats) = account
                .characters
                .entry((parts[3].to_string(), parts[4].to_string()))
                .or_default();
            totals.add(file.size);
            cats.entry(Category::of(&parts[5..]))
                .or_default()
                .add(file.size);
        } else {
            account
                .categories
                .entry(Category::of(&parts[3..]))
                .or_default()
                .add(file.size);
        }
    }

    SnapshotDetail {
        summary: manifest.summary(),
        skipped: manifest.skipped.clone(),
        accounts: accounts
            .into_iter()
            .map(|(name, a)| {
                let older: HashSet<String> =
                    older_groups(a.characters.keys().map(|(g, _)| g.as_str()))
                        .into_iter()
                        .map(str::to_string)
                        .collect();
                AccountNode {
                    name,
                    totals: a.totals,
                    categories: categories(a.categories),
                    characters: a
                        .characters
                        .into_iter()
                        .map(|((realm, name), (totals, cats))| CharacterNode {
                            older: older.contains(&realm),
                            realm,
                            name,
                            totals,
                            categories: categories(cats),
                        })
                        .collect(),
                }
            })
            .collect(),
        addons: addons
            .into_values()
            .map(|(name, totals)| AddonNode { name, totals })
            .collect(),
        other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::manifest::tests::manifest;

    #[test]
    fn groups_by_account_character_and_category() {
        let m = manifest(
            "01J9ZZZZZZZZZZZZZZZZZZZZZZ",
            &[
                "WTF/Config.wtf",
                "WTF/Account/A1/SavedVariables/Details.lua",
                "WTF/Account/A1/SavedVariables/Details.lua.bak",
                "WTF/Account/A1/macros-cache.txt",
                "WTF/Account/A1/config-cache.wtf",
                "WTF/Account/A1/Ashenvale/Thrandor/AddOns.txt",
                "WTF/Account/A1/Ashenvale/Thrandor/SavedVariables/Details.lua",
                "WTF/Account/A1/Ashenvale/Thrandor/bindings-cache.wtf",
                "WTF/Account/A1/Ashenvale/Thrandor/macros-cache.txt",
                "WTF/Account/A1/Ashenvale/Thrandor/layout-local.txt",
                "WTF/Account/A1/Pyrewood Village/Lúthien/chat-cache.txt",
            ],
        );
        let d = detail(&m);

        assert_eq!(d.other.files, 1);
        assert_eq!(d.accounts.len(), 1);
        let a1 = &d.accounts[0];
        assert_eq!(a1.name, "A1");
        assert_eq!(a1.totals.files, 10);
        let account_cats: Vec<(Category, u32)> = a1
            .categories
            .iter()
            .map(|c| (c.category, c.totals.files))
            .collect();
        assert_eq!(
            account_cats,
            [
                (Category::BindingsMacros, 1),
                (Category::AddonSettings, 2),
                (Category::Other, 1)
            ]
        );

        let thrandor = a1.characters.iter().find(|c| c.name == "Thrandor").unwrap();
        assert_eq!(thrandor.realm, "Ashenvale");
        assert_eq!(thrandor.totals.files, 5);
        let cats: Vec<(Category, u32)> = thrandor
            .categories
            .iter()
            .map(|c| (c.category, c.totals.files))
            .collect();
        assert_eq!(
            cats,
            [
                (Category::BindingsMacros, 2),
                (Category::AddonSettings, 2),
                (Category::ChatLayout, 1)
            ]
        );
        assert!(a1.characters.iter().any(|c| c.name == "Lúthien"));

        let details = d.addons.iter().find(|a| a.name == "Details").unwrap();
        assert_eq!(
            details.totals.files, 3,
            "account, .bak and character copies"
        );
    }

    /// Probe run 1: Forever's `<group id>/<First>-<Surname>` next to the
    /// older `<Realm>/<Name>`. The folders are kept whole (the restore
    /// selection matches on them). W1b: next to the group-id layout, every
    /// realm-named folder is an older settings folder: marked, restorable,
    /// not counted.
    #[test]
    fn groups_both_wtf_layouts() {
        let m = manifest(
            "01J9ZZZZZZZZZZZZZZZZZZZZZZ",
            &[
                "WTF/Account/A1/70/Ellygie-Vargur/AddOns.txt",
                "WTF/Account/A1/70/Ellygie-Vargur/SavedVariables/Details.lua",
                "WTF/Account/A1/70/Brannic/macros-cache.txt",
                "WTF/Account/A1/Classic Beta PvP 2/Ellygie/AddOns.txt",
                "WTF/Account/A1/Classic Beta PvP 2/Sela/AddOns.txt",
            ],
        );
        let d = detail(&m);
        let chars: Vec<(&str, &str, u32, bool)> = d.accounts[0]
            .characters
            .iter()
            .map(|c| (c.realm.as_str(), c.name.as_str(), c.totals.files, c.older))
            .collect();
        assert_eq!(
            chars,
            [
                ("70", "Brannic", 1, false),
                ("70", "Ellygie-Vargur", 2, false),
                ("Classic Beta PvP 2", "Ellygie", 1, true),
                ("Classic Beta PvP 2", "Sela", 1, true),
            ]
        );
        assert_eq!(m.char_count(), 2, "Brannic and Ellygie-Vargur");
    }
}
