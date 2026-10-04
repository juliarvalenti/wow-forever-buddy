//! Finding WoW installs on this machine (spec §1).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::install::layout::{inspect_root, normalize_to_root, Install};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum CandidateSource {
    Registry,
    CommonPath,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct InstallCandidate {
    pub install: Install,
    pub source: CandidateSource,
}

/// Somewhere installs might be found. A seam so tests can point detection at
/// the fixture instead of the real machine.
pub trait InstallSource {
    fn paths(&self) -> Vec<(PathBuf, CandidateSource)>;
}

/// Every source for this platform, in priority order.
pub fn system_sources() -> Vec<Box<dyn InstallSource + Send>> {
    vec![Box::new(RegistrySource), Box::new(CommonPathSource)]
}

/// Runs each source, validates what it finds, and drops duplicates (the same
/// root found twice, compared case-insensitively). Invalid paths are skipped.
pub fn detect(sources: &[Box<dyn InstallSource + Send>]) -> Vec<InstallCandidate> {
    let mut seen: Vec<String> = Vec::new();
    let mut found = Vec::new();
    for source in sources {
        for (path, kind) in source.paths() {
            let root = normalize_to_root(&path);
            let Ok(canonical) = dunce::canonicalize(&root) else {
                continue;
            };
            let key = canonical.to_string_lossy().to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            if let Ok(install) = inspect_root(&canonical) {
                seen.push(key);
                found.push(InstallCandidate {
                    install,
                    source: kind,
                });
            }
        }
    }
    found
}

/// Blizzard's install keys. `InstallPath` often points at a flavor folder;
/// `detect` normalizes that up to the root.
pub struct RegistrySource;

impl InstallSource for RegistrySource {
    #[cfg(windows)]
    fn paths(&self) -> Vec<(PathBuf, CandidateSource)> {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let mut paths = Vec::new();
        let mut push = |value: std::io::Result<String>| {
            if let Ok(p) = value {
                if !p.trim().is_empty() {
                    paths.push((PathBuf::from(p.trim()), CandidateSource::Registry));
                }
            }
        };

        // The main key plus any per-product subkeys (betas/PTRs may register their own).
        for base in [
            r"SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft",
            r"SOFTWARE\Blizzard Entertainment\World of Warcraft",
        ] {
            if let Ok(key) = hklm.open_subkey(base) {
                push(key.get_value("InstallPath"));
                for sub in key.enum_keys().flatten() {
                    if let Ok(subkey) = key.open_subkey(&sub) {
                        push(subkey.get_value("InstallPath"));
                    }
                }
            }
        }

        let uninstall = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
        if let Ok(key) = hklm.open_subkey(uninstall) {
            for sub in key.enum_keys().flatten() {
                let Ok(entry) = key.open_subkey(&sub) else {
                    continue;
                };
                let name: String = entry.get_value("DisplayName").unwrap_or_default();
                if name.starts_with("World of Warcraft") {
                    push(entry.get_value("InstallLocation"));
                }
            }
        }
        paths
    }

    #[cfg(not(windows))]
    fn paths(&self) -> Vec<(PathBuf, CandidateSource)> {
        Vec::new()
    }
}

/// Where Battle.net installs WoW by default, plus common hand-picked spots.
pub struct CommonPathSource;

impl InstallSource for CommonPathSource {
    fn paths(&self) -> Vec<(PathBuf, CandidateSource)> {
        let mut paths: Vec<PathBuf> = Vec::new();

        #[cfg(windows)]
        {
            for var in ["ProgramFiles(x86)", "ProgramFiles"] {
                if let Some(dir) = std::env::var_os(var) {
                    paths.push(PathBuf::from(dir).join("World of Warcraft"));
                }
            }
            // Skips A:/B: (floppy letters, which can stall when probed).
            for letter in 'C'..='Z' {
                let drive = PathBuf::from(format!("{letter}:\\"));
                if drive.exists() {
                    paths.push(drive.join("World of Warcraft"));
                    paths.push(drive.join("Games").join("World of Warcraft"));
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            paths.push(PathBuf::from("/Applications/World of Warcraft"));
            if let Some(home) = std::env::var_os("HOME") {
                paths.push(PathBuf::from(home).join("Applications/World of Warcraft"));
            }
        }

        paths
            .into_iter()
            .map(|p| (p, CandidateSource::CommonPath))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_copy;

    struct Fixed(Vec<(PathBuf, CandidateSource)>);

    impl InstallSource for Fixed {
        fn paths(&self) -> Vec<(PathBuf, CandidateSource)> {
            self.0.clone()
        }
    }

    #[test]
    fn finds_validates_and_dedupes() {
        let (dir, root) = fixture_copy();
        let empty = dir.path().join("not-wow");
        std::fs::create_dir(&empty).unwrap();

        let sources: Vec<Box<dyn InstallSource + Send>> = vec![
            Box::new(Fixed(vec![
                // Registry InstallPath pointing at a flavor folder.
                (root.join("_classic_beta_"), CandidateSource::Registry),
                (dir.path().join("missing"), CandidateSource::Registry),
            ])),
            Box::new(Fixed(vec![
                (empty, CandidateSource::CommonPath),
                (root.clone(), CandidateSource::CommonPath), // duplicate
            ])),
        ];

        let found = detect(&sources);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, CandidateSource::Registry);
        assert_eq!(found[0].install.root, dunce::canonicalize(&root).unwrap());
    }

    #[test]
    fn system_sources_do_not_panic() {
        // Real registry/disk probing; results depend on the machine.
        let _ = detect(&system_sources());
    }
}
