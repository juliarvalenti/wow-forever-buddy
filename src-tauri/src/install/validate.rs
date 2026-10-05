//! Turning a folder into a validated `Install` (spec §1, "Validation and flavors").

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::fsx::read::safe_read;
use crate::fsx::relpath::GameRoot;
use crate::install::layout::{
    label_from_folder, lookup, parse_build_info, product_matches_folder, BuildRow, Flavor, Install,
    KNOWN_FLAVORS,
};

/// How far up from a picked folder we look for the root. Covers picking
/// `<root>/_flavor_/WTF/Account/<ACCOUNT>/<Realm>`.
const MAX_LEVELS_UP: usize = 5;

/// `_retail_`, `_classic_beta_`, ...
pub fn is_flavor_dir_name(name: &str) -> bool {
    name.len() > 2 && name.starts_with('_') && name.ends_with('_')
}

/// Walks up from whatever the user (or the registry) pointed at, the root, a
/// flavor folder, `WTF` or deeper, to the WoW root.
pub fn normalize_root(picked: &Path) -> Option<PathBuf> {
    locate(picked).map(|(root, _)| root)
}

/// Like `normalize_root`, plus the flavor folder the pick was inside, if any.
///
/// Walks the path as given, not its canonical form: if `WTF` is a link to
/// another drive, canonicalizing first would leave the install entirely.
/// Only the root that's found gets canonicalized.
pub fn locate(picked: &Path) -> Option<(PathBuf, Option<String>)> {
    let picked = std::path::absolute(picked).ok()?;
    if !picked.exists() {
        return None;
    }
    let found = picked
        .ancestors()
        .take(MAX_LEVELS_UP + 1)
        .find(|dir| looks_like_root(dir))?;
    let flavor = picked
        .strip_prefix(found)
        .ok()
        .and_then(|rel| rel.components().next())
        .and_then(|c| c.as_os_str().to_str())
        .filter(|name| is_flavor_dir_name(name))
        .map(str::to_string);
    Some((dunce::canonicalize(found).ok()?, flavor))
}

fn looks_like_root(dir: &Path) -> bool {
    dir.join(".build.info").is_file()
        || flavor_dirs(dir)
            .iter()
            .any(|(_, path)| is_valid_flavor(path))
}

fn flavor_dirs(root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            is_flavor_dir_name(&name).then(|| (name, e.path()))
        })
        .collect();
    dirs.sort();
    dirs
}

fn is_valid_flavor(dir: &Path) -> bool {
    dir.join("WTF").is_dir() || find_exe(dir, &[]).is_some()
}

/// The flavor's game executable: a known name first, then any `Wow*.exe`,
/// then a macOS `World of Warcraft*.app` bundle.
fn find_exe(dir: &Path, preferred: &[&str]) -> Option<PathBuf> {
    for name in preferred {
        let path = dir.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            let is_file = e.file_type().is_ok_and(|t| t.is_file());
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            (is_file && name.starts_with("wow") && name.ends_with(".exe"))
                || (is_dir && name.starts_with("world of warcraft") && name.ends_with(".app"))
        })
        .map(|e| e.path())
        .collect();
    found.sort();
    found.into_iter().next()
}

/// Sorted subfolder names, minus `SavedVariables` (which sits next to the
/// group and character folders in the WTF tree).
fn subdirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) || e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|name| name != "SavedVariables")
        .collect();
    names.sort();
    names
}

/// The WTF roster: accounts, and how many character folders there are
/// (`Account/<account>/<group>/<character>`, see `layout::Flavor::characters`).
fn roster(wtf: &Path) -> (Vec<String>, u32) {
    let accounts_dir = wtf.join("Account");
    let accounts = subdirs(&accounts_dir);
    let mut characters = 0;
    for account in &accounts {
        let account_dir = accounts_dir.join(account);
        for group in subdirs(&account_dir) {
            characters += subdirs(&account_dir.join(&group)).len() as u32;
        }
    }
    (accounts, characters)
}

