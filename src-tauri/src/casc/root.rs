//! The root file: FileDataID to content key, in blocks tagged by locale and
//! content flags. <https://wowdev.wiki/TACT#Root>
//!
//! Three layouts are read: the `MFST` header from 8.2 on (with the 10.1.7
//! header and its v2 block header), and the header-less one before it.
//! Lookups scan the blocks rather than building a map of every file.

use std::collections::{HashMap, HashSet};

use super::bytes::Bytes;
use super::{CascError, CascResult, Key};

const LOCALE_EN_US: u32 = 0x2;
const LOW_VIOLENCE: u32 = 0x80;
const NO_NAME_HASH: u32 = 0x1000_0000;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Layout {
    /// Before 8.2: no header, `(ckey, name hash)` records interleaved.
    Legacy,
    /// 8.2 to 10.1.7: `MFST`, ckeys then name hashes, v1 block headers.
    Mfst,
    /// 10.1.7 on: a sized header and v2 block headers.
    MfstV2,
}

pub struct Root {
    data: Vec<u8>,
    layout: Layout,
    start: usize,
}

impl Root {
    pub fn parse(data: Vec<u8>) -> CascResult<Root> {
        let mut b = Bytes::new(&data);
        let (layout, start) = if data.starts_with(b"TSFM") {
            b.skip(4, "root magic")?;
            let header_size = b.u32_le("root header")?;
            let version = b.u32_le("root header")?;
            match (header_size, version) {
                (20..=64, 1) => (Layout::Mfst, header_size as usize),
                (20..=64, 2) => (Layout::MfstV2, header_size as usize),
                // Before 10.1.7 those two were the file counts.
                _ => (Layout::Mfst, 12),
            }
        } else {
            (Layout::Legacy, 0)
        };
        if start > data.len() {
            return Err(CascError::Bad("root header"));
        }
        let root = Root {
            data,
            layout,
            start,
        };
        // Walk it once so a damaged file fails here, not on a lookup.
        root.scan(&mut |_, _| {})?;
        Ok(root)
    }

    /// Content keys for the files in `wanted` that exist in the enUS (or
    /// all-locale) blocks.
    pub fn ckeys(&self, wanted: &HashSet<u32>) -> HashMap<u32, Key> {
        let mut found = HashMap::new();
        let _ = self.scan(&mut |id, key| {
            if wanted.contains(&id) {
                found.entry(id).or_insert(*key);
            }
        });
        found
    }

