//! Finding WoW installs (spec §1, "Detection order").

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::install::layout::Install;
use crate::install::validate::{choose_flavor, normalize_root, scan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSource {
    Saved,
    Registry,
    CommonPath,
}

/// A validated install found by detection. `install.active` is set when a
/// flavor can be chosen without asking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct InstallCandidate {
    pub install: Install,
    pub source: CandidateSource,
}

/// What `install_detect` returns: every install found, plus every place we
/// looked, so onboarding's "not found" state can say where.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct DetectReport {
    pub candidates: Vec<InstallCandidate>,
    pub looked_in: Vec<LookedIn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct LookedIn {
    pub source: CandidateSource,
    pub path: PathBuf,
}

/// Runs the sources and reports both the installs and the places checked.
pub fn detect_report(sources: &[Box<dyn InstallSource>]) -> DetectReport {
    DetectReport {
        candidates: detect(sources),
        looked_in: sources
            .iter()
            .flat_map(|s| {
                s.paths().into_iter().map(|path| LookedIn {
                    source: s.source(),
                    path,
                })
            })
            .collect(),
    }
}

/// Where to look. Each source returns raw paths; `detect` normalizes and
/// validates them.
pub trait InstallSource {
    fn source(&self) -> CandidateSource;
    fn paths(&self) -> Vec<PathBuf>;
}

/// A fixed list: the saved path, the common paths, or test paths.
pub struct FixedPaths(pub CandidateSource, pub Vec<PathBuf>);

impl InstallSource for FixedPaths {
    fn source(&self) -> CandidateSource {
        self.0
    }
    fn paths(&self) -> Vec<PathBuf> {
        self.1.clone()
    }
}

/// Runs every source in order and returns each distinct valid install once,
/// first source first.
pub fn detect(sources: &[Box<dyn InstallSource>]) -> Vec<InstallCandidate> {
    let mut found: Vec<InstallCandidate> = Vec::new();
    for source in sources {
        for path in source.paths() {
            let Some(root) = normalize_root(&path) else {
                continue;
            };
            if found.iter().any(|c| same_path(&c.install.root, &root)) {
                continue;
            }
            let Ok(mut install) = scan(&root) else {
                continue;
            };
            install.active = choose_flavor(&install, None).ok();
            found.push(InstallCandidate {
                install,
                source: source.source(),
            });
        }
    }
    found
}

/// Case-insensitive, like the Windows and default macOS file systems.
fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// Registry, then common paths. The saved path is prepended by the caller.
pub fn system_sources() -> Vec<Box<dyn InstallSource>> {
    let common: Box<dyn InstallSource> =
        Box::new(FixedPaths(CandidateSource::CommonPath, common_paths()));
    #[cfg(windows)]
    let sources: Vec<Box<dyn InstallSource>> = vec![Box::new(registry::RegistrySource), common];
    #[cfg(not(windows))]
    let sources = vec![common];
    sources
}

/// `Program Files` locations and `X:\World of Warcraft` /
/// `X:\Games\World of Warcraft` on each drive that exists (Windows), or
/// `/Applications/World of Warcraft` (macOS).
pub fn common_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(dir) = std::env::var_os(var) {
                paths.push(PathBuf::from(dir).join("World of Warcraft"));
            }
        }
        for letter in b'C'..=b'Z' {
            let drive = PathBuf::from(format!("{}:\\", letter as char));
            if !drive.exists() {
                continue;
            }
            paths.push(drive.join("World of Warcraft"));
            paths.push(drive.join("Games").join("World of Warcraft"));
        }
    } else if cfg!(target_os = "macos") {
        paths.push(PathBuf::from("/Applications/World of Warcraft"));
    }
    paths
}

#[cfg(windows)]
mod registry {
    use std::path::PathBuf;

    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    use super::{CandidateSource, InstallSource};

    /// (key, value) pairs Blizzard's installer writes. `InstallPath` often
    /// points at a flavor folder; `normalize_root` walks up from it.
    const KEYS: &[(&str, &str)] = &[
        (
            r"SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft",
            "InstallPath",
        ),
        (
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\World of Warcraft",
            "InstallLocation",
        ),
    ];

    pub struct RegistrySource;

    impl InstallSource for RegistrySource {
        fn source(&self) -> CandidateSource {
            CandidateSource::Registry
        }

        fn paths(&self) -> Vec<PathBuf> {
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            KEYS.iter()
                .filter_map(|(key, value)| {
                    let path: String = hklm.open_subkey(key).ok()?.get_value(value).ok()?;
                    let path = path.trim().trim_matches('"');
                    (!path.is_empty()).then(|| PathBuf::from(path))
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_copy;

    #[test]
    fn finds_each_install_once_in_source_order() {
        let (_a, root_a) = fixture_copy();
        let (_b, root_b) = fixture_copy();
        let missing = root_a.join("nope");
        let sources: Vec<Box<dyn InstallSource>> = vec![
            Box::new(FixedPaths(
                CandidateSource::Registry,
                vec![root_a.join("_classic_beta_"), missing],
            )),
            Box::new(FixedPaths(
                CandidateSource::CommonPath,
                vec![root_a.join("_classic_era_/WTF"), root_b.clone()],
            )),
        ];
        let found = detect(&sources);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].source, CandidateSource::Registry);
        assert_eq!(found[0].install.root, dunce::canonicalize(&root_a).unwrap());
        assert_eq!(found[0].install.active.as_deref(), Some("_classic_beta_"));
        assert_eq!(found[1].source, CandidateSource::CommonPath);
        assert_eq!(found[1].install.root, dunce::canonicalize(&root_b).unwrap());
    }

    #[test]
    fn nothing_found_is_empty_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let sources: Vec<Box<dyn InstallSource>> = vec![Box::new(FixedPaths(
            CandidateSource::CommonPath,
            vec![tmp.path().to_path_buf()],
        ))];
        assert!(detect(&sources).is_empty());
    }

    #[test]
    fn report_lists_every_place_looked() {
        let (_a, root) = fixture_copy();
        let tmp = tempfile::tempdir().unwrap();
        let sources: Vec<Box<dyn InstallSource>> = vec![
            Box::new(FixedPaths(
                CandidateSource::Registry,
                vec![tmp.path().into()],
            )),
            Box::new(FixedPaths(CandidateSource::CommonPath, vec![root.clone()])),
        ];
        let report = detect_report(&sources);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(
            report.looked_in,
            [
                LookedIn {
                    source: CandidateSource::Registry,
                    path: tmp.path().into()
                },
                LookedIn {
                    source: CandidateSource::CommonPath,
                    path: root
                },
            ]
        );
    }

    #[test]
    fn common_paths_for_this_platform() {
        let paths = common_paths();
        if cfg!(windows) {
            assert!(paths.contains(&PathBuf::from(r"C:\Games\World of Warcraft")));
            // Only drives that exist are listed.
            for p in &paths {
                let drive = p.ancestors().last().unwrap(); // e.g. "C:\"
                assert!(drive.exists(), "{}", p.display());
            }
        } else if cfg!(target_os = "macos") {
            assert_eq!(paths, [PathBuf::from("/Applications/World of Warcraft")]);
        }
    }
}
