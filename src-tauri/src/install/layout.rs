//! What a WoW install looks like on disk (spec §1), and how to read one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::fsx::read::safe_read;

/// A validated WoW root folder and the game flavors installed in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Install {
    pub root: PathBuf,
    /// Forever first, then alphabetical by label.
    pub flavors: Vec<Flavor>,
}

/// One `_<flavor>_` folder, e.g. `_classic_beta_`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Flavor {
    /// Folder name, e.g. "_classic_beta_". This is what settings store.
    pub id: String,
    pub label: String,
    pub is_forever: bool,
    /// Battle.net product code from `.build.info`, e.g. "wow_classic_beta".
    pub product: Option<String>,
    /// Client version from `.build.info`, e.g. "1.60.0.63105".
    pub version: Option<String>,
    pub dir: PathBuf,
    pub exe: Option<PathBuf>,
    pub has_wtf: bool,
    pub accounts: Vec<String>,
}

/// The install the app is working with: the root plus the chosen flavor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ActiveInstall {
    pub root: PathBuf,
    pub flavor: Flavor,
}

/// Known flavor folders, their Battle.net product codes and display labels.
/// Unknown `_x_` folders are still listed, labelled by folder name.
const KNOWN_FLAVORS: &[(&str, &str, &str)] = &[
    ("_retail_", "wow", "Retail"),
    ("_ptr_", "wowt", "Retail PTR"),
    ("_xptr_", "wowxptr", "Retail Experimental PTR"),
    ("_beta_", "wow_beta", "Retail Beta"),
    ("_classic_", "wow_classic", "Classic"),
    ("_classic_ptr_", "wow_classic_ptr", "Classic PTR"),
    ("_classic_beta_", "wow_classic_beta", "Classic Beta"),
    ("_classic_era_", "wow_classic_era", "Classic Era"),
    (
        "_classic_era_ptr_",
        "wow_classic_era_ptr",
        "Classic Era PTR",
    ),
    ("_anniversary_", "wow_anniversary", "Anniversary"),
];

/// Game executables that identify a flavor folder (and, in T6, a running game).
/// Forever's beta ships `WowB.exe` / `WowB-arm64.exe`.
pub const KNOWN_EXES: &[&str] = &[
    "Wow.exe",
    "Wow-64.exe",
    "Wow-arm64.exe",
    "WowT.exe",
    "WowB.exe",
    "WowB-arm64.exe",
    "WowClassic.exe",
    "WowClassic-arm64.exe",
    "WowClassicT.exe",
    "WowClassicB.exe",
];

/// WoW: Forever is identified by data, not folder name alone: `_classic_beta_`
/// is a generic Blizzard folder that past Classic betas used too. Forever's
/// beta is product `wow_classic_beta` on a 1.6x client; anything naming
/// "forever" (e.g. a future launch folder or product) counts as well.
fn forever_label(
    folder: &str,
    product: Option<&str>,
    version: Option<&str>,
) -> Option<&'static str> {
    let named_forever = folder.to_ascii_lowercase().contains("forever")
        || product.is_some_and(|p| p.to_ascii_lowercase().contains("forever"));
    if named_forever {
        return Some("WoW: Forever");
    }
    let forever_beta_build = version.is_some_and(|v| v.starts_with("1.6"));
    (product == Some("wow_classic_beta") && forever_beta_build).then_some("WoW: Forever (Beta)")
}

pub fn is_flavor_dir_name(name: &str) -> bool {
    name.len() > 2 && name.starts_with('_') && name.ends_with('_')
}

