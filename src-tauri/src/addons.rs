//! The Addons screen, read-only (F4, IMPLEMENTING.md §9): every addon in
//! `Interface/AddOns` with what its TOC says, and which characters have it
//! on per their `AddOns.txt`. Nothing here writes; toggling and installing
//! come later, through the write gate.
//!
//! TOC text is the addon author's, so it's untrusted: colour codes and
//! texture tags are stripped, files over `TOC_MAX` are skipped, and the UI
//! renders all of it as text.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fsx::read::safe_read;
use crate::install::layout::Flavor;
use crate::sessions;

/// A real TOC is a few KB; anything bigger isn't read.
const TOC_MAX: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AddonsList {
    /// The flavor folder, e.g. `_classic_beta_`, and its display name
    /// ("WoW: Forever (Beta)"), for the lede.
    pub flavor: String,
    pub game: String,
    /// The flavor's `Interface/AddOns` (full path).
    pub folder: String,
    /// The interface number this client reads (`16001` on Forever), from its
    /// version; `None` when the version isn't known.
    pub interface: Option<u32>,
    /// The characters the "Enabled for" columns are for, in the Characters
    /// screen's order (the WTF roster, older settings folders left out).
    pub characters: Vec<AddonCharacter>,
    /// Sorted by title.
    pub addons: Vec<AddonInfo>,
    /// When this was read (RFC 3339).
    pub read_at: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AddonCharacter {
    pub account: String,
    /// The group folder (Forever's opaque id, or a realm).
    pub group: String,
    /// The character folder, e.g. `Ellygie-Vargur`.
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AddonInfo {
    /// The folder name: the addon's name to WoW and in `AddOns.txt`.
    pub name: String,
    /// `## Title`, plain text; the folder name if there's none.
    pub title: String,
    pub version: Option<String>,
    pub author: Option<String>,
    /// `## Notes`, plain text.
    pub notes: Option<String>,
    /// Every value in `## Interface`.
    pub interfaces: Vec<u32>,
    /// Every listed interface is below the client's.
    pub out_of_date: bool,
    /// `## Dependencies` / `## RequiredDeps` (and other `Dep…` fields).
    pub needs: Vec<String>,
    /// The full path of its folder.
    pub path: String,
    /// One per `AddonsList::characters`, in that order.
    pub enabled: Vec<bool>,
}

/// `1.60.1.70009` → 16001: WoW's interface number is major·10000 +
/// minor·100 + patch (Classic Era `1.15.4` is 11504, Retail `12.0.1` 120001).
pub fn interface_of(version: &str) -> Option<u32> {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
    let (major, minor, patch) = (parts.next()??, parts.next()??, parts.next()??);
    Some(major * 10_000 + minor * 100 + patch)
}

/// The TOC suffixes this client prefers over a plain `<Folder>.toc`, most
/// preferred first (warcraft.wiki.gg "TOC format": game type, then family).
fn toc_suffixes(flavor: &Flavor) -> &'static [&'static str] {
    if flavor.is_forever {
        return &["_Camelot", "_Mainline"];
    }
    match flavor.product.as_deref() {
        Some("wow" | "wowt" | "wow_beta") => &["_Standard", "_Mainline"],
        Some("wow_classic_era" | "wow_classic_era_ptr") => &["_Vanilla", "_Classic"],
        Some("wow_classic" | "wow_classic_ptr") => &["_Mists", "_Classic"],
        _ => &[],
    }
}

/// Removes WoW markup from TOC text: colour codes (`|cffRRGGBB`, `|r`),
/// texture and atlas tags (`|T…|t`, `|A…|a`), and `||` for a literal bar.
pub fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '|' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('c') => {
                // |cAARRGGBB: eight hex digits.
                for _ in 0..8 {
                    chars.next_if(|h| h.is_ascii_hexdigit());
                }
            }
            Some('r') => {}
            Some(open @ ('T' | 'A')) => {
                let close = if open == 'T' { 't' } else { 'a' };
                while let Some(x) = chars.next() {
                    if x == '|' && chars.next_if_eq(&close).is_some() {
                        break;
                    }
                }
            }
            Some('|') => out.push('|'),
            Some(other) => {
                out.push('|');
                out.push(other);
            }
            None => out.push('|'),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The `## Key: value` fields of a TOC, keys lower-cased, first one wins.
fn fields(toc: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in toc.lines() {
        let Some((key, value)) = line.strip_prefix("##").and_then(|l| l.split_once(':')) else {
            continue;
        };
        let key = key.trim().to_lowercase();
        if !out.iter().any(|(k, _)| *k == key) {
            out.push((key, value.trim().to_string()));
        }
    }
    out
}

/// The TOC the game would load for `folder`: the first preferred suffix,
/// else `<Folder>.toc`. Names match case-insensitively, as on Windows.
fn toc_for(dir: &Path, folder: &str, suffixes: &[&str]) -> Option<PathBuf> {
    let files: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| (e.file_name().to_string_lossy().to_lowercase(), e.path()))
        .collect();
    let want = |suffix: &str| format!("{folder}{suffix}.toc").to_lowercase();
    suffixes.iter().chain(std::iter::once(&"")).find_map(|s| {
        files
            .iter()
            .find(|(n, _)| *n == want(s))
            .map(|(_, p)| p.clone())
    })
}

