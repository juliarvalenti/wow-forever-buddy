//! Game install detection and the active install (spec §1).
//!
//! - `layout`: the `Install`/`Flavor` model and the data-driven flavor table.
//! - `validate`: folder → validated `Install`, and choosing the active flavor.
//! - `detect`: the saved path, registry and common-path sources.
//! - `wtf`: character folder names (older pre-surname folders).

pub mod detect;
pub mod layout;
pub mod validate;
pub mod wtf;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::settings::{InstallChoice, SettingsStore};
use crate::error::{AppError, AppResult};
use crate::fsx::atomic::sweep_temp_files;
use crate::fsx::relpath::{GameRoot, LinkedFolder};
use detect::{
    detect, detect_report, system_sources, CandidateSource, DetectReport, FixedPaths, InstallSource,
};
use layout::Install;
use validate::{choose_flavor, locate, scan};

/// Emitted when the active install is set or resolved at startup.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct InstallChanged {
    pub install: Option<Install>,
}

/// The saved install, re-validated (the folder may have moved). `Ok(None)`
/// when none is saved.
///
/// Linked folders must still point where they did when the user confirmed
/// the install. If one was re-pointed, added or removed, this fails rather
/// than quietly following it; picking the folder again (`set`) re-records.
pub fn current(settings: &SettingsStore) -> AppResult<Option<Install>> {
    let Some(choice) = settings.get().install else {
        return Ok(None);
    };
    let mut install = scan(&choice.root)?;
    let Some(flavor) = install.flavor(&choice.flavor) else {
        return Err(AppError::InvalidInstall(format!(
            "{} no longer has a {} folder",
            install.root.display(),
            choice.flavor
        )));
    };
    if let Some(change) = link_change(&choice.links, &flavor.links) {
        return Err(AppError::InvalidInstall(format!(
            "{change}; pick the game folder again to confirm"
        )));
    }
    install.active = Some(choice.flavor);
    Ok(Some(install))
}

/// Describes the first difference between the recorded links and the links
/// found now, or `None` if they match.
fn link_change(saved: &[LinkedFolder], now: &[LinkedFolder]) -> Option<String> {
    let find = |links: &[LinkedFolder], folder: &str| {
        links
            .iter()
            .find(|l| l.folder == folder)
            .map(|l| l.target.clone())
    };
    let mut folders: Vec<&str> = saved.iter().chain(now).map(|l| l.folder.as_str()).collect();
    folders.sort();
    folders.dedup();
    for folder in folders {
        match (find(saved, folder), find(now, folder)) {
            (Some(was), Some(is)) if !same_path(&was, &is) => {
                return Some(format!(
                    "{folder} is now linked to {} instead of {}",
                    is.display(),
                    was.display()
                ))
            }
            (Some(was), None) => {
                return Some(format!("{folder} is no longer linked to {}", was.display()))
            }
            (None, Some(is)) => return Some(format!("{folder} is now linked to {}", is.display())),
            _ => {}
        }
    }
    None
}

/// Case-insensitive, like the Windows and default macOS file systems.
fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

impl GameRoot {
    /// The game root for the saved install, with the link targets recorded
    /// when the user confirmed it. Game-file access (T6 write gate, T7
    /// backups) must build its root here, never with `GameRoot::new` at use
    /// time, which would re-record whatever a link points to now.
    #[allow(dead_code)] // first callers: the write gate (T6) and backups (T7)
    pub fn from_saved(choice: &InstallChoice) -> AppResult<Self> {
        let base = dunce::canonicalize(choice.root.join(&choice.flavor))?;
        Ok(Self {
            base,
            links: choice.links.clone(),
        })
    }
}

/// Validates a picked folder (the root, a flavor folder, `WTF` or deeper),
/// chooses the active flavor and saves both. Picking inside a flavor folder
/// selects that flavor unless `flavor` says otherwise.
pub fn set(settings: &SettingsStore, path: &Path, flavor: Option<&str>) -> AppResult<Install> {
    let (root, inside) = locate(path).ok_or_else(|| {
        AppError::InvalidInstall(format!(
            "{} isn't a World of Warcraft folder",
            path.display()
        ))
    })?;
    let mut install = scan(&root)?;
    let picked = flavor
        .map(str::to_string)
        .or_else(|| inside.filter(|id| install.flavor(id).is_some()));
    let id = choose_flavor(&install, picked.as_deref())?;
    // The user is confirming this folder, so this is where links get recorded.
    let links = install
        .flavor(&id)
        .map(|f| f.links.clone())
        .unwrap_or_default();
    settings.update(|s| {
        s.install = Some(InstallChoice {
            root: install.root.clone(),
            flavor: id.clone(),
            links,
        })
    })?;
    install.active = Some(id);
    sweep(&install);
    Ok(install)
}

