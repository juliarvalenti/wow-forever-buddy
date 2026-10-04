//! The install model and the data-driven flavor table (spec §1).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::fsx::relpath::LinkedFolder;

/// A WoW root folder and the game flavors found in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Install {
    pub root: PathBuf,
    pub flavors: Vec<Flavor>,
    /// Folder name of the active flavor, once one is chosen.
    pub active: Option<String>,
}

impl Install {
    pub fn flavor(&self, id: &str) -> Option<&Flavor> {
        self.flavors.iter().find(|f| f.id == id)
    }

    pub fn active_flavor(&self) -> Option<&Flavor> {
        self.active.as_deref().and_then(|id| self.flavor(id))
    }
}

/// One `_<name>_` folder under the root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Flavor {
    /// Folder name, e.g. "_classic_beta_".
    pub id: String,
    /// Display name, e.g. "WoW: Forever (Beta)".
    pub label: String,
    /// Battle.net product code from `.build.info`, e.g. "wow_classic_beta".
    pub product: Option<String>,
    /// Client version from `.build.info`, e.g. "1.60.0.63105".
    pub version: Option<String>,
    pub is_forever: bool,
    pub dir: PathBuf,
    pub exe: Option<PathBuf>,
    pub has_wtf: bool,
    /// Account folder names under `WTF/Account`.
    pub accounts: Vec<String>,
    /// Realm (on Forever, ruleset) folder names across all accounts, for
    /// "7 characters on Ashenvale".
    pub realms: Vec<String>,
    /// Character folders under `WTF/Account/<account>/<realm>/`.
    pub characters: u32,
    /// `WTF` or `Interface/AddOns` when they're symlinks or junctions, as
    /// recorded by `GameRoot`. Allowed, and shown as info ("WTF is linked
    /// to D:\Sync\WTF").
    pub links: Vec<LinkedFolder>,
}

/// What we know about a flavor folder. Data, not code, so a new Forever
/// folder, product or exe at launch is a one-line change.
pub struct KnownFlavor {
    pub folder: &'static str,
    pub product: &'static str,
    /// Only matches when the client version starts with this. Blizzard reuses
    /// folders like `_classic_beta_` across betas, so the version tells them apart.
    pub version_prefix: Option<&'static str>,
    pub label: &'static str,
    /// Game exe names (Windows), most preferred first.
    pub exes: &'static [&'static str],
    pub is_forever: bool,
}

/// Checked in order; the first entry whose folder, product and version
/// prefix all match wins.
pub const KNOWN_FLAVORS: &[KnownFlavor] = &[
    // Forever is in beta until launch on 2026-11-04, under the generic
    // classic-beta folder and product. Interface 16001, build 1.60.x.
    KnownFlavor {
        folder: "_classic_beta_",
        product: "wow_classic_beta",
        version_prefix: Some("1.60."),
        label: "WoW: Forever (Beta)",
        exes: &["WowB.exe", "WowB-arm64.exe"],
        is_forever: true,
    },
    KnownFlavor {
        folder: "_classic_beta_",
        product: "wow_classic_beta",
        version_prefix: None,
        label: "Classic Beta",
        exes: &["WowB.exe", "WowB-arm64.exe"],
        is_forever: false,
    },
    KnownFlavor {
        folder: "_retail_",
        product: "wow",
        version_prefix: None,
        label: "Retail",
        exes: &["Wow.exe", "Wow-arm64.exe", "Wow-64.exe"],
        is_forever: false,
    },
    KnownFlavor {
        folder: "_classic_",
        product: "wow_classic",
        version_prefix: None,
        label: "Classic",
        exes: &["WowClassic.exe", "WowClassic-arm64.exe"],
        is_forever: false,
    },
    KnownFlavor {
        folder: "_classic_era_",
        product: "wow_classic_era",
        version_prefix: None,
        label: "Classic Era",
        exes: &["WowClassic.exe", "WowClassic-arm64.exe"],
        is_forever: false,
    },
    KnownFlavor {
        folder: "_ptr_",
        product: "wowt",
        version_prefix: None,
        label: "Retail PTR",
        exes: &["WowT.exe", "WowT-arm64.exe"],
        is_forever: false,
    },
    KnownFlavor {
        folder: "_xptr_",
        product: "wowxptr",
        version_prefix: None,
        label: "Retail XPTR",
        exes: &["WowT.exe", "WowT-arm64.exe"],
        is_forever: false,
    },
    KnownFlavor {
        folder: "_beta_",
        product: "wow_beta",
        version_prefix: None,
        label: "Retail Beta",
        exes: &["WowB.exe", "WowB-arm64.exe"],
        is_forever: false,
    },
    KnownFlavor {
        folder: "_classic_ptr_",
        product: "wow_classic_ptr",
        version_prefix: None,
        label: "Classic PTR",
        exes: &["WowClassicT.exe", "WowClassicT-arm64.exe"],
        is_forever: false,
    },
    KnownFlavor {
        folder: "_classic_era_ptr_",
        product: "wow_classic_era_ptr",
        version_prefix: None,
        label: "Classic Era PTR",
        exes: &["WowClassicT.exe", "WowClassicT-arm64.exe"],
        is_forever: false,
    },
];