/// One addon folder and whether it's on by default (`## DefaultState`), or
/// `None` if it has no TOC (not an addon to WoW).
fn read_addon(
    dir: &Path,
    folder: &str,
    suffixes: &[&str],
    interface: Option<u32>,
) -> Option<(AddonInfo, bool)> {
    let toc_path = toc_for(dir, folder, suffixes)?;
    if std::fs::metadata(&toc_path).ok()?.len() > TOC_MAX {
        return None;
    }
    let bytes = safe_read(&toc_path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let f = fields(&text);
    let get = |k: &str| {
        f.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| plain(v))
            .filter(|v| !v.is_empty())
    };
    let interfaces: Vec<u32> = get("interface")
        .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
        .unwrap_or_default();
    let out_of_date = interface
        .is_some_and(|game| !interfaces.is_empty() && interfaces.iter().all(|&i| i < game));
    let mut needs: Vec<String> = f
        .iter()
        .filter(|(k, _)| k.starts_with("dep") || k == "requireddeps")
        .flat_map(|(_, v)| v.split(',').map(plain).collect::<Vec<_>>())
        .filter(|d| !d.is_empty())
        .collect();
    needs.dedup();
    let default_on = !get("defaultstate").is_some_and(|s| s.eq_ignore_ascii_case("disabled"));
    let info = AddonInfo {
        name: folder.to_string(),
        title: get("title").unwrap_or_else(|| folder.to_string()),
        version: get("version"),
        author: get("author"),
        notes: get("notes"),
        interfaces,
        out_of_date,
        needs,
        path: dir.to_string_lossy().into_owned(),
        enabled: Vec::new(),
    };
    Some((info, default_on))
}

/// `Name: enabled|disabled` lines of one `AddOns.txt`, names lower-cased.
fn addons_txt(path: &Path) -> Vec<(String, bool)> {
    let Ok(bytes) = safe_read(path) else {
        return Vec::new();
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| {
            let (name, state) = line.split_once(':')?;
            Some((
                name.trim().to_lowercase(),
                state.trim().eq_ignore_ascii_case("enabled"),
            ))
        })
        .collect()
}

