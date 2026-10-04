use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// A path relative to a base folder (usually the flavor dir) that can't name
/// anything outside it (spec §4). Stored as components and written with `/`,
/// which is also the format backup manifests use (`WTF/Account/X/...`).
///
/// Rejected: empty paths, absolute paths, drive (`C:`) and UNC prefixes, `.`
/// and `..`, `:` anywhere (drives, NTFS alternate data streams), NUL,
/// components ending in a dot or space (Windows strips those, so `a.` would
/// alias `a`), and DOS device names like `CON` or `com1.txt`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(try_from = "String", into = "String")]
#[specta(transparent)]
pub struct RelPath(#[specta(type = String)] Vec<String>);

impl RelPath {
    pub fn new(raw: &str) -> AppResult<Self> {
        let escape = |why: &str| AppError::PathEscape(format!("{raw:?}: {why}"));

        if raw.is_empty() {
            return Err(escape("empty path"));
        }
        if raw.starts_with(['/', '\\']) {
            return Err(escape("absolute path"));
        }

        let mut parts = Vec::new();
        for part in raw.split(['/', '\\']) {
            if part.is_empty() {
                continue; // tolerate "a//b" and a trailing slash
            }
            if part == "." || part == ".." {
                return Err(escape("relative components are not allowed"));
            }
            if part.contains(':') {
                return Err(escape(
                    "':' is not allowed (drive or alternate data stream)",
                ));
            }
            if part.contains('\0') {
                return Err(escape("NUL byte"));
            }
            if part.ends_with(['.', ' ']) {
                return Err(escape("component ends with a dot or space"));
            }
            if is_dos_device(part) {
                return Err(escape("reserved device name"));
            }
            parts.push(part.to_string());
        }
        if parts.is_empty() {
            return Err(escape("empty path"));
        }
        Ok(Self(parts))
    }

    pub fn as_string(&self) -> String {
        self.0.join("/")
    }

    pub fn file_name(&self) -> &str {
        self.0.last().expect("RelPath is never empty")
    }

    /// Builds the relative form of `path` under `base`, for paths found by
    /// walking a folder. Fails if `path` isn't under `base`.
    pub fn from_under(base: &Path, path: &Path) -> AppResult<Self> {
        let rel = path.strip_prefix(base).map_err(|_| {
            AppError::PathEscape(format!(
                "{} is not under {}",
                path.display(),
                base.display()
            ))
        })?;
        let parts: Vec<&str> = rel
            .components()
            .map(|c| match c {
                Component::Normal(s) => s.to_str().ok_or_else(|| {
                    AppError::PathEscape(format!("non-UTF-8 path: {}", path.display()))
                }),
                _ => Err(AppError::PathEscape(format!(
                    "unexpected component in {}",
                    path.display()
                ))),
            })
            .collect::<AppResult<_>>()?;
        Self::new(&parts.join("/"))
    }

    fn join_onto(&self, base: &Path) -> PathBuf {
        self.0.iter().fold(base.to_path_buf(), |p, c| p.join(c))
    }

    fn starts_with_folder(&self, folder: &str) -> bool {
        let folder: Vec<&str> = folder.split('/').collect();
        self.0.len() >= folder.len() && self.0.iter().zip(&folder).all(|(a, b)| same_name(a, b))
    }

    /// Joins onto the game root and proves the result stays inside what it's
    /// allowed to reach, after resolving symlinks and junctions:
    /// - a path under a linked folder (see `GameRoot`) must land under that
    ///   folder's recorded target, and the link must still point there;
    /// - any other path must land under the game root itself.
    ///
    /// The check is on the deepest part of the path that exists, found
    /// without following a final link, so a dangling link is rejected too.
    /// Names compare case-insensitively (Windows and default macOS volumes).
    pub fn resolve(&self, root: &GameRoot) -> AppResult<PathBuf> {
        let escape = |why: String| AppError::PathEscape(format!("{self}: {why}"));
        let joined = self.join_onto(&root.base);

        let allowed = match root
            .links
            .iter()
            .find(|l| self.starts_with_folder(&l.folder))
        {
            Some(link) => {
                let literal = RelPath::new(&link.folder)?.join_onto(&root.base);
                let now = dunce::canonicalize(&literal).map_err(|_| {
                    escape(format!(
                        "the {} link is broken; re-check the game folder",
                        link.folder
                    ))
                })?;
                if !same_path(&now, &link.target) {
                    return Err(escape(format!(
                        "{} now points to {} instead of {}; re-check the game folder",
                        link.folder,
                        now.display(),
                        link.target.display()
                    )));
                }
                link.target.as_path()
            }
            None => root.base.as_path(),
        };

        let mut existing = joined.as_path();
        while std::fs::symlink_metadata(existing).is_err() {
            existing = existing
                .parent()
                .expect("the game root exists, so the walk stops there at the latest");
        }
        let existing_canon = dunce::canonicalize(existing)
            .map_err(|_| escape("goes through a broken link".into()))?;
        if !starts_with_ignore_case(&existing_canon, allowed) {
            return Err(escape(format!("resolves outside {}", allowed.display())));
        }
        Ok(joined)
    }
}

/// Folders players commonly link elsewhere, e.g. a `WTF` junction into
/// Dropbox or OneDrive to sync settings between PCs (spec §4).
const LINKABLE_FOLDERS: [&str; 2] = ["WTF", "Interface/AddOns"];

/// The game folder `RelPath`s resolve against, plus where its linkable
/// folders really live. Build it when validating the install, so a link that
/// is re-pointed later is caught instead of trusted.
///
/// Built only in Rust, by `GameRoot::new`. It must never be a command
/// argument: whoever supplies `links[].target` chooses the allowed roots.
/// (It's serializable so the UI can show "WTF is linked to …".)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct GameRoot {
    /// Canonical flavor folder, e.g. `C:\...\World of Warcraft\_classic_beta_`.
    pub base: PathBuf,
    /// Linkable folders that point outside their literal location.
    pub links: Vec<LinkedFolder>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct LinkedFolder {
    /// `/`-separated path relative to the game root, e.g. "WTF".
    pub folder: String,
    /// Canonical target recorded at validation, e.g. `D:\Dropbox\WTF`.
    /// The UI shows it as "WTF is linked to …".
    pub target: PathBuf,
}

impl GameRoot {
    pub fn new(flavor_dir: &Path) -> AppResult<Self> {
        let base = dunce::canonicalize(flavor_dir)?;
        let mut links = Vec::new();
        for folder in LINKABLE_FOLDERS {
            let literal = RelPath::new(folder)?.join_onto(&base);
            if std::fs::symlink_metadata(&literal).is_err() {
                continue;
            }
            // A dangling link records nothing; paths through it are rejected.
            let Ok(target) = dunce::canonicalize(&literal) else {
                continue;
            };
            if !same_path(&target, &literal) {
                links.push(LinkedFolder {
                    folder: folder.to_string(),
                    target,
                });
            }
        }
        Ok(Self { base, links })
    }
}

fn is_dos_device(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit())
}

