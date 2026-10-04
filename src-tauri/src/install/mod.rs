//! Game install detection and the active install (spec §1).
//!
//! - `layout`: the `Install`/`Flavor` model and the data-driven flavor table.
//! - `validate`: folder → validated `Install`, and choosing the active flavor.
//! - `detect`: the saved path, registry and common-path sources.

pub mod detect;
pub mod layout;
pub mod validate;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::settings::{InstallChoice, SettingsStore};
use crate::error::{AppError, AppResult};
use crate::fsx::atomic::sweep_temp_files;
use detect::{
    detect, system_sources, CandidateSource, FixedPaths, InstallCandidate, InstallSource,
};
use layout::Install;
use validate::{choose_flavor, normalize_root, scan};

/// Emitted when the active install is set or resolved at startup.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct InstallChanged {
    pub install: Option<Install>,
}

/// The saved install, re-validated (the folder may have moved). `Ok(None)`
/// when none is saved.
pub fn current(settings: &SettingsStore) -> AppResult<Option<Install>> {
    let Some(choice) = settings.get().install else {
        return Ok(None);
    };
    let mut install = scan(&choice.root)?;
    if install.flavor(&choice.flavor).is_none() {
        return Err(AppError::InvalidInstall(format!(
            "{} no longer has a {} folder",
            install.root.display(),
            choice.flavor
        )));
    }
    install.active = Some(choice.flavor);
    Ok(Some(install))
}

/// Validates a picked folder (the root, a flavor folder, `WTF` or deeper),
/// chooses the active flavor and saves both. Picking inside a flavor folder
/// selects that flavor unless `flavor` says otherwise.
pub fn set(settings: &SettingsStore, path: &Path, flavor: Option<&str>) -> AppResult<Install> {
    let root = normalize_root(path).ok_or_else(|| {
        AppError::InvalidInstall(format!(
            "{} isn't a World of Warcraft folder",
            path.display()
        ))
    })?;
    let mut install = scan(&root)?;
    let picked = flavor
        .map(str::to_string)
        .or_else(|| flavor_containing(&install, path));
    let id = choose_flavor(&install, picked.as_deref())?;
    settings.update(|s| {
        s.install = Some(InstallChoice {
            root: install.root.clone(),
            flavor: id.clone(),
        })
    })?;
    install.active = Some(id);
    sweep(&install);
    Ok(install)
}

fn flavor_containing(install: &Install, path: &Path) -> Option<String> {
    let path = dunce::canonicalize(path).ok()?;
    install
        .flavors
        .iter()
        .find(|f| path.starts_with(&f.dir))
        .map(|f| f.id.clone())
}

/// Every install we can find: the saved one first (with its saved flavor
/// active), then registry and common paths.
pub fn detect_all(settings: &SettingsStore) -> Vec<InstallCandidate> {
    let saved = settings.get().install;
    let mut sources: Vec<Box<dyn InstallSource>> = Vec::new();
    if let Some(choice) = &saved {
        sources.push(Box::new(FixedPaths(
            CandidateSource::Saved,
            vec![choice.root.clone()],
        )));
    }
    sources.extend(system_sources());
    let mut found = detect(&sources);
    if let (Some(choice), Some(first)) = (&saved, found.first_mut()) {
        if first.source == CandidateSource::Saved && first.install.flavor(&choice.flavor).is_some()
        {
            first.install.active = Some(choice.flavor.clone());
        }
    }
    found
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
    use crate::test_support::fixture_copy;

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
}