/// Every addon in the flavor's `Interface/AddOns`, with each character's
/// on/off from its `AddOns.txt` (no line: the TOC's `## DefaultState`,
/// enabled if unset). `Blizzard_*` folders and folders without a TOC are
/// left out.
pub fn list(flavor: &Flavor) -> AddonsList {
    let addons_dir = flavor.dir.join("Interface").join("AddOns");
    let interface = flavor.version.as_deref().and_then(interface_of);
    let suffixes = toc_suffixes(flavor);

    let roster: Vec<_> = sessions::wtf_characters(&flavor.dir.join("WTF"))
        .into_iter()
        .filter(|c| !c.older)
        .map(|c| c.character)
        .collect();
    let states: Vec<Vec<(String, bool)>> = roster
        .iter()
        .map(|r| {
            addons_txt(
                &flavor
                    .dir
                    .join("WTF")
                    .join("Account")
                    .join(&r.account)
                    .join(&r.realm)
                    .join(&r.name)
                    .join("AddOns.txt"),
            )
        })
        .collect();

    let mut seen = HashSet::new();
    let mut addons: Vec<AddonInfo> = std::fs::read_dir(&addons_dir)
        .into_iter()
        .flatten()
        .flatten()
        // `is_dir` through the metadata, so a linked addon folder (a
        // developer's junction) counts too; it's only read.
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let folder = e.file_name().to_string_lossy().into_owned();
            if folder.starts_with('.') || folder.to_lowercase().starts_with("blizzard_") {
                return None;
            }
            if !seen.insert(folder.to_lowercase()) {
                return None;
            }
            let (mut addon, default_on) = read_addon(&e.path(), &folder, suffixes, interface)?;
            let key = folder.to_lowercase();
            addon.enabled = states
                .iter()
                .map(|lines| {
                    lines
                        .iter()
                        .find(|(n, _)| *n == key)
                        .map_or(default_on, |(_, on)| *on)
                })
                .collect();
            Some(addon)
        })
        .collect();
    addons.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });

    AddonsList {
        flavor: flavor.id.clone(),
        game: flavor.label.clone(),
        folder: addons_dir.to_string_lossy().into_owned(),
        interface,
        characters: roster
            .into_iter()
            .map(|r| AddonCharacter {
                account: r.account,
                group: r.realm,
                folder: r.name,
            })
            .collect(),
        addons,
        read_at: chrono::Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn forever(dir: &Path) -> Flavor {
        Flavor {
            id: "_classic_beta_".into(),
            label: "WoW: Forever (Beta)".into(),
            product: Some("wow_classic_beta".into()),
            version: Some("1.60.1.70009".into()),
            is_forever: true,
            dir: dir.to_path_buf(),
            exe: None,
            has_wtf: true,
            accounts: vec!["A".into()],
            characters: 2,
            links: Vec::new(),
        }
    }

    /// A Forever folder: two characters, a few addons covering the §9 rules.
    fn game() -> (tempfile::TempDir, Flavor) {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let wtf = "WTF/Account/A/70";
        write(d, &format!("{wtf}/Thrandor/SavedVariables/x.lua"), "");
        write(
            d,
            &format!("{wtf}/Velyra-Duskmane/AddOns.txt"),
            "Questie: disabled\nquiet: enabled\n",
        );
        // Velyra's folder from before surnames: never a column (W1b).
        write(
            d,
            "WTF/Account/A/Classic Beta PvP 2/Velyra/AddOns.txt",
            "Questie: enabled\n",
        );
        let a = "Interface/AddOns";
        write(
            d,
            &format!("{a}/Questie/Questie.toc"),
            "## Interface: 11504, 16001\n## Title: |cff00ff00Questie|r\n## Notes: Shows |TInterface\\Icons\\x:0|tquests\n## Author: Aero\n## Version: 10.3.0\n",
        );
        // Several TOCs: Forever loads _Camelot over _Mainline and the plain one.
        write(
            d,
            &format!("{a}/Details/Details.toc"),
            "## Interface: 11504\n## Title: Details (plain)\n",
        );
        write(
            d,
            &format!("{a}/Details/Details_Mainline.toc"),
            "## Interface: 120001\n## Title: Details (mainline)\n",
        );
        write(
            d,
            &format!("{a}/Details/Details_Camelot.toc"),
            "## Interface: 16001\n## Title: Details!\n## Dependencies: LibStub, Ace3\n",
        );
        write(
            d,
            &format!("{a}/OldThing/OldThing.toc"),
            "## Interface: 11502\n",
        );
        write(
            d,
            &format!("{a}/Quiet/Quiet.toc"),
            "## Interface: 16001\n## DefaultState: disabled\n",
        );
        write(
            d,
            &format!("{a}/Blizzard_Thing/Blizzard_Thing.toc"),
            "## Interface: 16001\n",
        );
        write(d, &format!("{a}/NotAnAddon/readme.txt"), "");
        let flavor = forever(d);
        (tmp, flavor)
    }

    #[test]
    fn addons_are_read_per_the_spec() {
        let (_tmp, flavor) = game();
        let l = list(&flavor);
        assert_eq!(l.interface, Some(16001));
        assert!(l.folder.ends_with("AddOns"));
        let names: Vec<&str> = l.addons.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            ["Details", "OldThing", "Questie", "Quiet"],
            "by title; no Blizzard_, no TOC-less folder"
        );
        assert_eq!(
            l.characters
                .iter()
                .map(|c| c.folder.as_str())
                .collect::<Vec<_>>()
                .len(),
            2,
            "older settings folder left out"
        );

        let details = &l.addons[0];
        assert_eq!(details.title, "Details!", "the _Camelot TOC");
        assert_eq!(details.needs, ["LibStub", "Ace3"]);
        assert!(!details.out_of_date);

        let questie = l.addons.iter().find(|a| a.name == "Questie").unwrap();
        assert_eq!(questie.title, "Questie", "colour codes stripped");
        assert_eq!(
            questie.notes.as_deref(),
            Some("Shows quests"),
            "texture tag stripped"
        );
        assert_eq!(questie.interfaces, [11504, 16001]);
        assert!(!questie.out_of_date, "one listed interface is current");

        assert!(
            l.addons
                .iter()
                .find(|a| a.name == "OldThing")
                .unwrap()
                .out_of_date
        );
    }

    #[test]
    fn enabled_state_comes_from_addons_txt_then_default_state() {
        let (_tmp, flavor) = game();
        let l = list(&flavor);
        let col = |folder: &str| {
            l.characters
                .iter()
                .position(|c| c.folder == folder)
                .unwrap()
        };
        let (thrandor, velyra) = (col("Thrandor"), col("Velyra-Duskmane"));
        let on = |name: &str| {
            l.addons
                .iter()
                .find(|a| a.name == name)
                .unwrap()
                .enabled
                .clone()
        };
        // Velyra's AddOns.txt turns Questie off; Thrandor has no AddOns.txt.
        assert!(on("Questie")[thrandor] && !on("Questie")[velyra]);
        // DefaultState: disabled, unless AddOns.txt says otherwise (any case).
        assert!(!on("Quiet")[thrandor] && on("Quiet")[velyra]);
    }

    #[test]
    fn interface_numbers_and_markup() {
        assert_eq!(interface_of("1.60.1.70009"), Some(16001));
        assert_eq!(interface_of("1.15.4.56000"), Some(11504));
        assert_eq!(interface_of("12.0.1"), Some(120001));
        assert_eq!(interface_of("x"), None);
        assert_eq!(plain("|cFFff8000Big|r ||  bar"), "Big | bar");
        assert_eq!(plain("a|Aatlas:1|ab"), "ab");
        assert_eq!(plain("trailing |"), "trailing |");
    }

    #[test]
    fn no_addons_folder_is_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        let l = list(&forever(tmp.path()));
        assert!(l.addons.is_empty() && l.characters.is_empty());
    }
}
