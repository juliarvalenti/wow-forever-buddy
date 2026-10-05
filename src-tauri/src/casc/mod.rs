//! Reading files out of the game's local CASC storage (`<WoW root>/Data`) by
//! FileDataID: today, item icons for the addon's `icon_file_id`.
//!
//! The chain, each layer in its own module:
//! `.build.info` → build config → encoding (ckey → ekey) and root
//! (FileDataID → ckey); then the local `.idx` (ekey → archive, offset) and
//! `data.NNN` (a 30-byte header, then BLTE). Icons are BLP2 inside.
//!
//! Read-only: files under `Data` are only ever opened for reading, with full
//! sharing, and only the needed span of an archive is read. Everything is
//! parsed as untrusted (sizes checked and capped, no panics); any failure is
//! an error for that one file, never a crash, and callers treat it as "no
//! icon".

mod blp;
mod blte;
mod bytes;
mod config;
mod encoding;
mod icons;
mod idx;
mod root;

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::fsx::read::{open_shared, safe_read};

pub use config::BuildInfo;
pub use icons::{cache_path, IconCache};

/// A 16-byte content or encoding key.
pub type Key = [u8; 16];

/// Encoding and root are tens of MB on retail builds.
const MAX_META: usize = 512 << 20;
/// Any one other file (an icon is a few KB).
const MAX_FILE: usize = 64 << 20;
const ARCHIVE_HEADER: usize = 30;

#[derive(Debug, thiserror::Error)]
pub enum CascError {
    /// Malformed data; names the structure, never quotes it.
    #[error("malformed {0}")]
    Bad(&'static str),
    #[error("over the size cap")]
    TooBig,
    /// Needs a key we don't have.
    #[error("encrypted")]
    Encrypted,
    #[error("not found: {0}")]
    Missing(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type CascResult<T> = Result<T, CascError>;

/// One installed build's storage, opened.
pub struct Casc {
    pub build: BuildInfo,
    store: Store,
    encoding: encoding::Encoding,
    root: root::Root,
}

/// `Data/data`: the local index and the archives, by encoding key.
struct Store {
    data_dir: PathBuf,
    indexes: [Option<PathBuf>; idx::BUCKETS],
    /// Each bucket's index, read on first use (`None` if it wouldn't parse).
    buckets: [OnceLock<Option<idx::Entries>>; idx::BUCKETS],
}

/// The installed build for a flavor folder (`<WoW root>/_classic_beta_`):
/// cheap, so a cache hit doesn't need the storage opened.
pub fn build_of(flavor_dir: &Path) -> CascResult<BuildInfo> {
    let game_root = flavor_dir
        .parent()
        .ok_or_else(|| CascError::Missing("WoW root".into()))?;
    let flavor = read_text(&flavor_dir.join(".flavor.info"))?;
    let product = config::flavor_product(&flavor).ok_or(CascError::Bad(".flavor.info"))?;
    config::build_info(&read_text(&game_root.join(".build.info"))?, &product)
}

impl Casc {
    pub fn open(flavor_dir: &Path) -> CascResult<Casc> {
        let build = build_of(flavor_dir)?;
        let data = flavor_dir
            .parent()
            .ok_or_else(|| CascError::Missing("WoW root".into()))?
            .join("Data");
        let (root_ckey, encoding_ekey) =
            config::build_config(&read_text(&config_path(&data, &build.build_key))?)?;
        let data_dir = data.join("data");
        let store = Store {
            indexes: idx::newest(&data_dir)?,
            data_dir,
            buckets: std::array::from_fn(|_| OnceLock::new()),
        };
        let encoding = encoding::Encoding::parse(store.read(&encoding_ekey, MAX_META)?)?;
        let root_ekey = encoding
            .ekey(&root_ckey)
            .ok_or_else(|| CascError::Missing("root in encoding".into()))?;
        let root = root::Root::parse(store.read(&root_ekey, MAX_META)?)?;
        Ok(Casc {
            build,
            store,
            encoding,
            root,
        })
    }

    /// Content keys for the FileDataIDs that exist in this build.
    pub fn ckeys(&self, ids: &HashSet<u32>) -> HashMap<u32, Key> {
        self.root.ckeys(ids)
    }

    pub fn by_ckey(&self, ckey: &Key) -> CascResult<Vec<u8>> {
        let ekey = self
            .encoding
            .ekey(ckey)
            .ok_or_else(|| CascError::Missing("file in encoding".into()))?;
        self.store.read(&ekey, MAX_FILE)
    }
}

impl Store {
    fn read(&self, ekey: &Key, cap: usize) -> CascResult<Vec<u8>> {
        let b = idx::bucket(ekey);
        let entries = self.buckets[b].get_or_init(|| {
            let path = self.indexes[b].as_ref()?;
            idx::parse(&safe_read(path).ok()?).ok()
        });
        let prefix: [u8; 9] = ekey[..9].try_into().unwrap_or_default();
        let loc = entries
            .as_ref()
            .ok_or(CascError::Bad("local index"))?
            .get(&prefix)
            .copied()
            .ok_or_else(|| CascError::Missing("file in local index".into()))?;
        let size = loc.size as usize;
        if size < ARCHIVE_HEADER || size - ARCHIVE_HEADER > cap.saturating_add(4096) {
            return Err(CascError::TooBig);
        }
        let path = self.data_dir.join(format!("data.{:03}", loc.archive));
        let mut span = vec![0; size];
        {
            let mut file = open_shared(&path)?;
            file.seek(SeekFrom::Start(loc.offset))?;
            file.read_exact(&mut span)?;
        } // handle closed before decoding
        check_archive_header(&span, ekey, loc.size)?;
        blte::decode(&span[ARCHIVE_HEADER..], cap)
    }
}

/// `data.NNN` puts the encoding key (reversed) and size before each file; a
/// mismatch means the index is stale (the game is patching).
fn check_archive_header(span: &[u8], ekey: &Key, size: u32) -> CascResult<()> {
    let mut b = bytes::Bytes::new(span);
    let mut stored: Key = b.array("archive header")?;
    stored.reverse();
    let stored_size = b.u32_le("archive header")?;
    if stored[..9] != ekey[..9] || stored_size != size {
        return Err(CascError::Bad("archive header"));
    }
    Ok(())
}

fn config_path(data: &Path, key: &Key) -> PathBuf {
    let hex = config::hex(key);
    data.join("config")
        .join(&hex[..2])
        .join(&hex[2..4])
        .join(hex)
}

fn read_text(path: &Path) -> CascResult<String> {
    let bytes = safe_read(path).map_err(|e| CascError::Missing(e.to_string()))?;
    String::from_utf8(bytes).map_err(|_| CascError::Bad("text file encoding"))
}

#[cfg(test)]
mod tests;