/// Every known game exe name, for the game-running check's fallback when an
/// exe path can't be read (T6).
#[allow(dead_code)] // first caller: the process poller (T6)
pub fn known_exe_names() -> Vec<&'static str> {
    let mut names: Vec<&str> = Vec::new();
    for exe in KNOWN_FLAVORS.iter().flat_map(|k| k.exes) {
        if !names.contains(exe) {
            names.push(exe);
        }
    }
    names
}

/// Whether `product` belongs in `folder`: per the table, or by the naming
/// convention `_x_` ↔ `wow_x` for folders the table doesn't know yet.
pub fn product_matches_folder(product: &str, folder: &str) -> bool {
    let known: Vec<_> = KNOWN_FLAVORS
        .iter()
        .filter(|k| k.folder == folder)
        .collect();
    if known.is_empty() {
        product.strip_prefix("wow") == Some(folder.trim_end_matches('_'))
    } else {
        known.iter().any(|k| k.product == product)
    }
}

/// The table entry for a flavor folder, given what `.build.info` says.
pub fn lookup(
    folder: &str,
    product: Option<&str>,
    version: Option<&str>,
) -> Option<&'static KnownFlavor> {
    KNOWN_FLAVORS.iter().find(|k| {
        k.folder == folder
            && product.is_none_or(|p| p == k.product)
            && k.version_prefix
                .is_none_or(|prefix| version.is_some_and(|v| v.starts_with(prefix)))
    })
}

/// "_classic_era_" → "Classic Era", for folders the table doesn't know.
pub fn label_from_folder(folder: &str) -> String {
    folder
        .trim_matches('_')
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(first) => first.to_uppercase().chain(c).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A row of `.build.info`.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildRow {
    pub product: String,
    pub version: String,
    pub active: bool,
}

/// Parses `.build.info`: a pipe-separated table whose header cells look like
/// `Name!TYPE:size`. Missing columns or junk lines yield no rows, never an
/// error; labels just fall back to folder names.
pub fn parse_build_info(text: &str) -> Vec<BuildRow> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let columns: Vec<&str> = header
        .split('|')
        .map(|cell| cell.split('!').next().unwrap_or(cell).trim())
        .collect();
    let col = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (Some(product), Some(version)) = (col("Product"), col("Version")) else {
        return Vec::new();
    };
    let active = col("Active");

    lines
        .filter(|l| !l.starts_with('#'))
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').collect();
            let product = cells.get(product)?.trim();
            if product.is_empty() {
                return None;
            }
            Some(BuildRow {
                product: product.to_string(),
                version: cells.get(version).map_or("", |v| v.trim()).to_string(),
                active: active
                    .and_then(|i| cells.get(i))
                    .is_none_or(|a| a.trim() != "0"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_fixture_build_info() {
        let text = std::fs::read_to_string(crate::test_support::fixture_root().join(".build.info"))
            .unwrap();
        let rows = parse_build_info(&text);
        assert_eq!(
            rows,
            vec![
                BuildRow {
                    product: "wow_classic_beta".into(),
                    version: "1.60.0.63105".into(),
                    active: true
                },
                BuildRow {
                    product: "wow_classic_era".into(),
                    version: "1.15.7.61582".into(),
                    active: true
                },
            ]
        );
    }

    #[test]
    fn build_info_junk_yields_nothing() {
        assert!(parse_build_info("").is_empty());
        assert!(parse_build_info("not a table").is_empty());
        assert!(parse_build_info("Branch!STRING:0|Version!STRING:0\nus|1.0").is_empty());
        let rows = parse_build_info(
            "Product!STRING:0|Version!STRING:0|Active!DEC:1\r\nwow|11.0|0\r\n\r\n|1.0|1\n",
        );
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].active);
    }

    #[test]
    fn forever_is_told_apart_from_older_classic_betas() {
        let forever = lookup(
            "_classic_beta_",
            Some("wow_classic_beta"),
            Some("1.60.0.63105"),
        )
        .unwrap();
        assert!(forever.is_forever);
        assert_eq!(forever.label, "WoW: Forever (Beta)");

        let old = lookup(
            "_classic_beta_",
            Some("wow_classic_beta"),
            Some("3.4.0.46158"),
        )
        .unwrap();
        assert!(!old.is_forever);
        assert_eq!(old.label, "Classic Beta");

        // No .build.info: don't claim it's Forever.
        assert!(!lookup("_classic_beta_", None, None).unwrap().is_forever);
        assert!(lookup("_forever_", None, None).is_none());
    }

    #[test]
    fn products_map_to_folders() {
        assert!(product_matches_folder("wow", "_retail_"));
        assert!(product_matches_folder("wow_classic_beta", "_classic_beta_"));
        assert!(!product_matches_folder("wow_classic", "_classic_era_"));
        assert!(!product_matches_folder("wow", "_classic_"));
        // Unknown folder: naming convention.
        assert!(product_matches_folder("wow_forever", "_forever_"));
        assert!(!product_matches_folder("wow_other", "_forever_"));
    }

    #[test]
    fn labels_and_exe_names() {
        assert_eq!(label_from_folder("_forever_"), "Forever");
        assert_eq!(label_from_folder("_classic_era_ptr_"), "Classic Era Ptr");
        let names = known_exe_names();
        assert!(names.contains(&"WowB.exe") && names.contains(&"WowB-arm64.exe"));
        assert_eq!(names.iter().filter(|n| **n == "WowB.exe").count(), 1);
    }
}
