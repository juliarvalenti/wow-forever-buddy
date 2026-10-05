//! The Addons screen (F4, IMPLEMENTING.md §9): every addon in
//! `Interface/AddOns` with what its TOC says, and which characters have it
//! on per their `AddOns.txt`.
//!
//! The one write is F6's toggle, which rewrites a character's `AddOns.txt`
//! (never anything in `Interface/AddOns`) through the write gate, and can be
//! undone from its safety snapshot. Installing and removing addons come
//! later; they must refuse linked addon folders, as V4 does.
//!
//! TOC text is the addon author's, so it's untrusted: colour codes and
//! texture tags are stripped, files over `TOC_MAX` are skipped, and the UI
//! renders all of it as text.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::backup::manifest::{Manifest, Trigger};
use crate::error::{AppError, AppResult};
use crate::fsx::read::safe_read;
use crate::fsx::relpath::{GameRoot, RelPath};
use crate::game::gate::{MutationTarget, WriteGate};
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
    /// Its settings folder is a link to somewhere else, so the write gate
    /// won't write its AddOns.txt: "linked folder" instead of a switch.
    pub linked: bool,
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
    let root = GameRoot::new(&flavor.dir).ok();
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
            .map(|r| {
                let key = CharacterKey {
                    account: r.account,
                    group: r.realm,
                    folder: r.name,
                };
                // The write gate's own test, asked in advance: a settings
                // folder that resolves outside WTF is a link it refuses.
                let linked = root.as_ref().is_some_and(|root| {
                    matches!(
                        addons_txt_rel(&key).and_then(|p| p.resolve(root)),
                        Err(AppError::PathEscape(_))
                    )
                });
                AddonCharacter {
                    account: key.account,
                    group: key.group,
                    folder: key.folder,
                    linked,
                }
            })
            .collect(),
        addons,
        read_at: chrono::Utc::now().to_rfc3339(),
    }
}

// ---- F6: turning an addon on or off per character -------------------------

/// `AddOns.txt` with `name`'s line set to `on`: every matching line (any
/// case) rewritten as `Name: enabled|disabled`, or one appended if there's
/// none. Every other line stays byte for byte, in order, and CRLF files stay
/// CRLF.
fn with_state(existing: &[u8], name: &str, on: bool) -> Vec<u8> {
    let state = if on { "enabled" } else { "disabled" };
    let crlf = existing.windows(2).any(|w| w == b"\r\n");
    let eol: &[u8] = if crlf { b"\r\n" } else { b"\n" };
    let mut out = Vec::with_capacity(existing.len() + name.len() + 12);
    let mut found = false;
    let mut lines = existing.split(|&b| b == b'\n').peekable();
    while let Some(raw) = lines.next() {
        // The empty piece after a final newline isn't a line.
        if raw.is_empty() && lines.peek().is_none() {
            break;
        }
        let line = raw.strip_suffix(b"\r").unwrap_or(raw);
        let text = String::from_utf8_lossy(line);
        let ours = text
            .split_once(':')
            .is_some_and(|(n, _)| n.trim().eq_ignore_ascii_case(name));
        if ours {
            out.extend_from_slice(format!("{name}: {state}").as_bytes());
            found = true;
        } else {
            out.extend_from_slice(line);
        }
        out.extend_from_slice(eol);
    }
    if !found {
        out.extend_from_slice(format!("{name}: {state}").as_bytes());
        out.extend_from_slice(eol);
    }
    out
}

/// A character the Addons screen shows, as the UI names it back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
pub struct CharacterKey {
    pub account: String,
    pub group: String,
    pub folder: String,
}

/// One staged switch: turn `addon` on or off for `character`.
#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
pub struct AddonChange {
    pub addon: String,
    pub character: CharacterKey,
    pub enabled: bool,
}

/// What Apply did, for "4 changes applied. A safety snapshot was taken
/// first." with Undo.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ToggleResult {
    /// The safety snapshot taken first; `None` if every file already said
    /// so and nothing was written.
    pub snapshot_id: Option<String>,
    /// Changes that took effect (ones already so are left out).
    pub applied: u32,
}

fn addons_txt_rel(c: &CharacterKey) -> AppResult<RelPath> {
    RelPath::new(&format!(
        "WTF/Account/{}/{}/{}/AddOns.txt",
        c.account, c.group, c.folder
    ))
}