/// Every install we can find: the saved one first (with its saved flavor
/// active), then registry and common paths, plus every place we looked.
pub fn detect_all(settings: &SettingsStore) -> DetectReport {
    let saved = settings.get().install;
    let mut sources: Vec<Box<dyn InstallSource>> = Vec::new();
    if let Some(choice) = &saved {
        sources.push(Box::new(FixedPaths(
            CandidateSource::Saved,
            vec![choice.root.clone()],
        )));
    }
    sources.extend(system_sources());
    let mut report = detect_report(&sources);
    if let (Some(choice), Some(first)) = (&saved, report.candidates.first_mut()) {
        if first.source == CandidateSource::Saved && first.install.flavor(&choice.flavor).is_some()
        {
            first.install.active = Some(choice.flavor.clone());
        }
    }
    report
}

/// Startup step 4. Returns the saved install if it's still valid. If nothing
/// is saved, saves and returns the first detected install whose flavor is
/// unambiguous. A saved install that no longer validates is left alone (the
/// drive may just be unplugged); `install_get` reports the error.
pub fn resolve_on_startup(settings: &SettingsStore) -> Option<Install> {
    resolve_with(settings, &system_sources())
}

fn resolve_with(settings: &SettingsStore, sources: &[Box<dyn InstallSource>]) -> Option<Install> {
    if settings.get().install.is_some() {
        let install = current(settings).ok()??;
        sweep(&install);
        return Some(install);
    }
    let found = detect(sources)
        .into_iter()
        .find(|c| c.install.active.is_some())?;
    set(
        settings,
        &found.install.root,
        found.install.active.as_deref(),
    )
    .ok()
}

