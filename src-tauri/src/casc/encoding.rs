//! The encoding file: content key (what root names) to encoding key (what
//! the local index and archives are keyed by).
//! <https://wowdev.wiki/TACT#Encoding_table>
//!
//! Kept as the decoded bytes plus the page index; a lookup binary-searches
//! the index and scans one page, so the millions of entries are never
//! expanded into a map.

use super::bytes::Bytes;
use super::{CascError, CascResult, Key};

const HEADER: usize = 22;
/// Pages are 4 KB in every known build; anything far bigger is not ours.
const MAX_PAGE: usize = 1 << 20;

pub struct Encoding {
    data: Vec<u8>,
    /// First content key of each page, with where the page starts.
    pages: Vec<(Key, usize)>,
    page_size: usize,
}

impl Encoding {
    pub fn parse(data: Vec<u8>) -> CascResult<Encoding> {
        let mut b = Bytes::new(&data);
        if b.take(2, "encoding magic")? != b"EN" {
            return Err(CascError::Bad("encoding magic"));
        }
        let version = b.u8("encoding version")?;
        let ckey_size = b.u8("encoding key size")?;
        let ekey_size = b.u8("encoding key size")?;
        if version != 1 || ckey_size != 16 || ekey_size != 16 {
            return Err(CascError::Bad("encoding version"));
        }
        let page_size = usize::from(b.u16_be("encoding page size")?) * 1024;
        b.skip(2, "encoding page size")?;
        let page_count = b.u32_be("encoding page count")? as usize;
        b.skip(4, "encoding page count")?;
        b.skip(1, "encoding flags")?;
        let espec_size = b.u32_be("encoding espec size")? as usize;
        debug_assert_eq!(b.pos(), HEADER);
        if page_size == 0 || page_size > MAX_PAGE {
            return Err(CascError::Bad("encoding page size"));
        }
        b.skip(espec_size, "encoding espec")?;

        // The index (first key + md5 per page) and the pages must both fit.
        let index_len = page_count
            .checked_mul(32)
            .ok_or(CascError::Bad("encoding page count"))?;
        let pages_len = page_count
            .checked_mul(page_size)
            .ok_or(CascError::Bad("encoding page count"))?;
        if index_len.saturating_add(pages_len) > b.remaining() {
            return Err(CascError::Bad("encoding page count"));
        }
        let first_page = b.pos() + index_len;
        let mut pages = Vec::with_capacity(page_count);
        for i in 0..page_count {
            let first: Key = b.array("encoding page index")?;
            b.skip(16, "encoding page index")?;
            pages.push((first, first_page + i * page_size));
        }
        if pages.windows(2).any(|w| w[0].0 > w[1].0) {
            return Err(CascError::Bad("encoding page order"));
        }
        Ok(Encoding {
            data,
            pages,
            page_size,
        })
    }

    /// The first encoding key for a content key.
    pub fn ekey(&self, ckey: &Key) -> Option<Key> {
        // The last page whose first key is <= ckey.
        let i = self.pages.partition_point(|(first, _)| first <= ckey);
        let (_, start) = *self.pages.get(i.checked_sub(1)?)?;
        let mut b = Bytes::new(&self.data[start..start + self.page_size]);
        loop {
            let count = b.u8("").ok()? as usize;
            if count == 0 {
                return None; // the page's zero padding
            }
            b.skip(5, "").ok()?; // file size
            let key: Key = b.array("").ok()?;
            let ekeys = b.take(count * 16, "").ok()?;
            if &key == ckey {
                return ekeys[..16].try_into().ok();
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An encoding file with one 4 KB page per `page` of `(ckey, ekey)` pairs
    /// (each page's pairs sorted by ckey).
    pub fn build(pages: &[&[(Key, Key)]]) -> Vec<u8> {
        let espec = b"z\0";
        let mut out = b"EN".to_vec();
        out.extend([1, 16, 16]);
        out.extend(4u16.to_be_bytes());
        out.extend(4u16.to_be_bytes());
        out.extend((pages.len() as u32).to_be_bytes());
        out.extend(0u32.to_be_bytes());
        out.push(0);
        out.extend((espec.len() as u32).to_be_bytes());
        out.extend(espec);
        for page in pages {
            out.extend(page.first().map_or([0; 16], |p| p.0));
            out.extend([0; 16]);
        }
        for page in pages {
            let mut bytes = Vec::new();
            for (ckey, ekey) in page.iter() {
                bytes.push(1);
                bytes.extend([0, 0, 0, 0, 9]);
                bytes.extend(ckey);
                bytes.extend(ekey);
            }
            bytes.resize(4096, 0);
            out.extend(bytes);
        }
        out
    }

    fn key(n: u8) -> Key {
        [n; 16]
    }

    #[test]
    fn finds_keys_across_pages_and_misses_cleanly() {
        let file = build(&[
            &[(key(1), key(101)), (key(3), key(103))],
            &[(key(5), key(105)), (key(7), key(107))],
        ]);
        let enc = Encoding::parse(file).unwrap();
        assert_eq!(enc.ekey(&key(1)), Some(key(101)));
        assert_eq!(enc.ekey(&key(3)), Some(key(103)));
        assert_eq!(enc.ekey(&key(7)), Some(key(107)));
        assert_eq!(enc.ekey(&key(0)), None);
        assert_eq!(enc.ekey(&key(4)), None);
        assert_eq!(enc.ekey(&key(9)), None);
    }

    #[test]
    fn a_page_count_past_the_end_is_refused() {
        let mut file = build(&[&[(key(1), key(2))]]);
        file[9..13].copy_from_slice(&1000u32.to_be_bytes());
        assert!(Encoding::parse(file).is_err());
    }

    proptest::proptest! {
        #[test]
        fn never_panics(at in 0usize..4200, byte in proptest::num::u8::ANY, probe in proptest::num::u8::ANY) {
            let mut file = build(&[&[(key(1), key(2)), (key(3), key(4))]]);
            let at = at % file.len();
            file[at] = byte;
            if let Ok(enc) = Encoding::parse(file) {
                let _ = enc.ekey(&key(probe));
            }
        }
    }
}
