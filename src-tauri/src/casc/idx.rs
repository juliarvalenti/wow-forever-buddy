//! The local index: `Data/data/BBVVVVVVVV.idx`, one file per bucket (the
//! highest version wins), mapping the first 9 bytes of an encoding key to
//! where its BLTE lives in `data.NNN`. <https://wowdev.wiki/CASC#Local_index>

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::bytes::Bytes;
use super::{CascError, CascResult, Key};

pub const BUCKETS: usize = 16;
const ENTRY: usize = 18;

/// Where one file is: `data.{archive:03}` at `offset`, `size` bytes
/// including its 30-byte header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    pub archive: u32,
    pub offset: u64,
    pub size: u32,
}

pub fn bucket(ekey: &Key) -> usize {
    let i = ekey[..9].iter().fold(0u8, |acc, b| acc ^ b);
    usize::from((i & 0xF) ^ (i >> 4))
}

/// The newest index file for each bucket in `data_dir`.
pub fn newest(data_dir: &Path) -> CascResult<[Option<PathBuf>; BUCKETS]> {
    let mut best: [Option<(u32, PathBuf)>; BUCKETS] = Default::default();
    for entry in std::fs::read_dir(data_dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".idx").filter(|s| s.len() == 10) else {
            continue;
        };
        let (Ok(b), Ok(v)) = (
            usize::from_str_radix(&stem[..2], 16),
            u32::from_str_radix(&stem[2..], 16),
        ) else {
            continue;
        };
        if let Some(slot) = best.get_mut(b) {
            if slot.as_ref().is_none_or(|(have, _)| v > *have) {
                *slot = Some((v, path));
            }
        }
    }
    Ok(best.map(|slot| slot.map(|(_, path)| path)))
}

/// One bucket's entries, by 9-byte key prefix.
pub type Entries = HashMap<[u8; 9], Location>;

pub fn parse(data: &[u8]) -> CascResult<Entries> {
    let mut b = Bytes::new(data);
    let header_len = b.u32_le("idx header")? as usize;
    b.skip(4, "idx header")?; // its hash
    let header = b.take(header_len, "idx header")?;
    let mut h = Bytes::new(header);
    let version = h.u16_le("idx header")?;
    h.skip(2, "idx header")?; // bucket, extra bytes
    let size_len = h.u8("idx header")?;
    let offset_len = h.u8("idx header")?;
    let key_len = h.u8("idx header")?;
    let offset_bits = h.u8("idx header")?;
    if version != 7 || (size_len, offset_len, key_len, offset_bits) != (4, 5, 9, 30) {
        return Err(CascError::Bad("idx layout"));
    }
    // Entries start on the next 16-byte boundary.
    let at = (8 + header_len).next_multiple_of(16);
    b.skip(at.saturating_sub(b.pos()), "idx header")?;
    let entries_len = b.u32_le("idx entries")? as usize;
    b.skip(4, "idx entries")?; // their hash
    if !entries_len.is_multiple_of(ENTRY) {
        return Err(CascError::Bad("idx entries"));
    }
    let entries = b.take(entries_len, "idx entries")?;
    let mut out = HashMap::with_capacity(entries_len / ENTRY);
    for e in entries.as_chunks::<ENTRY>().0 {
        let mut key = [0; 9];
        key.copy_from_slice(&e[..9]);
        let packed = e[9..14]
            .iter()
            .fold(0u64, |acc, &b| (acc << 8) | u64::from(b));
        let size = u32::from_le_bytes([e[14], e[15], e[16], e[17]]);
        out.entry(key).or_insert(Location {
            archive: (packed >> 30) as u32,
            offset: packed & ((1 << 30) - 1),
            size,
        });
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A v7 index file holding `entries`.
    pub fn build(bucket: u8, entries: &[(Key, Location)]) -> Vec<u8> {
        let mut header = 7u16.to_le_bytes().to_vec();
        header.extend([bucket, 0, 4, 5, 9, 30]);
        header.extend((1u64 << 38).to_le_bytes());
        let mut out = (header.len() as u32).to_le_bytes().to_vec();
        out.extend([0; 4]);
        out.extend(&header);
        out.resize((8 + header.len()).next_multiple_of(16), 0);
        out.extend(((entries.len() * ENTRY) as u32).to_le_bytes());
        out.extend([0; 4]);
        for (key, loc) in entries {
            out.extend(&key[..9]);
            let packed = (u64::from(loc.archive) << 30) | loc.offset;
            out.extend(&packed.to_be_bytes()[3..]);
            out.extend(loc.size.to_le_bytes());
        }
        out
    }

    #[test]
    fn reads_entries_and_unpacks_archive_and_offset() {
        let loc = Location {
            archive: 3,
            offset: 0x2345_6789,
            size: 1234,
        };
        let key = [0x11; 16];
        let map = parse(&build(0, &[(key, loc)])).unwrap();
        assert_eq!(map.get(&[0x11; 9]), Some(&loc));
    }

    #[test]
    fn bucket_matches_the_wiki_formula() {
        let mut key = [0; 16];
        key[0] = 0xAB;
        // 0xAB: (0xB) ^ (0xA) = 1
        assert_eq!(bucket(&key), 1);
        // Only the first 9 bytes count.
        key[12] = 0xFF;
        assert_eq!(bucket(&key), 1);
    }

    #[test]
    fn picks_the_newest_file_per_bucket() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "0000000001.idx",
            "0000000003.idx",
            "0f00000002.idx",
            "zz.idx",
            "0100000009.txt",
        ] {
            std::fs::write(tmp.path().join(name), b"").unwrap();
        }
        let best = newest(tmp.path()).unwrap();
        assert!(best[0].as_ref().unwrap().ends_with("0000000003.idx"));
        assert!(best[15].as_ref().unwrap().ends_with("0f00000002.idx"));
        assert!(best[1].is_none());
    }

    proptest::proptest! {
        #[test]
        fn never_panics(at in 0usize..96, byte in proptest::num::u8::ANY) {
            let loc = Location { archive: 1, offset: 2, size: 3 };
            let mut file = build(0, &[([1; 16], loc), ([2; 16], loc)]);
            let at = at % file.len();
            file[at] = byte;
            let _ = parse(&file);
        }

        #[test]
        fn never_panics_on_garbage(data in proptest::collection::vec(proptest::num::u8::ANY, 0..256)) {
            let _ = parse(&data);
        }
    }
}