/// Applies staged switches (IMPLEMENTING.md §11) through the write gate:
/// refused while WoW runs, one safety snapshot of exactly the AddOns.txt
/// files that change, each replaced atomically with all of its changes.
///
/// Nothing from the UI is trusted: each addon must be one the list shows
/// (a folder with a TOC, so no `:` or line break can reach the file), and
/// each character must be on the WTF roster with older settings folders
/// left out; the paths are built from those, never taken as given. A
/// character whose settings folder is a link is refused, as the list says
/// in advance. One bad change refuses the whole set, before any write.
pub fn apply(
    gate: &WriteGate,
    target: &MutationTarget,
    flavor: &Flavor,
    changes: &[AddonChange],
) -> AppResult<ToggleResult> {
    let listed = list(flavor);
    // Everything is checked before anything is written: one bad change
    // refuses the whole set.
    struct Edit<'a> {
        col: usize,
        addon: &'a AddonInfo,
        on: bool,
    }
    let mut edits: Vec<Edit> = Vec::new();
    for ch in changes {
        let Some(addon) = listed.addons.iter().find(|a| a.name == ch.addon) else {
            return Err(AppError::NotFound(format!("addon {:?}", ch.addon)));
        };
        if addon.name.chars().any(|c| c == ':' || c.is_control()) {
            return Err(AppError::NotFound(format!("addon {:?}", ch.addon)));
        }
        let c = &ch.character;
        let Some(col) = listed
            .characters
            .iter()
            .position(|r| r.account == c.account && r.group == c.group && r.folder == c.folder)
        else {
            return Err(AppError::NotFound(format!("character {:?}", c.folder)));
        };
        if listed.characters[col].linked {
            return Err(AppError::PathEscape(format!(
                "{}'s settings folder is a link",
                c.folder
            )));
        }
        // Already so (by its line or the TOC's default): nothing to write.
        if addon.enabled[col] != ch.enabled {
            edits.push(Edit {
                col,
                addon,
                on: ch.enabled,
            });
        }
    }

    // One write per character's file, with all of its changes folded in.
    let mut cols: Vec<usize> = edits.iter().map(|e| e.col).collect();
    cols.sort_unstable();
    cols.dedup();
    let mut writes: Vec<(RelPath, Vec<u8>)> = Vec::new();
    for col in cols {
        let r = &listed.characters[col];
        let rel = addons_txt_rel(&CharacterKey {
            account: r.account.clone(),
            group: r.group.clone(),
            folder: r.folder.clone(),
        })?;
        let mut bytes = match rel.resolve(&target.game) {
            Ok(path) if path.is_file() => safe_read(&path)?,
            Ok(_) => Vec::new(),
            Err(e) => return Err(e),
        };
        for e in edits.iter().filter(|e| e.col == col) {
            bytes = with_state(&bytes, &e.addon.name, e.on);
        }
        writes.push((rel, bytes));
    }
    if writes.is_empty() {
        return Ok(ToggleResult {
            snapshot_id: None,
            applied: 0,
        });
    }

    let label = match edits.as_slice() {
        [one] => format!(
            "Before turning {} {} for {}",
            one.addon.title,
            if one.on { "on" } else { "off" },
            listed.characters[one.col].folder.replace('-', " ")
        ),
        many => format!("Before {} addon changes", many.len()),
    };
    let paths: Vec<RelPath> = writes.iter().map(|(p, _)| p.clone()).collect();
    let guard = gate.begin("addons_toggle", target, &paths, &label)?;
    for (path, bytes) in &writes {
        guard.write(path, bytes)?;
    }
    let snapshot_id = guard.snapshot_id().to_string();
    guard.commit()?;
    Ok(ToggleResult {
        snapshot_id: Some(snapshot_id),
        applied: edits.len() as u32,
    })
}

/// `WTF/Account/<account>/<group>/<character>/AddOns.txt`, and nothing else.
fn is_addons_txt(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    matches!(parts.as_slice(), ["WTF", "Account", _, _, _, "AddOns.txt"])
}