/// Scans a root and validates it. Fails unless at least one flavor folder has
/// a game exe or a WTF folder (a WTF-only copy still counts, so backups work).
pub fn scan(root: &Path) -> AppResult<Install> {
    let root = dunce::canonicalize(root)
        .map_err(|e| AppError::InvalidInstall(format!("{}: {e}", root.display())))?;

    // A `.build.info` we can't read right now (Battle.net mid-update) only
    // costs us labels and versions.
    let build_path = root.join(".build.info");
    let rows: Vec<BuildRow> = if build_path.is_file() {
        safe_read(&build_path)
            .map(|bytes| parse_build_info(&String::from_utf8_lossy(&bytes)))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut flavors = Vec::new();
    for (id, dir) in flavor_dirs(&root) {
        let row = rows
            .iter()
            .filter(|r| product_matches_folder(&r.product, &id))
            .max_by_key(|r| r.active);
        let product = row.map(|r| r.product.as_str());
        let version = row.map(|r| r.version.as_str()).filter(|v| !v.is_empty());
        let known = lookup(&id, product, version);

        let preferred = KNOWN_FLAVORS
            .iter()
            .filter(|k| k.folder == id)
            .flat_map(|k| k.exes.iter().copied())
            .collect::<Vec<_>>();
        let exe = find_exe(&dir, &preferred);
        let wtf = dir.join("WTF");
        let has_wtf = wtf.is_dir();
        if exe.is_none() && !has_wtf {
            continue;
        }

        let (accounts, characters) = roster(&wtf);
        flavors.push(Flavor {
            label: known.map_or_else(|| label_from_folder(&id), |k| k.label.to_string()),
            is_forever: known.is_some_and(|k| k.is_forever),
            product: product.map(str::to_string),
            version: version.map(str::to_string),
            accounts,
            characters,
            // The same record T4's resolver checks paths against.
            links: GameRoot::new(&dir).map(|r| r.links).unwrap_or_default(),
            id,
            dir,
            exe,
            has_wtf,
        });
    }

    if flavors.is_empty() {
        return Err(AppError::InvalidInstall(format!(
            "no WoW game folder (like _classic_beta_ or _retail_) with a game or a WTF folder in {}",
            root.display()
        )));
    }
    Ok(Install {
        root,
        flavors,
        active: None,
    })
}

/// Picks the active flavor: the requested one, else Forever, else the only
/// flavor with a WTF folder, else the only flavor.
pub fn choose_flavor(install: &Install, requested: Option<&str>) -> AppResult<String> {
    if let Some(id) = requested {
        return install.flavor(id).map(|f| f.id.clone()).ok_or_else(|| {
            AppError::InvalidInstall(format!(
                "{id} isn't a game folder in {}",
                install.root.display()
            ))
        });
    }
    let pick = |fs: Vec<&Flavor>| (fs.len() == 1).then(|| fs[0].id.clone());
    pick(install.flavors.iter().filter(|f| f.is_forever).collect())
        .or_else(|| pick(install.flavors.iter().filter(|f| f.has_wtf).collect()))
        .or_else(|| pick(install.flavors.iter().collect()))
        .ok_or_else(|| AppError::InvalidInstall("several game versions found; choose one".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsx::relpath::LinkedFolder;
    use crate::test_support::{fixture_copy, link_dir};

    #[test]
    fn scans_the_fixture_install() {
        let (_tmp, root) = fixture_copy();
        let install = scan(&root).unwrap();
        assert_eq!(install.root, dunce::canonicalize(&root).unwrap());
        let ids: Vec<_> = install.flavors.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["_classic_beta_", "_classic_era_"]);

        let forever = &install.flavors[0];
        assert_eq!(forever.label, "WoW: Forever (Beta)");
        assert!(forever.is_forever);
        assert_eq!(forever.product.as_deref(), Some("wow_classic_beta"));
        assert_eq!(forever.version.as_deref(), Some("1.60.0.63105"));
        assert_eq!(
            forever.exe.as_ref().unwrap().file_name().unwrap(),
            "WowB.exe"
        );
        assert!(forever.has_wtf);
        assert_eq!(forever.accounts, ["ACCOUNT1", "ACCOUNT2"]);
        // ACCOUNT1: Thrandor + Velyra on Ashenvale, Brannic on Old Blanchy,
        // Lúthien on Pyrewood Village. ACCOUNT2: Fizzwick on Ashenvale.
        assert_eq!(forever.characters, 5);

        let era = &install.flavors[1];
        assert_eq!(era.label, "Classic Era");
        assert_eq!(era.version.as_deref(), Some("1.15.7.61582"));
        assert!(era.exe.is_none());
        assert!(era.has_wtf);
        assert_eq!(era.accounts, ["ERA1"]);
        assert_eq!(era.characters, 1);
    }

    #[test]
    fn normalizes_up_from_anywhere_inside() {
        let (_tmp, root) = fixture_copy();
        let canon = dunce::canonicalize(&root).unwrap();
        for picked in [
            "",
            "_classic_beta_",
            "_classic_beta_/WTF",
            "_classic_beta_/WTF/Account/ACCOUNT1",
            "_classic_beta_/WTF/Account/ACCOUNT1/Old Blanchy",
            "_classic_era_/WTF",
        ] {
            assert_eq!(
                normalize_root(&root.join(picked)),
                Some(canon.clone()),
                "{picked:?}"
            );
        }
    }

    #[test]
    fn rejects_folders_that_are_not_wow() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(normalize_root(tmp.path()), None);
        assert_eq!(normalize_root(&tmp.path().join("missing")), None);
        assert!(matches!(scan(tmp.path()), Err(AppError::InvalidInstall(_))));

        // A flavor-shaped folder with neither exe nor WTF doesn't count.
        std::fs::create_dir_all(tmp.path().join("_retail_/Interface")).unwrap();
        assert_eq!(normalize_root(tmp.path()), None);
        assert!(scan(tmp.path()).is_err());
    }

    #[test]
    fn works_without_build_info() {
        let (_tmp, root) = fixture_copy();
        std::fs::remove_file(root.join(".build.info")).unwrap();
        assert!(normalize_root(&root.join("_classic_beta_")).is_some());
        let install = scan(&root).unwrap();
        // Without a version we can't tell Forever from an older classic beta.
        assert_eq!(install.flavors[0].label, "Classic Beta");
        assert!(!install.flavors[0].is_forever);
        assert_eq!(install.flavors[0].version, None);
    }

    /// Probe run 1: Forever's `Account/<A>/<group id>/<First>-<Surname>`
    /// next to the older `Account/<A>/<Realm>/<Name>`, both counted.
    #[test]
    fn counts_characters_in_both_wtf_layouts() {
        let tmp = tempfile::tempdir().unwrap();
        let account = tmp.path().join("_classic_beta_/WTF/Account/ACCOUNT1");
        for dir in [
            "70/Ellygie-Vargur",
            "70/Ellyanna-Vargur",
            "70/Brannic",
            "Classic Beta PvP 2/Ellygie",
            "SavedVariables",
        ] {
            std::fs::create_dir_all(account.join(dir)).unwrap();
        }
        let install = scan(tmp.path()).unwrap();
        assert_eq!(install.flavors[0].accounts, ["ACCOUNT1"]);
        assert_eq!(install.flavors[0].characters, 4);
    }

    #[test]
    fn unknown_flavor_folders_use_naming_convention() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("_forever_/WTF")).unwrap();
        std::fs::write(tmp.path().join("_forever_/WowForever.exe"), b"").unwrap();
        std::fs::write(
            tmp.path().join(".build.info"),
            "Product!STRING:0|Version!STRING:0\nwow_forever|2.0.0.1\n",
        )
        .unwrap();
        let install = scan(tmp.path()).unwrap();
        let f = &install.flavors[0];
        assert_eq!(f.label, "Forever");
        assert_eq!(f.product.as_deref(), Some("wow_forever"));
        assert_eq!(f.version.as_deref(), Some("2.0.0.1"));
        assert_eq!(
            f.exe.as_ref().unwrap().file_name().unwrap(),
            "WowForever.exe"
        );
    }

    #[test]
    fn prefers_known_exe_names() {
        let (_tmp, root) = fixture_copy();
        let dir = root.join("_classic_beta_");
        std::fs::write(dir.join("WowB-arm64.exe"), b"").unwrap();
        std::fs::write(dir.join("Wow-Launcher-Helper.exe"), b"").unwrap();
        let install = scan(&root).unwrap();
        assert_eq!(
            install.flavors[0]
                .exe
                .as_ref()
                .unwrap()
                .file_name()
                .unwrap(),
            "WowB.exe"
        );
    }

    #[test]
    fn flavor_choice() {
        let (_tmp, root) = fixture_copy();
        let install = scan(&root).unwrap();
        assert_eq!(choose_flavor(&install, None).unwrap(), "_classic_beta_");
        assert_eq!(
            choose_flavor(&install, Some("_classic_era_")).unwrap(),
            "_classic_era_"
        );
        assert!(choose_flavor(&install, Some("_retail_")).is_err());

        // No Forever, two WTF flavors: ambiguous.
        let mut no_forever = install.clone();
        no_forever.flavors[0].is_forever = false;
        assert!(choose_flavor(&no_forever, None).is_err());
        // Only one has a WTF folder: that one.
        no_forever.flavors[1].has_wtf = false;
        assert_eq!(choose_flavor(&no_forever, None).unwrap(), "_classic_beta_");
    }

    #[test]
    fn linked_wtf_and_addons_are_allowed_and_reported() {
        let (tmp, root) = fixture_copy();
        let flavor = root.join("_classic_beta_");
        // No spaces: `cmd /C` quoting rules make mklink brittle with them.
        let elsewhere = tmp.path().join("syncdrive");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::rename(flavor.join("WTF"), elsewhere.join("WTF")).unwrap();
        // Join components separately: a `/` inside a Windows path reaches
        // `cmd` as a switch ("Invalid switch - AddOns").
        let addons = flavor.join("Interface").join("AddOns");
        std::fs::rename(&addons, elsewhere.join("AddOns")).unwrap();
        link_dir(&elsewhere.join("WTF"), &flavor.join("WTF"));
        link_dir(&elsewhere.join("AddOns"), &addons);

        let install = scan(&root).unwrap();
        let f = &install.flavors[0];
        assert!(f.has_wtf);
        assert_eq!(f.accounts, ["ACCOUNT1", "ACCOUNT2"]);
        let wtf_target = dunce::canonicalize(elsewhere.join("WTF")).unwrap();
        let addons_target = dunce::canonicalize(elsewhere.join("AddOns")).unwrap();
        assert_eq!(
            f.links,
            [
                LinkedFolder {
                    folder: "WTF".into(),
                    target: wtf_target
                },
                LinkedFolder {
                    folder: "Interface/AddOns".into(),
                    target: addons_target
                },
            ]
        );
        // Exactly what the resolver will check paths against.
        assert_eq!(f.links, GameRoot::new(&f.dir).unwrap().links);
        // Unlinked flavors report nothing.
        assert!(install.flavors[1].links.is_empty());

        // Picking the linked WTF still finds this install and flavor.
        let canon_root = dunce::canonicalize(&root).unwrap();
        assert_eq!(
            locate(&flavor.join("WTF/Account")),
            Some((canon_root, Some("_classic_beta_".into())))
        );
    }

    #[test]
    fn locate_reports_the_flavor_picked_into() {
        let (_tmp, root) = fixture_copy();
        assert_eq!(locate(&root).unwrap().1, None);
        assert_eq!(
            locate(&root.join("_classic_era_/WTF"))
                .unwrap()
                .1
                .as_deref(),
            Some("_classic_era_")
        );
    }
}
