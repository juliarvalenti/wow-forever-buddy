//! Extracting Blizzard's own Interface code and UI art (CASC2): the
//! community listfile maps paths to FileDataIDs, the storage gives the
//! bytes, and everything lands in a dev-local cache under the build key,
//! like icons. Never committed: it's Blizzard's, fetched per build.
//!
//! The paths are untrusted (a public CSV): each one is sanitized before it
//! names a file under the cache, and a bad row is one row's failure, never
//! a write outside the cache.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{blp, icons, Casc, CascError};

/// The paths worth taking by default: the game's own UI code (the modern
/// layout puts it in Interface/AddOns/Blizzard_*), the fonts, and the art
/// the addon windows draw with. Case doesn't matter; see `selected`.
pub const DEFAULT_FILTERS: &[&str] = &[
    "interface/addons",
    "interface/fonts",
    "interface/buttons",
    "interface/common",
    "interface/tooltips",
    "interface/panels",
    "interface/paperdoll",
    "interface/bags",
];

/// One listfile row: a FileDataID and the path it's known by.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: u32,
    pub path: String,
}

/// The listfile is `id;path` per line, but some mirrors write it reversed;
/// whichever column parses as the ID wins. Bad lines are dropped, not fatal.
pub fn parse_listfile(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let (a, b) = line.split_once(';')?;
            let (a, b) = (a.trim(), b.trim());
            match (a.parse::<u32>().ok(), b.parse::<u32>().ok()) {
                (Some(id), _) => Some(Entry {
                    id,
                    path: b.to_string(),
                }),
                (None, Some(id)) => Some(Entry {
                    id,
                    path: a.to_string(),
                }),
                _ => None,
            }
        })
        .collect()
}

/// A path is taken when some filter is a prefix of it (both lowercased,
/// backslashes normalized): `interface/framexml` takes the whole folder.
pub fn selected(path: &str, filters: &[String]) -> bool {
    let path = normalize(path);
    filters.iter().any(|f| {
        let f = normalize(f);
        !f.is_empty() && (path == f || path.starts_with(&format!("{f}/")))
    })
}

fn normalize(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_lowercase()
}

/// A path safe to join under the cache: no traversal, no drive letters, no
/// empty components. `None` refuses it.
fn safe_rel(path: &str) -> Option<PathBuf> {
    let normalized = normalize(path);
    if normalized.is_empty() || normalized.contains('\0') {
        return None;
    }
    let mut rel = PathBuf::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return None;
        }
        rel.push(part);
    }
    Some(rel)
}

/// What a run did. Every count is per file; nothing aborts the run.
#[derive(Debug, Default, PartialEq)]
pub struct Summary {
    pub extracted: u32,
    /// Already in the cache for this build.
    pub skipped: u32,
    /// In the build but not downloaded yet: Battle.net streams files the
    /// game hasn't needed, so one login in game fills these in.
    pub not_local: u32,
    /// In the listfile but not in this build's root.
    pub missing: u32,
    /// Refused paths, unreadable files, decode and write failures.
    pub failed: u32,
    pub bytes: u64,
}

/// Reads every selected file into `out/<product>/<build key>/interface/...`,
/// `.blp` decoded to PNG, everything else written raw. `dry_run` reports
/// what would be extracted without touching the disk.
pub fn run(
    casc: &Casc,
    out: &Path,
    listfile: &[Entry],
    filters: &[String],
    dry_run: bool,
    verbose: bool,
) -> Summary {
    let mut summary = Summary::default();
    let picked: Vec<&Entry> = listfile.iter().filter(|e| selected(&e.path, filters)).collect();
    // One root scan covers all of them, like the icon cache's fill.
    let ids: HashSet<u32> = picked.iter().map(|e| e.id).collect();
    let ckeys = casc.ckeys(&ids);
    let dir = icons::build_dir(out, &casc.build);
    for entry in picked {
        let Some(rel) = safe_rel(&entry.path) else {
            if verbose {
                eprintln!("failed: {}: refused path", entry.path);
            }
            summary.failed += 1;
            continue;
        };
        let path = target(&dir, &rel);
        let made = if path.is_file() {
            summary.skipped += 1;
            continue;
        } else if dry_run {
            summary.extracted += 1;
            continue;
        } else {
            extract_one(casc, ckeys.get(&entry.id), &path)
        };
        match made {
            Ok(len) => {
                summary.extracted += 1;
                summary.bytes += len;
            }
            Err(Fail::NotLocal) => {
                if verbose {
                    eprintln!("not downloaded yet: {}", entry.path);
                }
                summary.not_local += 1;
            }
            Err(Fail::Missing) => {
                if verbose {
                    eprintln!("not in this build: {}", entry.path);
                }
                summary.missing += 1;
            }
            Err(Fail::Broken(why)) => {
                if verbose {
                    eprintln!("failed: {}: {why}", entry.path);
                }
                summary.failed += 1;
            }
        }
    }
    summary
}

/// Where a file lands: `.blp` becomes `.png`, like the icon cache.
fn target(dir: &Path, rel: &Path) -> PathBuf {
    let mut path = dir.join(rel);
    if rel.extension().is_some_and(|e| e.eq_ignore_ascii_case("blp")) {
        path.set_extension("png");
    }
    path
}

enum Fail {
    NotLocal,
    Missing,
    Broken(String),
}

