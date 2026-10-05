//! The whole chain against a small install built here: `.build.info`, a
//! build config, an encoding and root file, the local index and one archive.

use super::*;
use std::collections::BTreeMap;

const ROOT_CKEY: Key = [0x01; 16];
const ROOT_EKEY: Key = [0x11; 16];
const ENC_CKEY: Key = [0x02; 16];
const ENC_EKEY: Key = [0x12; 16];
const ICON_CKEY: Key = [0x03; 16];
const ICON_EKEY: Key = [0x13; 16];
const LOCKED_CKEY: Key = [0x04; 16];
const LOCKED_EKEY: Key = [0x14; 16];
const STREAMED_CKEY: Key = [0x05; 16];
const STREAMED_EKEY: Key = [0x15; 16];
const BUILD_KEY: &str = "0123456789abcdef0123456789abcdef";

/// An icon that reads, one whose chunk is encrypted, and one that's in the
/// build but not downloaded yet (until `stream_in`).
pub(crate) const ICON: u32 = 136_235;
pub(crate) const LOCKED: u32 = 136_236;
pub(crate) const STREAMED: u32 = 136_237;

/// A built install; the app's icon tests use it too.
pub(crate) struct Install {
    _tmp: tempfile::TempDir,
    pub flavor: PathBuf,
    pub data: PathBuf,
    entries: BTreeMap<usize, Vec<(Key, idx::Location)>>,
}

impl Install {
    /// What Battle.net does when the game first needs a streamed file: it's
    /// appended to an archive, and its bucket's index gets a new version.
    pub(crate) fn stream_in(&mut self) {
        let data_dir = self.data.join("data");
        let path = data_dir.join("data.000");
        let mut archive = std::fs::read(&path).unwrap();
        let blte = blte::tests::encode(&[(b'Z', &icon_blp())]);
        let loc = append(&mut archive, STREAMED_EKEY, blte);
        std::fs::write(&path, archive).unwrap();
        let bucket = idx::bucket(&STREAMED_EKEY);
        let entries = self.entries.entry(bucket).or_default();
        entries.push((STREAMED_EKEY, loc));
        std::fs::write(
            data_dir.join(format!("{bucket:02x}00000003.idx")),
            idx::tests::build(bucket as u8, entries),
        )
        .unwrap();
    }
}

impl Install {
    /// Breaks ICON's BLTE magic in the archive, leaving everything else
    /// (encoding, root) readable: damaged data, as opposed to absent.
    pub(crate) fn damage_icon(&self) {
        let path = self.data.join("data").join("data.000");
        let mut archive = std::fs::read(&path).unwrap();
        let mut reversed = ICON_EKEY;
        reversed.reverse();
        let at = archive
            .windows(16)
            .position(|w| w == reversed)
            .expect("ICON's header");
        archive[at + ARCHIVE_HEADER] = b'X';
        std::fs::write(&path, archive).unwrap();
    }
}

/// Adds one file (30-byte header, then its BLTE) to an archive.
fn append(archive: &mut Vec<u8>, ekey: Key, blte: Vec<u8>) -> idx::Location {
    let size = (ARCHIVE_HEADER + blte.len()) as u32;
    let loc = idx::Location {
        archive: 0,
        offset: archive.len() as u64,
        size,
    };
    let mut reversed = ekey;
    reversed.reverse();
    archive.extend(reversed);
    archive.extend(size.to_le_bytes());
    archive.extend([0; 10]);
    archive.extend(blte);
    loc
}

fn icon_blp() -> Vec<u8> {
    // 4x4 DXT1, solid red.
    blp::tests::build(2, 0, 0, 4, 4, &[], &[0x00, 0xF8, 0, 0, 0, 0, 0, 0])
}

pub(crate) fn install() -> Install {
    let tmp = tempfile::tempdir().unwrap();
    let flavor = tmp.path().join("_classic_beta_");
    let data = tmp.path().join("Data");
    std::fs::create_dir_all(&flavor).unwrap();
    std::fs::write(
        flavor.join(".flavor.info"),
        "Product Flavor!STRING:0\nwow_classic_beta\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join(".build.info"),
        format!(
            "Branch!STRING:0|Active!DEC:1|Build Key!HEX:16|Version!STRING:0|Product!STRING:0\n\
             us|1|{BUILD_KEY}|1.60.1.70205|wow_classic_beta\n"
        ),
    )
    .unwrap();
    let config = data.join("config").join("01").join("23");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join(BUILD_KEY),
        format!(
            "root = {}\nencoding = {} {}\n",
            config::hex(&ROOT_CKEY),
            config::hex(&ENC_CKEY),
            config::hex(&ENC_EKEY)
        ),
    )
    .unwrap();

    let encoding = encoding::tests::build(&[&[
        (ROOT_CKEY, ROOT_EKEY),
        (ICON_CKEY, ICON_EKEY),
        (LOCKED_CKEY, LOCKED_EKEY),
        (STREAMED_CKEY, STREAMED_EKEY),
    ]]);
    let root = root::tests::build(&[(
        0xFFFF_FFFF,
        0,
        &[
            (ICON, ICON_CKEY),
            (LOCKED, LOCKED_CKEY),
            (STREAMED, STREAMED_CKEY),
        ],
    )]);
    let files: [(Key, Vec<u8>); 4] = [
        (ENC_EKEY, blte::tests::encode(&[(b'Z', &encoding)])),
        (ROOT_EKEY, blte::tests::encode(&[(b'N', &root)])),
        (ICON_EKEY, blte::tests::encode(&[(b'Z', &icon_blp())])),
        (
            LOCKED_EKEY,
            blte::tests::encode(&[(b'E', b"key name + iv")]),
        ),
    ];

    let mut archive = Vec::new();
    let mut by_bucket: BTreeMap<usize, Vec<(Key, idx::Location)>> = BTreeMap::new();
    for (ekey, blte) in files {
        let loc = append(&mut archive, ekey, blte);
        by_bucket
            .entry(idx::bucket(&ekey))
            .or_default()
            .push((ekey, loc));
    }
    let data_dir = data.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("data.000"), archive).unwrap();
    for (bucket, entries) in &by_bucket {
        let idx = idx::tests::build(*bucket as u8, entries);
        // An older, empty version alongside, which must be ignored.
        std::fs::write(data_dir.join(format!("{bucket:02x}00000001.idx")), b"").unwrap();
        std::fs::write(data_dir.join(format!("{bucket:02x}00000002.idx")), idx).unwrap();
    }
    Install {
        _tmp: tmp,
        flavor,
        data,
        entries: by_bucket,
    }
}