    /// Calls `f(fileDataId, ckey)` for each file in a block we'd use.
    fn scan(&self, f: &mut dyn FnMut(u32, &Key)) -> CascResult<()> {
        let mut b = Bytes::new(&self.data[self.start..]);
        while b.remaining() > 0 {
            let count = b.u32_le("root block")? as usize;
            let (content, locale) = match self.layout {
                Layout::MfstV2 => {
                    let locale = b.u32_le("root block")?;
                    let unk1 = b.u32_le("root block")?;
                    let unk2 = b.u32_le("root block")?;
                    let unk3 = b.u8("root block")?;
                    (unk1 | unk2 | (u32::from(unk3) << 17), locale)
                }
                _ => (b.u32_le("root block")?, b.u32_le("root block")?),
            };
            let deltas = b.take(
                count.checked_mul(4).ok_or(CascError::Bad("root block"))?,
                "root block ids",
            )?;
            let named = content & NO_NAME_HASH == 0;
            let (keys, stride) = match self.layout {
                Layout::Legacy => (b.take(count.saturating_mul(24), "root block keys")?, 24),
                _ => {
                    let keys = b.take(count.saturating_mul(16), "root block keys")?;
                    if named {
                        b.skip(count.saturating_mul(8), "root block names")?;
                    }
                    (keys, 16)
                }
            };
            if locale & LOCALE_EN_US == 0 || content & LOW_VIOLENCE != 0 {
                continue;
            }
            let mut id: i64 = -1;
            for (i, &delta) in deltas.as_chunks::<4>().0.iter().enumerate() {
                id += 1 + i64::from(i32::from_le_bytes(delta));
                let Ok(id32) = u32::try_from(id) else {
                    return Err(CascError::Bad("root block ids"));
                };
                let key: &Key = keys[i * stride..i * stride + 16]
                    .try_into()
                    .map_err(|_| CascError::Bad("root block keys"))?;
                f(id32, key);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `(locale, content flags, [(fileDataId, ckey)])`.
    pub type Block<'a> = (u32, u32, &'a [(u32, Key)]);

    /// A 10.1.7+ root with one v2 block per `Block`.
    pub fn build(blocks: &[Block]) -> Vec<u8> {
        let mut out = b"TSFM".to_vec();
        out.extend(24u32.to_le_bytes());
        out.extend(2u32.to_le_bytes());
        let total: usize = blocks.iter().map(|b| b.2.len()).sum();
        out.extend((total as u32).to_le_bytes());
        out.extend((total as u32).to_le_bytes());
        out.extend(0u32.to_le_bytes());
        for &(locale, content, files) in blocks {
            out.extend((files.len() as u32).to_le_bytes());
            out.extend(locale.to_le_bytes());
            out.extend(content.to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.push(0);
            let mut prev: i64 = -1;
            for (id, _) in files {
                out.extend(((i64::from(*id) - prev - 1) as i32).to_le_bytes());
                prev = i64::from(*id);
            }
            for (_, key) in files {
                out.extend(key);
            }
            if content & NO_NAME_HASH == 0 {
                out.extend(vec![0xAB; files.len() * 8]);
            }
        }
        out
    }

    fn key(n: u8) -> Key {
        [n; 16]
    }

    fn wanted(ids: &[u32]) -> HashSet<u32> {
        ids.iter().copied().collect()
    }

    #[test]
    fn finds_files_in_en_us_blocks_only() {
        let file = build(&[
            (0x20, 0, &[(10, key(1))]), // deDE only
            (0xFFFF_FFFF, LOW_VIOLENCE, &[(10, key(2))]),
            (0xFFFF_FFFF, NO_NAME_HASH, &[(10, key(3)), (12, key(4))]),
            (LOCALE_EN_US, 0, &[(136_235, key(5))]),
        ]);
        let root = Root::parse(file).unwrap();
        let found = root.ckeys(&wanted(&[10, 12, 136_235, 99]));
        assert_eq!(found.get(&10), Some(&key(3)));
        assert_eq!(found.get(&12), Some(&key(4)));
        assert_eq!(found.get(&136_235), Some(&key(5)));
        assert_eq!(found.get(&99), None);
    }

    #[test]
    fn reads_the_pre_10_1_7_header_and_the_legacy_layout() {
        // 8.2 MFST: magic, two counts, v1 blocks.
        let mut mfst = b"TSFM".to_vec();
        mfst.extend(1u32.to_le_bytes());
        mfst.extend(1u32.to_le_bytes());
        mfst.extend(1u32.to_le_bytes()); // count
        mfst.extend(NO_NAME_HASH.to_le_bytes());
        mfst.extend(LOCALE_EN_US.to_le_bytes());
        mfst.extend(41i32.to_le_bytes());
        mfst.extend(key(7));
        let found = Root::parse(mfst).unwrap().ckeys(&wanted(&[41]));
        assert_eq!(found.get(&41), Some(&key(7)));

        // Legacy: v1 blocks, (ckey, name hash) interleaved.
        let mut legacy = Vec::new();
        legacy.extend(2u32.to_le_bytes());
        legacy.extend(0u32.to_le_bytes());
        legacy.extend(LOCALE_EN_US.to_le_bytes());
        legacy.extend(5i32.to_le_bytes());
        legacy.extend(0i32.to_le_bytes());
        for n in [8, 9] {
            legacy.extend(key(n));
            legacy.extend([0; 8]);
        }
        let found = Root::parse(legacy).unwrap().ckeys(&wanted(&[5, 6]));
        assert_eq!(found.get(&5), Some(&key(8)));
        assert_eq!(found.get(&6), Some(&key(9)));
    }

    #[test]
    fn a_block_longer_than_the_file_is_refused() {
        let mut file = build(&[(LOCALE_EN_US, 0, &[(1, key(1))])]);
        file[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Root::parse(file).is_err());
    }

    proptest::proptest! {
        #[test]
        fn never_panics(at in 0usize..128, byte in proptest::num::u8::ANY) {
            let mut file = build(&[(LOCALE_EN_US, 0, &[(1, key(1)), (5, key(2))])]);
            let at = at % file.len();
            file[at] = byte;
            if let Ok(root) = Root::parse(file) {
                let _ = root.ckeys(&wanted(&[1, 5]));
            }
        }

        #[test]
        fn never_panics_on_garbage(data in proptest::collection::vec(proptest::num::u8::ANY, 0..256)) {
            let _ = Root::parse(data);
        }
    }
}