/// Case-insensitive, including non-ASCII names (accented realm and character
/// names are common).
fn same_name(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.components().count() == b.components().count() && starts_with_ignore_case(a, b)
}

fn starts_with_ignore_case(path: &Path, prefix: &Path) -> bool {
    let mut path = path.components();
    prefix.components().all(|p| {
        path.next().is_some_and(|c| {
            same_name(
                &c.as_os_str().to_string_lossy(),
                &p.as_os_str().to_string_lossy(),
            )
        })
    })
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_string())
    }
}

impl TryFrom<String> for RelPath {
    type Error = AppError;
    fn try_from(s: String) -> AppResult<Self> {
        Self::new(&s)
    }
}

impl From<RelPath> for String {
    fn from(p: RelPath) -> String {
        p.as_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_normalizes_separators() {
        let p = RelPath::new(r"WTF\Account\ACC#1\SavedVariables\Foo.lua").unwrap();
        assert_eq!(p.as_string(), "WTF/Account/ACC#1/SavedVariables/Foo.lua");
        assert_eq!(p.file_name(), "Foo.lua");
        assert_eq!(RelPath::new("a//b/").unwrap().as_string(), "a/b");
        assert_eq!(
            RelPath::new("WTF/Account/X/Realm Name/Char/layout-local.txt")
                .unwrap()
                .as_string(),
            "WTF/Account/X/Realm Name/Char/layout-local.txt"
        );
    }

    #[test]
    fn rejects_escapes_and_aliases() {
        for bad in [
            "",
            "/",
            "/etc/passwd",
            r"\Windows",
            r"\\server\share\x",
            r"\\?\C:\x",
            "C:",
            r"C:\x",
            "C:x",
            "..",
            "a/../../b",
            "./a",
            "a/./b",
            "file.txt:stream",
            "a\0b",
            "dir./x",
            "x ",
            "CON",
            "nul.txt",
            "a/COM1",
            "LPT9.log",
        ] {
            assert!(
                matches!(RelPath::new(bad), Err(AppError::PathEscape(_))),
                "should reject {bad:?}"
            );
        }
        // Lookalikes that are fine.
        for ok in ["CONFIG.wtf", "console.txt", "COM10", "auxiliary"] {
            assert!(RelPath::new(ok).is_ok(), "should accept {ok:?}");
        }
    }

    #[test]
    fn serde_round_trip_validates() {
        let p: RelPath = serde_json::from_str(r#""WTF/Config.wtf""#).unwrap();
        assert_eq!(serde_json::to_string(&p).unwrap(), r#""WTF/Config.wtf""#);
        assert!(serde_json::from_str::<RelPath>(r#""../x""#).is_err());
    }

    fn rel(s: &str) -> RelPath {
        RelPath::new(s).unwrap()
    }

    fn is_escape(r: AppResult<PathBuf>) -> bool {
        matches!(r, Err(AppError::PathEscape(_)))
    }

    /// A directory link: a symlink on unix, a junction on Windows (junctions
    /// need no special privilege, unlike symlinks).
    fn link_dir(link: &Path, target: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        {
            // cmd reads a '/' inside an argument as a switch, so hand mklink
            // backslashes only.
            let native = |p: &Path| p.to_string_lossy().replace('/', "\\");
            let status = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(native(link))
                .arg(native(target))
                .status()
                .unwrap();
            assert!(status.success(), "mklink /J {link:?} {target:?}");
        }
    }

    fn remove_link(link: &Path) {
        // A junction is removed like a directory on Windows; a symlink like a file on unix.
        #[cfg(windows)]
        std::fs::remove_dir(link).unwrap();
        #[cfg(unix)]
        std::fs::remove_file(link).unwrap();
    }

    /// `base/` (the game root) and `outside/` (somewhere it must not reach).
    fn base_and_outside() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        (tmp, base, outside)
    }

    #[test]
    fn resolve_stays_inside_base() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("WTF/Account")).unwrap();
        let root = GameRoot::new(tmp.path()).unwrap();
        assert!(root.links.is_empty());

        let existing = rel("WTF/Account").resolve(&root).unwrap();
        assert!(existing.ends_with("WTF/Account"));

        // Not-yet-existing targets resolve through their existing ancestor.
        let new = rel("WTF/Account/NEW/SavedVariables/x.lua")
            .resolve(&root)
            .unwrap();
        assert!(new.ends_with("x.lua"));
    }

    #[test]
    fn resolve_rejects_link_escape() {
        let (_tmp, base, outside) = base_and_outside();
        link_dir(&base.join("link"), &outside);
        let root = GameRoot::new(&base).unwrap();
        assert!(is_escape(rel("link/evil.txt").resolve(&root)));
    }

    /// Players junction WTF into Dropbox/OneDrive; that must keep working.
    #[test]
    fn linked_wtf_is_allowed_and_recorded() {
        let (_tmp, base, outside) = base_and_outside();
        let synced = outside.join("Dropbox/WTF");
        std::fs::create_dir_all(synced.join("Account/ACC")).unwrap();
        link_dir(&base.join("WTF"), &synced);

        let root = GameRoot::new(&base).unwrap();
        assert_eq!(root.links.len(), 1);
        assert_eq!(root.links[0].folder, "WTF");
        assert!(same_path(
            &root.links[0].target,
            &dunce::canonicalize(&synced).unwrap()
        ));

        let path = rel("wtf/Account/ACC/SavedVariables/Foo.lua")
            .resolve(&root)
            .unwrap();
        crate::fsx::atomic::atomic_replace(&path, b"synced").unwrap();
        assert_eq!(
            std::fs::read(synced.join("Account/ACC/SavedVariables/Foo.lua")).unwrap(),
            b"synced"
        );
        // Paths outside WTF still have to stay in the game root.
        assert!(rel("Interface/AddOns/X/X.toc").resolve(&root).is_ok());
    }

    #[test]
    fn link_inside_linked_wtf_cannot_escape() {
        let (_tmp, base, outside) = base_and_outside();
        let synced = outside.join("Dropbox/WTF");
        let elsewhere = outside.join("Users/someone");
        std::fs::create_dir_all(synced.join("Account")).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        link_dir(&base.join("WTF"), &synced);
        link_dir(&synced.join("Account/evil"), &elsewhere);

        let root = GameRoot::new(&base).unwrap();
        assert!(is_escape(rel("WTF/Account/evil/x.lua").resolve(&root)));
        // The game root itself is not reachable through the WTF allowance either.
        assert!(rel("WTF/Account").resolve(&root).is_ok());
    }

    #[test]
    fn repointed_wtf_link_is_rejected() {
        let (_tmp, base, outside) = base_and_outside();
        let synced = outside.join("Dropbox/WTF");
        let other = outside.join("Users/someone");
        std::fs::create_dir_all(&synced).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        link_dir(&base.join("WTF"), &synced);
        let root = GameRoot::new(&base).unwrap();

        remove_link(&base.join("WTF"));
        link_dir(&base.join("WTF"), &other);
        assert!(is_escape(rel("WTF/Config.wtf").resolve(&root)));
    }

    /// WTF was a plain folder at validation and became a link afterwards.
    #[test]
    fn wtf_turned_into_a_link_later_is_rejected() {
        let (_tmp, base, outside) = base_and_outside();
        std::fs::create_dir_all(base.join("WTF")).unwrap();
        let root = GameRoot::new(&base).unwrap();

        std::fs::remove_dir(base.join("WTF")).unwrap();
        link_dir(&base.join("WTF"), &outside);
        assert!(is_escape(rel("WTF/Config.wtf").resolve(&root)));
    }

    #[cfg(unix)]
    #[test]
    fn dangling_link_is_rejected() {
        let (_tmp, base, outside) = base_and_outside();
        std::os::unix::fs::symlink(outside.join("gone"), base.join("dangling")).unwrap();
        let root = GameRoot::new(&base).unwrap();
        assert!(is_escape(rel("dangling").resolve(&root)));
        assert!(is_escape(rel("dangling/x.lua").resolve(&root)));
    }

    #[test]
    fn non_ascii_names_compare_case_insensitively() {
        assert!(same_name("Lúthien", "LÚTHIEN"));
        assert!(!same_name("Lúthien", "Luthien"));
        assert!(rel("WTF/Account/A/Pyrewood Village/Lúthien").starts_with_folder("wtf"));
    }

    #[test]
    fn from_under_builds_relative_paths() {
        let base = Path::new("/games/wow/_classic_beta_");
        let p = RelPath::from_under(base, &base.join("WTF").join("Config.wtf")).unwrap();
        assert_eq!(p.as_string(), "WTF/Config.wtf");
        assert!(RelPath::from_under(base, Path::new("/elsewhere/x")).is_err());
    }
}