/// Inspects `root` as a WoW install. Fails with `InvalidInstall` unless at
/// least one flavor folder has a WTF folder or a game executable.
pub fn inspect_root(root: &Path) -> AppResult<Install> {
    let invalid = |why: &str| AppError::InvalidInstall(format!("{}: {why}", root.display()));
    if !root.is_dir() {
        return Err(invalid("not a folder"));
    }
    let builds = safe_read(&root.join(".build.info"))
        .map(|bytes| parse_build_info(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default();

    let mut flavors = Vec::new();
    for entry in std::fs::read_dir(root)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() && is_flavor_dir_name(&name) {
            let flavor = inspect_flavor(&entry.path(), &name, &builds)?;
            if flavor.has_wtf || flavor.exe.is_some() {
                flavors.push(flavor);
            }
        }
    }
    if flavors.is_empty() {
        return Err(invalid(
            "no WoW game folders (like _retail_ or _classic_beta_) found",
        ));
    }
    flavors.sort_by(|a, b| {
        b.is_forever
            .cmp(&a.is_forever)
            .then_with(|| a.label.cmp(&b.label))
    });
    Ok(Install {
        root: root.to_path_buf(),
        flavors,
    })
}

fn inspect_flavor(dir: &Path, id: &str, builds: &[BuildRow]) -> AppResult<Flavor> {
    let known = KNOWN_FLAVORS.iter().find(|(folder, _, _)| *folder == id);
    let product = known.map(|(_, product, _)| product.to_string());
    let version = product
        .as_deref()
        .and_then(|p| builds.iter().find(|b| b.product == p))
        .map(|b| b.version.clone());

    let forever = forever_label(id, product.as_deref(), version.as_deref());
    let label = forever
        .or(known.map(|(_, _, label)| *label))
        .map(str::to_string)
        .unwrap_or_else(|| id.trim_matches('_').replace('_', " "));

    let entries: Vec<String> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let exe = KNOWN_EXES
        .iter()
        .find_map(|exe| entries.iter().find(|e| e.eq_ignore_ascii_case(exe)))
        .or_else(|| {
            entries
                .iter()
                .find(|e| e.starts_with("World of Warcraft") && e.ends_with(".app"))
        })
        .map(|name| dir.join(name));

    let wtf = dir.join("WTF");
    let accounts = list_accounts(&wtf);
    Ok(Flavor {
        id: id.to_string(),
        label,
        is_forever: forever.is_some(),
        product,
        version,
        dir: dir.to_path_buf(),
        exe,
        has_wtf: wtf.is_dir(),
        accounts,
    })
}

fn list_accounts(wtf: &Path) -> Vec<String> {
    let mut accounts: Vec<String> = std::fs::read_dir(wtf.join("Account"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.eq_ignore_ascii_case("SavedVariables"))
        .collect();
    accounts.sort();
    accounts
}

/// Turns whatever the user picked (the root, a flavor folder, or a WTF
/// folder) into the install root.
pub fn normalize_to_root(picked: &Path) -> PathBuf {
    let mut path = picked.to_path_buf();
    if path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("WTF"))
    {
        path.pop();
    }
    if path
        .file_name()
        .is_some_and(|n| is_flavor_dir_name(&n.to_string_lossy()))
    {
        path.pop();
    }
    path
}

/// Picks the flavor to work with: the requested one, else Forever, else the
/// only flavor that has a WTF folder.
pub fn choose_flavor(install: &Install, requested: Option<&str>) -> AppResult<Flavor> {
    if let Some(id) = requested {
        return install
            .flavors
            .iter()
            .find(|f| f.id.eq_ignore_ascii_case(id))
            .cloned()
            .ok_or_else(|| {
                AppError::InvalidInstall(format!("no {id} folder in {}", install.root.display()))
            });
    }
    if let Some(forever) = install.flavors.iter().find(|f| f.is_forever) {
        return Ok(forever.clone());
    }
    let with_wtf: Vec<&Flavor> = install.flavors.iter().filter(|f| f.has_wtf).collect();
    match with_wtf.as_slice() {
        [only] => Ok((*only).clone()),
        _ => Err(AppError::InvalidInstall(format!(
            "{} has several game folders; choose one",
            install.root.display()
        ))),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct BuildRow {
    product: String,
    version: String,
}

/// `.build.info` is a pipe-separated table whose header cells look like
/// `Name!TYPE:size`. We only need each row's Product and Version.
fn parse_build_info(text: &str) -> Vec<BuildRow> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let columns: Vec<&str> = header
        .split('|')
        .map(|c| c.split('!').next().unwrap_or(c).trim())
        .collect();
    let (Some(product_col), Some(version_col)) = (
        columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case("Product")),
        columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case("Version")),
    ) else {
        return Vec::new();
    };
    lines
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').collect();
            Some(BuildRow {
                product: cells.get(product_col)?.trim().to_string(),
                version: cells.get(version_col)?.trim().to_string(),
            })
        })
        .filter(|row| !row.product.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture_copy, fixture_root};

    #[test]
    fn parses_build_info() {
        let rows =
            parse_build_info(&std::fs::read_to_string(fixture_root().join(".build.info")).unwrap());
        assert_eq!(
            rows,
            vec![
                BuildRow {
                    product: "wow_classic_beta".into(),
                    version: "1.60.0.63105".into()
                },
                BuildRow {
                    product: "wow_classic_era".into(),
                    version: "1.15.7.61582".into()
                },
            ]
        );
        assert!(parse_build_info("").is_empty());
        assert!(parse_build_info("Branch!STRING:0|Active!DEC:1\nus|1").is_empty());
    }

    #[test]
    fn inspects_fixture_install() {
        let install = inspect_root(&fixture_root()).unwrap();
        let ids: Vec<&str> = install.flavors.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            ["_classic_beta_", "_classic_era_"],
            "Forever sorts first"
        );

        let forever = &install.flavors[0];
        assert!(forever.is_forever);
        assert_eq!(forever.label, "WoW: Forever (Beta)");
        assert_eq!(forever.product.as_deref(), Some("wow_classic_beta"));
        assert_eq!(forever.version.as_deref(), Some("1.60.0.63105"));
        assert!(forever.exe.as_ref().unwrap().ends_with("WowB.exe"));
        assert!(forever.has_wtf);
        assert_eq!(forever.accounts, ["ACCOUNT1", "ACCOUNT2"]);

        let era = &install.flavors[1];
        assert!(!era.is_forever);
        assert_eq!(era.label, "Classic Era");
        assert_eq!(era.exe, None, "WTF-only flavor still counts");
        assert_eq!(era.accounts, ["ERA1"]);
    }

    #[test]
    fn old_classic_beta_is_not_forever() {
        assert_eq!(
            forever_label(
                "_classic_beta_",
                Some("wow_classic_beta"),
                Some("1.13.2.31650")
            ),
            None
        );
        assert_eq!(
            forever_label("_classic_beta_", Some("wow_classic_beta"), None),
            None
        );
        assert_eq!(forever_label("_forever_", None, None), Some("WoW: Forever"));
        assert_eq!(
            forever_label("_x_", Some("wow_forever"), Some("2.0.0")),
            Some("WoW: Forever")
        );
    }

    #[test]
    fn rejects_folders_that_are_not_installs() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(
            inspect_root(tmp.path()),
            Err(AppError::InvalidInstall(_))
        ));
        std::fs::create_dir(tmp.path().join("_retail_")).unwrap(); // empty flavor
        assert!(inspect_root(tmp.path()).is_err());
        assert!(inspect_root(&tmp.path().join("missing")).is_err());
    }

    #[test]
    fn works_without_build_info() {
        let (_dir, root) = fixture_copy();
        std::fs::remove_file(root.join(".build.info")).unwrap();
        let install = inspect_root(&root).unwrap();
        let beta = install
            .flavors
            .iter()
            .find(|f| f.id == "_classic_beta_")
            .unwrap();
        assert!(
            !beta.is_forever,
            "can't tell a Forever build without a version"
        );
        assert_eq!(beta.label, "Classic Beta");
    }

    #[test]
    fn normalizes_any_pick_to_the_root() {
        let root = Path::new("/games/World of Warcraft");
        assert_eq!(normalize_to_root(root), root);
        assert_eq!(normalize_to_root(&root.join("_classic_beta_")), root);
        assert_eq!(
            normalize_to_root(&root.join("_classic_beta_").join("WTF")),
            root
        );
        assert_eq!(
            normalize_to_root(&root.join("_classic_beta_").join("wtf")),
            root
        );
    }

    #[test]
    fn chooses_requested_then_forever_then_only_wtf() {
        let install = inspect_root(&fixture_root()).unwrap();
        assert_eq!(choose_flavor(&install, None).unwrap().id, "_classic_beta_");
        assert_eq!(
            choose_flavor(&install, Some("_CLASSIC_ERA_")).unwrap().id,
            "_classic_era_"
        );
        assert!(choose_flavor(&install, Some("_retail_")).is_err());

        let mut no_forever = install.clone();
        no_forever
            .flavors
            .iter_mut()
            .for_each(|f| f.is_forever = false);
        assert!(
            choose_flavor(&no_forever, None).is_err(),
            "two WTF flavors: ambiguous"
        );
        no_forever.flavors.retain(|f| f.id == "_classic_era_");
        assert_eq!(
            choose_flavor(&no_forever, None).unwrap().id,
            "_classic_era_"
        );
    }
}
