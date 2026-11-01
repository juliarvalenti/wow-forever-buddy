//! The public TACT keys, one per key name a build's encrypted files may ask
//! for: wowdev's community list (`TACTKeys/WoW.txt`), fetched like the
//! listfile and never committed. A name we don't have stays `Encrypted`, so
//! one unknown file never blocks a run.

use std::collections::HashMap;
use std::path::Path;

use super::CascResult;

/// Key name (8 bytes, as in the BLTE header) to Salsa20 key (16 bytes).
#[derive(Default)]
pub struct TactKeys {
    keys: HashMap<[u8; 8], [u8; 16]>,
}

impl TactKeys {
    pub fn get(&self, name: &[u8; 8]) -> Option<&[u8; 16]> {
        self.keys.get(name)
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Parses `HEXNAME HEXKEY` lines, one key per line; anything else (a comment,
/// a blank line, a malformed row) is skipped, never fatal.
pub fn parse(text: &str) -> TactKeys {
    let mut keys = TactKeys::default();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(name), Some(key), None) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        if let (Ok(name), Ok(key)) = (hex_bytes::<8>(name), hex_bytes::<16>(key)) {
            keys.keys.insert(name, key);
        }
    }
    keys
}

pub fn load(path: &Path) -> CascResult<TactKeys> {
    let text =
        std::fs::read_to_string(path).map_err(|e| super::CascError::Missing(e.to_string()))?;
    Ok(parse(&text))
}

fn hex_bytes<const N: usize>(text: &str) -> Result<[u8; N], ()> {
    if text.len() != N * 2 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(());
    }
    let mut out = [0; N];
    for (byte, pair) in out.iter_mut().zip(text.as_bytes().chunks(2)) {
        *byte = hex(pair[0]) << 4 | hex(pair[1]);
    }
    Ok(out)
}

fn hex(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rows_and_skips_the_rest() {
        let keys = parse(
            "# comment\n\
             0001C772CCC21F2D BDC51862ABED79B2DE48C8E7E66C6200\n\
             \n\
             short BDC51862ABED79B2DE48C8E7E66C6200\n\
             0001C772CCC21F2D not-hex\n\
             E07E107F1390A3DF 290D27B0E871F8C5B14A14E514D0F0D9\n",
        );
        assert_eq!(keys.len(), 2);
        assert_eq!(
            keys.get(&[0x00, 0x01, 0xC7, 0x72, 0xCC, 0xC2, 0x1F, 0x2D]),
            Some(&[
                0xBD, 0xC5, 0x18, 0x62, 0xAB, 0xED, 0x79, 0xB2, 0xDE, 0x48, 0xC8, 0xE7, 0xE6, 0x6C,
                0x62, 0x00
            ])
        );
        let missing = [0xEE; 8];
        assert!(keys.get(&missing).is_none());
    }
}