/// Every file under `dir` with its bytes and mtime.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
    walkdir::WalkDir::new(dir)
        .sort_by_file_name()
        .into_iter()
        .map(Result::unwrap)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let p = e.path().to_path_buf();
            let modified = e.metadata().unwrap().modified().unwrap();
            (p.clone(), std::fs::read(&p).unwrap(), modified)
        })
        .collect()
}

#[test]
fn reads_an_icon_through_the_whole_chain_and_caches_it() {
    let game = install();
    let cache_dir = tempfile::tempdir().unwrap();
    let before = snapshot(&game.data);

    let casc = Casc::open(&game.flavor).unwrap();
    assert_eq!(casc.build.version, "1.60.1.70205");
    let cache = IconCache::new(cache_dir.path());
    let out = cache.fill(&casc, &[ICON]);
    let path = out[0].1.as_ref().unwrap();
    assert!(path.ends_with(format!("wow_classic_beta/{BUILD_KEY}/{ICON}.png")));
    let png = std::fs::read(path).unwrap();
    assert!(png.starts_with(b"\x89PNG"));

    // Nothing under Data was touched.
    assert_eq!(snapshot(&game.data), before);

    // A hit needs only .build.info: it still works with the archive gone.
    std::fs::remove_file(game.data.join("data").join("data.000")).unwrap();
    let build = build_of(&game.flavor).unwrap();
    assert_eq!(cache.cached(&build, ICON).as_ref(), Some(path));
    assert!(cache.fill(&casc, &[ICON])[0].1.is_ok());
}

#[test]
fn each_icon_fails_on_its_own() {
    let game = install();
    let cache_dir = tempfile::tempdir().unwrap();
    let casc = Casc::open(&game.flavor).unwrap();
    let out = IconCache::new(cache_dir.path()).fill(&casc, &[LOCKED, 999, ICON]);
    assert!(matches!(out[0].1, Err(CascError::Encrypted)));
    assert!(matches!(out[1].1, Err(CascError::Missing(_))));
    assert!(out[2].1.is_ok());
    // Failures leave nothing in the cache.
    let build = build_of(&game.flavor).unwrap();
    assert!(cache_path(cache_dir.path(), &build, LOCKED)
        .parent()
        .unwrap()
        .is_dir());
    assert!(!cache_path(cache_dir.path(), &build, LOCKED).exists());
}

#[test]
fn a_streamed_file_is_not_local_until_the_index_has_it() {
    let mut game = install();
    let cache_dir = tempfile::tempdir().unwrap();
    let casc = Casc::open(&game.flavor).unwrap();
    let out = IconCache::new(cache_dir.path()).fill(&casc, &[STREAMED]);
    assert!(matches!(out[0].1, Err(CascError::NotLocal)));
    assert!(!out[0].1.as_ref().unwrap_err().is_broken());

    let before = index_set(&game.flavor).unwrap();
    assert_eq!(before, casc.index_set());
    game.stream_in();
    let after = index_set(&game.flavor).unwrap();
    assert_ne!(before, after, "a new index version is a new set");

    // The storage opened before still can't see it; a fresh one can.
    let ckey = casc.ckeys(&[STREAMED].into())[&STREAMED];
    assert!(matches!(casc.by_ckey(&ckey), Err(CascError::NotLocal)));
    let fresh = Casc::open(&game.flavor).unwrap();
    assert!(
        IconCache::new(cache_dir.path()).fill(&fresh, &[STREAMED])[0]
            .1
            .is_ok()
    );
}

#[test]
fn a_stale_index_is_caught_by_the_archive_header() {
    let game = install();
    let casc = Casc::open(&game.flavor).unwrap();
    // Shift every file in the archive by one byte: the index now points
    // one byte early, as if the game had rewritten the archive.
    let archive = game.data.join("data").join("data.000");
    let mut bytes = std::fs::read(&archive).unwrap();
    bytes.insert(0, 0);
    std::fs::write(&archive, bytes).unwrap();
    let ckey = casc.ckeys(&[ICON].into())[&ICON];
    assert!(matches!(
        casc.by_ckey(&ckey),
        Err(CascError::Bad("archive header"))
    ));
}

#[test]
fn a_missing_or_foreign_install_is_an_error_not_a_panic() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(Casc::open(&tmp.path().join("_classic_beta_")).is_err());

    // Another product's install: .build.info has no row for ours.
    let game = install();
    std::fs::write(
        game.flavor.join(".flavor.info"),
        "Product Flavor!STRING:0\nwow_classic_era\n",
    )
    .unwrap();
    assert!(matches!(
        Casc::open(&game.flavor),
        Err(CascError::Missing(_))
    ));
}