/// Undoes a toggle: puts back exactly the AddOns.txt files its safety
/// snapshot holds (and removes any it created), through the write gate with
/// a snapshot of its own. Refused for any snapshot that isn't a toggle's
/// (one holding anything but AddOns.txt files) or that was taken in another
/// flavor than the one `target` writes into.
pub fn undo(
    gate: &WriteGate,
    target: &MutationTarget,
    manifest: &Manifest,
    blob: impl Fn(&str) -> AppResult<Vec<u8>>,
) -> AppResult<()> {
    let holds_something = !manifest.files.is_empty() || !manifest.absent.is_empty();
    // The paths are relative to a flavor folder: a snapshot taken in another
    // flavor would write its characters' AddOns.txt into this flavor's
    // characters at the same paths. The flavor is the folder being written to.
    let flavor = target
        .game
        .base
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    if !holds_something
        || flavor.as_deref() != Some(manifest.flavor.as_str())
        || manifest.trigger != Trigger::PreWrite
        || !manifest.files.iter().all(|f| is_addons_txt(&f.path))
        || !manifest.absent.iter().all(|p| is_addons_txt(p))
    {
        return Err(AppError::NotFound(format!("addon change {}", manifest.id)));
    }
    let restore: Vec<(RelPath, Vec<u8>)> = manifest
        .files
        .iter()
        .map(|f| Ok((RelPath::new(&f.path)?, blob(&f.blake3)?)))
        .collect::<AppResult<_>>()?;
    let remove: Vec<RelPath> = manifest
        .absent
        .iter()
        .map(|p| RelPath::new(p))
        .collect::<AppResult<_>>()?;
    let paths: Vec<RelPath> = restore
        .iter()
        .map(|(p, _)| p.clone())
        .chain(remove.iter().cloned())
        .collect();
    let guard = gate.begin(
        "addons_toggle_undo",
        target,
        &paths,
        "Before undoing an addon change",
    )?;
    for (path, bytes) in &restore {
        guard.write(path, bytes)?;
    }
    for path in &remove {
        guard.remove(path)?;
    }
    guard.commit()
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

    // ---- F6 toggles ------------------------------------------------------

    #[test]
    fn only_the_addons_line_changes() {
        let before = b"Details: enabled\nWeakAuras: enabled\nquestie: disabled\nX: enabled\n";
        assert_eq!(
            with_state(before, "Questie", true),
            b"Details: enabled\nWeakAuras: enabled\nQuestie: enabled\nX: enabled\n",
            "any case, in place, order kept"
        );
        assert_eq!(
            with_state(b"Details: enabled\r\n", "Questie", false),
            b"Details: enabled\r\nQuestie: disabled\r\n",
            "appended, CRLF kept"
        );
        assert_eq!(with_state(b"", "Questie", false), b"Questie: disabled\n");
        assert_eq!(
            with_state(b"Details: enabled", "Questie", true),
            b"Details: enabled\nQuestie: enabled\n",
            "a last line without a newline gets one"
        );
        // A line that only starts with the name isn't the addon's.
        assert_eq!(
            with_state(b"QuestieX: enabled\n", "Questie", false),
            b"QuestieX: enabled\nQuestie: disabled\n"
        );
    }

    /// The real app pieces over the fixture game folder: the write gate with
    /// the backup store's safety snapshots.
    struct Game {
        _dir: tempfile::TempDir,
        root: PathBuf,
        core: crate::state::AppCore,
        probe: std::sync::Arc<crate::game::process::fake::FakeProbe>,
    }

    impl Game {
        fn new() -> Game {
            use crate::config::paths::AppPaths;
            use crate::game::process::fake::FakeProbe;
            use crate::secrets::MemoryStore;
            use std::sync::Arc;

            let (dir, root) = crate::test_support::fixture_copy();
            let probe = Arc::new(FakeProbe::default());
            let core = crate::state::AppCore::with_parts(
                AppPaths::under(&dir.path().join("app")),
                Arc::new(MemoryStore::default()),
                probe.clone(),
            )
            .unwrap();
            crate::install::set(&core.settings, &root, Some("_classic_beta_")).unwrap();
            write(
                &root.join("_classic_beta_"),
                "Interface/AddOns/Questie/Questie.toc",
                "## Interface: 16001\n## Title: Questie\n",
            );
            Game {
                _dir: dir,
                root,
                core,
                probe,
            }
        }

        /// One addon switched the same way for `who`.
        fn set(
            &self,
            addon: &str,
            who: &[(&str, &str, &str)],
            on: bool,
        ) -> AppResult<ToggleResult> {
            let changes: Vec<(&str, (&str, &str, &str), bool)> =
                who.iter().map(|&c| (addon, c, on)).collect();
            self.apply(&changes)
        }

        fn apply(&self, changes: &[(&str, (&str, &str, &str), bool)]) -> AppResult<ToggleResult> {
            let install = crate::install::current(&self.core.settings)
                .unwrap()
                .unwrap();
            let changes: Vec<AddonChange> = changes
                .iter()
                .map(|&(addon, (a, g, f), enabled)| AddonChange {
                    addon: addon.to_string(),
                    character: CharacterKey {
                        account: a.to_string(),
                        group: g.to_string(),
                        folder: f.to_string(),
                    },
                    enabled,
                })
                .collect();
            apply(
                &self.core.write_gate().unwrap(),
                &self.core.mutation_target().unwrap(),
                install.active_flavor().unwrap(),
                &changes,
            )
        }

        fn undo(&self, id: &str) -> AppResult<()> {
            let store = self.core.backups().unwrap();
            undo(
                &self.core.write_gate().unwrap(),
                &self.core.mutation_target().unwrap(),
                &store.manifest(id).unwrap(),
                |h| store.blobs().get(h),
            )
        }

        fn txt(&self, account: &str, character: &str) -> Option<String> {
            let p = self
                .root
                .join("_classic_beta_/WTF/Account")
                .join(account)
                .join("Ashenvale")
                .join(character)
                .join("AddOns.txt");
            std::fs::read_to_string(p).ok()
        }
    }

    const THRANDOR: (&str, &str, &str) = ("ACCOUNT1", "Ashenvale", "Thrandor");
    const FIZZWICK: (&str, &str, &str) = ("ACCOUNT2", "Ashenvale", "Fizzwick");

    #[test]
    fn a_toggle_rewrites_one_line_and_undo_puts_it_back() {
        let g = Game::new();
        let before = g.txt("ACCOUNT1", "Thrandor").unwrap();
        assert!(before.contains("Questie: disabled"));

        let r = g.set("Questie", &[THRANDOR], true).unwrap();
        assert_eq!(r.applied, 1);
        let after = g.txt("ACCOUNT1", "Thrandor").unwrap();
        assert_eq!(
            after,
            before.replace("Questie: disabled", "Questie: enabled")
        );

        g.undo(r.snapshot_id.as_deref().unwrap()).unwrap();
        assert_eq!(g.txt("ACCOUNT1", "Thrandor").unwrap(), before);
    }

    #[test]
    fn a_new_addons_txt_is_created_and_undo_removes_it() {
        let g = Game::new();
        assert_eq!(g.txt("ACCOUNT2", "Fizzwick"), None);
        // No AddOns.txt: on by default, so turning it on writes nothing.
        let none = g.set("Questie", &[FIZZWICK], true).unwrap();
        assert!(none.snapshot_id.is_none() && none.applied == 0);

        let r = g.set("Questie", &[FIZZWICK, THRANDOR], false).unwrap();
        assert_eq!(r.applied, 1, "Thrandor already had it off");
        assert_eq!(
            g.txt("ACCOUNT2", "Fizzwick").as_deref(),
            Some("Questie: disabled\n")
        );

        g.undo(r.snapshot_id.as_deref().unwrap()).unwrap();
        assert_eq!(
            g.txt("ACCOUNT2", "Fizzwick"),
            None,
            "it didn't exist before"
        );
    }

    /// Changes staged across addons apply together: one snapshot, one write
    /// per character with every change folded in, and one undo for all.
    #[test]
    fn staged_changes_apply_together() {
        let g = Game::new();
        let before = g.txt("ACCOUNT1", "Thrandor").unwrap();
        let r = g
            .apply(&[
                ("Questie", THRANDOR, true),
                ("Details", THRANDOR, false),
                ("Questie", FIZZWICK, false),
            ])
            .unwrap();
        assert_eq!(r.applied, 3);
        let after = g.txt("ACCOUNT1", "Thrandor").unwrap();
        assert!(after.contains("Questie: enabled") && after.contains("Details: disabled"));
        assert!(after.contains("WeakAuras: enabled"), "untouched line kept");
        assert_eq!(
            g.txt("ACCOUNT2", "Fizzwick").as_deref(),
            Some("Questie: disabled\n")
        );

        g.undo(r.snapshot_id.as_deref().unwrap()).unwrap();
        assert_eq!(g.txt("ACCOUNT1", "Thrandor").unwrap(), before);
        assert_eq!(g.txt("ACCOUNT2", "Fizzwick"), None);
    }

    /// A character whose settings folder is a link: the list says so, and
    /// a change for it is refused before anything is written.
    #[test]
    fn a_linked_character_folder_is_refused() {
        let g = Game::new();
        let account = g.root.join("_classic_beta_/WTF/Account/ACCOUNT1/Ashenvale");
        let elsewhere = g._dir.path().join("elsewhere/Velyra");
        std::fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
        std::fs::rename(account.join("Velyra"), &elsewhere).unwrap();
        crate::test_support::link_dir(&elsewhere, &account.join("Velyra"));
        let before_thrandor = g.txt("ACCOUNT1", "Thrandor");

        let install = crate::install::current(&g.core.settings).unwrap().unwrap();
        let l = list(install.active_flavor().unwrap());
        let linked: Vec<&str> = l
            .characters
            .iter()
            .filter(|c| c.linked)
            .map(|c| c.folder.as_str())
            .collect();
        assert_eq!(linked, ["Velyra"]);

        let velyra = ("ACCOUNT1", "Ashenvale", "Velyra");
        let err = g
            .apply(&[("Questie", THRANDOR, true), ("Questie", velyra, true)])
            .unwrap_err();
        assert!(matches!(err, AppError::PathEscape(_)), "{err}");
        assert_eq!(
            g.txt("ACCOUNT1", "Thrandor"),
            before_thrandor,
            "nothing written"
        );
    }

    #[test]
    fn toggles_name_only_what_the_screen_shows() {
        let g = Game::new();
        let before = g.txt("ACCOUNT1", "Thrandor");
        let refused = |r: AppResult<ToggleResult>| matches!(r, Err(AppError::NotFound(_)));
        // An addon that isn't a listed folder, or names a line of its own.
        assert!(refused(g.set("Nope", &[THRANDOR], true)));
        assert!(refused(g.set("Questie: enabled\nEvil", &[THRANDOR], true)));
        // A character that isn't on the roster, or a path dressed as one.
        assert!(refused(g.set(
            "Questie",
            &[("ACCOUNT1", "Ashenvale", "Nobody")],
            true
        )));
        assert!(refused(g.set(
            "Questie",
            &[("ACCOUNT1", "..", "Thrandor")],
            true
        )));
        assert_eq!(g.txt("ACCOUNT1", "Thrandor"), before, "nothing written");
    }

    #[test]
    fn toggles_are_refused_while_wow_runs() {
        let g = Game::new();
        let before = g.txt("ACCOUNT1", "Thrandor");
        g.probe.set_running(true);
        let err = g.set("Questie", &[THRANDOR], true).unwrap_err();
        assert!(matches!(err, AppError::GameRunning(_)), "{err}");
        assert_eq!(g.txt("ACCOUNT1", "Thrandor"), before);
    }

    /// Undo after switching the active flavor: the toggle's snapshot belongs
    /// to `_classic_beta_`, so it's refused rather than written into
    /// `_classic_era_`'s characters at the same paths.
    #[test]
    fn undo_refuses_a_snapshot_from_another_flavor() {
        let g = Game::new();
        let r = g.set("Questie", &[THRANDOR], true).unwrap();
        let id = r.snapshot_id.unwrap();

        crate::install::set(&g.core.settings, &g.root, Some("_classic_era_")).unwrap();
        let era = g.root.join("_classic_era_/WTF/Account");
        let files_before: Vec<_> = walk_addons_txt(&era);
        assert!(matches!(g.undo(&id), Err(AppError::NotFound(_))));
        assert_eq!(
            walk_addons_txt(&era),
            files_before,
            "nothing written in the other flavor"
        );

        // Back in its own flavor, the same undo works.
        crate::install::set(&g.core.settings, &g.root, Some("_classic_beta_")).unwrap();
        g.undo(&id).unwrap();
        assert!(g
            .txt("ACCOUNT1", "Thrandor")
            .unwrap()
            .contains("Questie: disabled"));
    }

    /// Every AddOns.txt under an Account folder, with its contents.
    fn walk_addons_txt(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.file_name().is_some_and(|n| n == "AddOns.txt") {
                    out.push((p.clone(), std::fs::read(&p).unwrap()));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn undo_takes_only_a_toggles_snapshot() {
        use crate::backup::manifest::Trigger;
        use crate::backup::{SnapshotRequest, SnapshotScope};
        let g = Game::new();
        let game = g.core.active_game().unwrap();
        let full = g
            .core
            .backups()
            .unwrap()
            .create(
                SnapshotRequest {
                    game: &game.root,
                    flavor: &game.flavor,
                    trigger: Trigger::Manual,
                    label: None,
                    scope: SnapshotScope::Full {
                        include_addons: false,
                    },
                    game_running: false,
                },
                &mut |_, _| {},
            )
            .unwrap()
            .unwrap();
        assert!(matches!(g.undo(&full.id), Err(AppError::NotFound(_))));
    }
}