fn extract_one(casc: &Casc, ckey: Option<&super::Key>, path: &Path) -> Result<u64, Fail> {
    let ckey = ckey.ok_or(Fail::Missing)?;
    let data = read(casc, ckey)?;
    let bytes = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        blp::decode(&data)
            .map_err(|e| Fail::Broken(e.to_string()))
            .and_then(|image| blp::to_png(&image).map_err(|e| Fail::Broken(e.to_string())))?
    } else {
        data
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Fail::Broken(e.to_string()))?;
    }
    crate::fsx::atomic::atomic_replace(path, &bytes).map_err(|e| Fail::Broken(e.to_string()))?;
    Ok(bytes.len() as u64)
}

fn read(casc: &Casc, ckey: &super::Key) -> Result<Vec<u8>, Fail> {
    casc.by_ckey(ckey).map_err(|e| match e {
        CascError::NotLocal => Fail::NotLocal,
        CascError::Missing(_) => Fail::Missing,
        other => Fail::Broken(other.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listfile_rows_parse_in_either_column_order() {
        let rows = parse_listfile(
            "1;interface/framexml/foo.lua\ninterface/bar.lua;2\n\nno-id;no-path\n",
        );
        assert_eq!(
            rows,
            [
                Entry {
                    id: 1,
                    path: "interface/framexml/foo.lua".into()
                },
                Entry {
                    id: 2,
                    path: "interface/bar.lua".into()
                },
            ]
        );
    }

    #[test]
    fn selection_is_a_case_insensitive_folder_prefix() {
        let filters: Vec<String> = ["Interface/FrameXML", "interface/fonts"]
            .iter()
            .map(|f| f.to_string())
            .collect();
        assert!(selected("INTERFACE/FrameXML/UIParent.lua", &filters));
        assert!(selected("interface/fonts/friz.ttf", &filters));
        // A prefix of a name, not a folder, doesn't take it.
        assert!(!selected("interface/fontsdonotexist.blp", &filters));
        assert!(!selected("dbfilesclient/item.dbc", &filters));
    }

    #[test]
    fn unsafe_paths_are_refused() {
        assert_eq!(safe_rel("interface/common/ok.lua"), Some("interface/common/ok.lua".into()));
        assert_eq!(safe_rel("\\interface\\buttons\\ok.blp"), Some("interface/buttons/ok.blp".into()));
        for bad in [
            "",
            "..",
            "../secrets.lua",
            "interface/../../etc/passwd",
            "interface/a\0b.lua",
            "c:/interface/windows.lua",
        ] {
            assert_eq!(safe_rel(bad), None, "{bad}");
        }
    }

    /// The whole chain against the built install: one listfile row per
    /// kind of file (a read, a decode, and a streamed-in later).
    #[test]
    fn extracts_through_the_real_chain() {
        use crate::casc::tests::{install, ICON, LOCKED, STREAMED};

        let mut install = install();
        let casc = Casc::open(&install.flavor).unwrap();
        let out = tempfile::tempdir().unwrap();
        let filters: Vec<String> = ["interface/framexml", "interface/buttons", "interface/common"]
            .iter()
            .map(|f| f.to_string())
            .collect();
        let listfile = vec![
            Entry { id: ICON, path: "Interface/Buttons/UI-Icon.blp".into() },
            Entry { id: LOCKED, path: "interface/common/locked.lua".into() },
            Entry { id: STREAMED, path: "interface/framexml/streamed.lua".into() },
            Entry { id: 999, path: "interface/common/not-in-build.lua".into() },
            // Inside the filters but refused by the path check.
            Entry { id: ICON, path: "interface/common/../../secrets.lua".into() },
        ];

        // LOCKED's chunk is encrypted and the traversal path is refused;
        // both fail on their own.
        let mut summary = run(&casc, out.path(), &listfile, &filters, false, false);
        assert_eq!(
            (summary.extracted, summary.not_local, summary.missing, summary.failed),
            (1, 1, 1, 2),
            "{summary:?}"
        );
        let png = icons::build_dir(out.path(), &casc.build).join("interface/buttons/ui-icon.png");
        assert!(png.is_file());
        let bytes = std::fs::read(&png).unwrap();
        assert!(bytes.starts_with(b"\x89PNG"));

        // The same run again: the one file already in the cache.
        let again = run(&casc, out.path(), &listfile, &filters, false, false);
        assert_eq!(again.skipped, 1);

        // What was streamed in since the storage was opened arrives on a
        // fresh open: NotLocal until then.
        assert_eq!(summary.not_local, 1);
        install.stream_in();
        let casc = Casc::open(&install.flavor).unwrap();
        summary = run(&casc, out.path(), &listfile, &filters, false, false);
        assert_eq!(
            (summary.extracted, summary.skipped, summary.not_local),
            (1, 1, 0),
            "{summary:?}"
        );
        assert!(icons::build_dir(out.path(), &casc.build)
            .join("interface/framexml/streamed.lua")
            .is_file());
    }

    #[test]
    fn dry_run_touches_nothing() {
        use crate::casc::tests::{install, ICON};

        let install = install();
        let casc = Casc::open(&install.flavor).unwrap();
        let out = tempfile::tempdir().unwrap();
        let filters: Vec<String> = vec!["interface/buttons".into()];
        let listfile = [Entry { id: ICON, path: "interface/buttons/icon.blp".into() }];
        let summary = run(&casc, out.path(), &listfile, &filters, true, false);
        assert_eq!(summary, Summary { extracted: 1, ..Summary::default() });
        assert!(!icons::build_dir(out.path(), &casc.build).exists());
    }
}