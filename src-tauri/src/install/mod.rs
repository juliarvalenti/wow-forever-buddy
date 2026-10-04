//! Game install detection and validation (spec §1).

pub mod detect;
pub mod layout;

use std::path::Path;

use crate::config::settings::InstallChoice;
use crate::error::AppResult;
use crate::install::layout::{choose_flavor, inspect_root, normalize_to_root, ActiveInstall};

/// Validates a picked folder (root, flavor folder or WTF folder) and resolves
/// the flavor to use.
pub fn resolve(picked: &Path, flavor: Option<&str>) -> AppResult<ActiveInstall> {
    let root = dunce::canonicalize(normalize_to_root(picked))?;
    let install = inspect_root(&root)?;
    let flavor = choose_flavor(&install, flavor)?;
    Ok(ActiveInstall {
        root: install.root,
        flavor,
    })
}

/// Re-validates the install saved in settings. `None` if it's gone (e.g. an
/// unplugged drive); the saved choice is kept so it comes back on its own.
pub fn resolve_saved(choice: &InstallChoice) -> Option<ActiveInstall> {
    resolve(&choice.root, Some(&choice.flavor)).ok()
}