/// Removes temp files a crash mid-write left in the active WTF folder.
fn sweep(install: &Install) {
    if let Some(flavor) = install.active_flavor() {
        let _ = sweep_temp_files(&flavor.dir.join("WTF"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsx::relpath::RelPath;
    use crate::test_support::{fixture_copy, link_dir, unlink_dir};

    fn store() -> (tempfile::TempDir, SettingsStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(tmp.path().join("settings.json")).unwrap();
        (tmp, store)
    }

    fn sources(paths: Vec<std::path::PathBuf>) -> Vec<Box<dyn InstallSource>> {
        vec![Box::new(FixedPaths(CandidateSource::CommonPath, paths))]
    }

    #[test]
    fn set_saves_and_current_revalidates() {
        let (_s, settings) = store();
        let (_w, root) = fixture_copy();
        assert_eq!(current(&settings).unwrap(), None);

        let install = set(&settings, &root, None).unwrap();
        assert_eq!(install.active.as_deref(), Some("_classic_beta_"));
        let saved = settings.get().install.unwrap();
        assert_eq!(saved.root, install.root);
        assert_eq!(saved.flavor, "_classic_beta_");
        assert_eq!(current(&settings).unwrap(), Some(install));

        // The folder goes away: current() reports it rather than guessing.
        std::fs::rename(root.join("_classic_beta_"), root.join("moved")).unwrap();
        assert!(matches!(
            current(&settings),
            Err(AppError::InvalidInstall(_))
        ));
    }

    #[test]
    fn picking_inside_a_flavor_selects_it() {
        let (_s, settings) = store();
        let (_w, root) = fixture_copy();
        let install = set(&settings, &root.join("_classic_era_/WTF"), None).unwrap();
        assert_eq!(install.active.as_deref(), Some("_classic_era_"));
        // An explicit flavor wins.
        let install = set(
            &settings,
            &root.join("_classic_era_"),
            Some("_classic_beta_"),
        )
        .unwrap();
        assert_eq!(install.active.as_deref(), Some("_classic_beta_"));
    }

    #[test]
    fn set_rejects_bad_input_without_saving() {
        let (_s, settings) = store();
        let (_w, root) = fixture_copy();
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(
            set(&settings, tmp.path(), None),
            Err(AppError::InvalidInstall(_))
        ));
        assert!(set(&settings, &root, Some("_retail_")).is_err());
        assert_eq!(settings.get().install, None);
    }

    #[test]
    fn set_sweeps_leftover_temp_files() {
        let (_s, settings) = store();
        let (_w, root) = fixture_copy();
        let leftover =
            root.join("_classic_beta_/WTF/Account/ACCOUNT1/SavedVariables/.Details.lua.wfb-tmp-1");
        std::fs::write(&leftover, b"partial").unwrap();
        set(&settings, &root, None).unwrap();
        assert!(!leftover.exists());
    }

    #[test]
    fn startup_detects_and_saves_when_nothing_is_saved() {
        let (_s, settings) = store();
        let (_w, root) = fixture_copy();
        let install = resolve_with(&settings, &sources(vec![root.clone()])).unwrap();
        assert_eq!(install.active.as_deref(), Some("_classic_beta_"));
        assert_eq!(settings.get().install.unwrap().flavor, "_classic_beta_");
    }

    #[test]
    fn startup_keeps_a_saved_install_even_if_it_is_missing() {
        let (_s, settings) = store();
        let (_w, root) = fixture_copy();
        let (_w2, other) = fixture_copy();
        set(&settings, &root, Some("_classic_era_")).unwrap();
        let saved = settings.get().install;

        // Still valid: returned as saved, not replaced by detection.
        let install = resolve_with(&settings, &sources(vec![other.clone()])).unwrap();
        assert_eq!(install.active.as_deref(), Some("_classic_era_"));

        // Gone: nothing is returned and the saved choice is untouched.
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(resolve_with(&settings, &sources(vec![other])), None);
        assert_eq!(settings.get().install, saved);
    }

    #[test]
    fn startup_with_nothing_found() {
        let (_s, settings) = store();
        assert_eq!(resolve_with(&settings, &sources(vec![])), None);
        assert_eq!(settings.get().install, None);
    }

    /// Moves the fixture's Forever WTF to `<tmp>/<name>` and links it back.
    /// Returns the flavor folder and the canonical link target.
    fn link_wtf_to(
        tmp: &Path,
        root: &Path,
        name: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let flavor = root.join("_classic_beta_");
        let target = tmp.join(name);
        std::fs::rename(flavor.join("WTF"), &target).unwrap();
        link_dir(&target, &flavor.join("WTF"));
        (flavor, dunce::canonicalize(&target).unwrap())
    }

    #[test]
    fn set_records_links_and_a_repointed_link_is_refused() {
        let (_s, settings) = store();
        let (tmp, root) = fixture_copy();
        let (flavor, first) = link_wtf_to(tmp.path(), &root, "wtf-first");

        set(&settings, &root, None).unwrap();
        let saved = settings.get().install.unwrap();
        assert_eq!(
            saved.links,
            [LinkedFolder {
                folder: "WTF".into(),
                target: first.clone()
            }]
        );
        assert!(current(&settings).unwrap().is_some());

        // Someone re-points the junction elsewhere.
        let second = tmp.path().join("wtf-second");
        std::fs::create_dir_all(second.join("Account")).unwrap();
        unlink_dir(&flavor.join("WTF"));
        link_dir(&second, &flavor.join("WTF"));

        let err = current(&settings).unwrap_err().to_string();
        assert!(err.contains("WTF is now linked to"), "{err}");
        assert!(err.contains("instead of"), "{err}");
        // Nothing was silently re-recorded, and startup doesn't accept it either.
        assert_eq!(settings.get().install, Some(saved.clone()));
        assert_eq!(resolve_with(&settings, &sources(vec![])), None);
        assert_eq!(settings.get().install, Some(saved.clone()));

        // Game paths built from the saved install refuse the new target.
        let game = GameRoot::from_saved(&saved).unwrap();
        assert!(RelPath::new("WTF/Config.wtf")
            .unwrap()
            .resolve(&game)
            .is_err());

        // Picking the folder again is the user confirming: it re-records.
        set(&settings, &root, None).unwrap();
        let resaved = settings.get().install.unwrap();
        assert_eq!(
            resaved.links[0].target,
            dunce::canonicalize(&second).unwrap()
        );
        assert!(current(&settings).unwrap().is_some());
    }

    #[test]
    fn a_folder_that_becomes_a_link_or_stops_being_one_is_refused() {
        let (_s, settings) = store();
        let (tmp, root) = fixture_copy();
        set(&settings, &root, None).unwrap();
        assert!(settings.get().install.unwrap().links.is_empty());

        // Plain WTF turned into a link after confirming.
        let (flavor, _) = link_wtf_to(tmp.path(), &root, "moved-wtf");
        let err = current(&settings).unwrap_err().to_string();
        assert!(err.contains("WTF is now linked to"), "{err}");

        // Confirm the link, then turn it back into a plain folder.
        set(&settings, &root, None).unwrap();
        unlink_dir(&flavor.join("WTF"));
        std::fs::create_dir_all(flavor.join("WTF/Account")).unwrap();
        let err = current(&settings).unwrap_err().to_string();
        assert!(err.contains("WTF is no longer linked"), "{err}");
    }

    #[test]
    fn from_saved_uses_the_recorded_links() {
        let (_s, settings) = store();
        let (tmp, root) = fixture_copy();
        let (_, target) = link_wtf_to(tmp.path(), &root, "wtf");
        set(&settings, &root, None).unwrap();
        let game = GameRoot::from_saved(&settings.get().install.unwrap()).unwrap();
        assert_eq!(game.links[0].target, target);
        let path = RelPath::new("WTF/Config.wtf")
            .unwrap()
            .resolve(&game)
            .unwrap();
        assert!(path.ends_with("Config.wtf"));
    }

    #[test]
    fn older_settings_without_links_still_load() {
        let choice: InstallChoice = serde_json::from_value(
            serde_json::json!({ "root": "/games/wow", "flavor": "_classic_beta_" }),
        )
        .unwrap();
        assert!(choice.links.is_empty());
    }
}
