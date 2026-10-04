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

    /// Joins onto `base` and proves the result is still inside it after
    /// resolving symlinks/junctions: the deepest existing ancestor of the
    /// target must canonicalize to somewhere under the canonical base.
    /// Comparison is case-insensitive (Windows and default macOS volumes).
    pub fn resolve(&self, base: &Path) -> AppResult<PathBuf> {
        let base_canon = dunce::canonicalize(base)?;
        let joined = self.0.iter().fold(base_canon.clone(), |p, c| p.join(c));

        let mut existing = joined.as_path();
        while !existing.exists() {
            existing = existing
                .parent()
                .expect("joined path always has the base as an ancestor");
        }
        let existing_canon = dunce::canonicalize(existing)?;
        if !starts_with_ignore_case(&existing_canon, &base_canon) {
            return Err(AppError::PathEscape(format!(
                "{} resolves outside {}",
                self,
                base.display()
            )));
        }
        Ok(joined)
    }
}

fn is_dos_device(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit())
}

fn starts_with_ignore_case(path: &Path, prefix: &Path) -> bool {
    let mut path = path.components();
    prefix.components().all(|p| {
        path.next().is_some_and(|c| {
            c.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&p.as_os_str().to_string_lossy())
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

    #[test]
    fn resolve_stays_inside_base() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("WTF/Account")).unwrap();

        let existing = RelPath::new("WTF/Account")
            .unwrap()
            .resolve(tmp.path())
            .unwrap();
        assert!(existing.ends_with("WTF/Account"));

        // Not-yet-existing targets resolve through their existing ancestor.
        let new = RelPath::new("WTF/Account/NEW/SavedVariables/x.lua")
            .unwrap()
            .resolve(tmp.path())
            .unwrap();
        assert!(new.ends_with("x.lua"));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_symlink_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("link")).unwrap();

        let err = RelPath::new("link/evil.txt").unwrap().resolve(&base);
        assert!(matches!(err, Err(AppError::PathEscape(_))));
    }

    #[cfg(windows)]
    #[test]
    fn resolve_rejects_junction_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        // Junctions need no special privilege, unlike symlinks.
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(base.join("link"))
            .arg(&outside)
            .status()
            .unwrap();
        assert!(status.success());

        let err = RelPath::new("link/evil.txt").unwrap().resolve(&base);
        assert!(matches!(err, Err(AppError::PathEscape(_))));
    }

    #[test]
    fn from_under_builds_relative_paths() {
        let base = Path::new("/games/wow/_classic_beta_");
        let p = RelPath::from_under(base, &base.join("WTF").join("Config.wtf")).unwrap();
        assert_eq!(p.as_string(), "WTF/Config.wtf");
        assert!(RelPath::from_under(base, Path::new("/elsewhere/x")).is_err());
    }
}
