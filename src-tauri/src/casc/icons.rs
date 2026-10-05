//! Icons as PNGs in the app's own data folder, keyed by build and
//! FileDataID: `<cache>/<build key>/<id>.png`. A patch changes the build key,
//! so a stale icon is never served; a hit needs only `.build.info` read.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{blp, config, BuildInfo, Casc, CascError, CascResult};

pub fn cache_path(cache: &Path, build: &BuildInfo, id: u32) -> PathBuf {
    cache
        .join(config::hex(&build.build_key))
        .join(format!("{id}.png"))
}

pub struct IconCache {
    dir: PathBuf,
}

impl IconCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        IconCache { dir: dir.into() }
    }

    pub fn cached(&self, build: &BuildInfo, id: u32) -> Option<PathBuf> {
        let path = cache_path(&self.dir, build, id);
        path.is_file().then_some(path)
    }

    /// Each icon's PNG, read from the game and cached if it wasn't already.
    /// One root scan covers all of them; each fails on its own.
    pub fn fill(&self, casc: &Casc, ids: &[u32]) -> Vec<(u32, CascResult<PathBuf>)> {
        let missing: HashSet<u32> = ids
            .iter()
            .copied()
            .filter(|&id| self.cached(&casc.build, id).is_none())
            .collect();
        let ckeys = casc.ckeys(&missing);
        ids.iter()
            .map(|&id| {
                let path = cache_path(&self.dir, &casc.build, id);
                if !missing.contains(&id) {
                    return (id, Ok(path));
                }
                let made = ckeys
                    .get(&id)
                    .ok_or_else(|| CascError::Missing("file in root".into()))
                    .and_then(|ckey| casc.by_ckey(ckey))
                    .and_then(|blp| blp::to_png(&blp::decode(&blp)?))
                    .and_then(|png| write(&path, &png))
                    .map(|()| path);
                (id, made)
            })
            .collect()
    }
}

fn write(path: &Path, png: &[u8]) -> CascResult<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::fsx::atomic::atomic_replace(path, png)
        .map_err(|e| std::io::Error::other(e.to_string()).into())
}
