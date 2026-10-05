//! BLTE, the container every CASC file is stored in: a chunk table, then
//! chunks that are each raw (`N`), zlib (`Z`) or encrypted (`E`).
//! <https://wowdev.wiki/BLTE>
//!
//! Encrypted chunks need keys we don't have; they fail with `Encrypted`
//! rather than coming back as garbage. Every size is checked against `cap`
//! before anything is allocated or inflated.

use std::io::Read;

use super::bytes::Bytes;
use super::{CascError, CascResult};

/// Chunks in one file. Real files have at most a few thousand.
const MAX_CHUNKS: usize = 1 << 16;

pub fn decode(data: &[u8], cap: usize) -> CascResult<Vec<u8>> {
    let mut b = Bytes::new(data);
    if b.take(4, "BLTE magic")? != b"BLTE" {
        return Err(CascError::Bad("BLTE magic"));
    }
    let header_size = b.u32_be("BLTE header size")? as usize;
    if header_size == 0 {
        // One chunk, no table: the rest of the file.
        let mut out = Vec::new();
        chunk(b.take(b.remaining(), "BLTE chunk")?, None, cap, &mut out)?;
        return Ok(out);
    }
    let flags = b.u8("BLTE flags")?;
    let count = b.uint_be(3, "BLTE chunk count")? as usize;
    if flags != 0x0F || count == 0 || count > MAX_CHUNKS || header_size != 12 + 24 * count {
        return Err(CascError::Bad("BLTE chunk table"));
    }
    let mut table = Vec::with_capacity(count);
    let mut total = 0usize;
    for _ in 0..count {
        let comp = b.u32_be("BLTE chunk size")? as usize;
        let decomp = b.u32_be("BLTE chunk size")? as usize;
        b.skip(16, "BLTE chunk hash")?;
        total = total.saturating_add(decomp);
        table.push((comp, decomp));
    }
    if total > cap {
        return Err(CascError::TooBig);
    }
    let mut out = Vec::with_capacity(total);
    for (comp, decomp) in table {
        chunk(b.take(comp, "BLTE chunk")?, Some(decomp), cap, &mut out)?;
    }
    Ok(out)
}

/// Appends one chunk's contents to `out`. `size` is what the table says it
/// decodes to (absent for a table-less file).
fn chunk(data: &[u8], size: Option<usize>, cap: usize, out: &mut Vec<u8>) -> CascResult<()> {
    let Some((&mode, body)) = data.split_first() else {
        return Err(CascError::Bad("BLTE empty chunk"));
    };
    let start = out.len();
    let room = cap.saturating_sub(start);
    match mode {
        b'N' => {
            if body.len() > room {
                return Err(CascError::TooBig);
            }
            out.extend_from_slice(body);
        }
        b'Z' => {
            // One byte over the room, so "too big" is told apart from "fits".
            let limit = room.saturating_add(1) as u64;
            flate2::read::ZlibDecoder::new(body)
                .take(limit)
                .read_to_end(out)
                .map_err(|_| CascError::Bad("BLTE zlib"))?;
            if out.len() - start > room {
                return Err(CascError::TooBig);
            }
        }
        b'E' => return Err(CascError::Encrypted),
        _ => return Err(CascError::Bad("BLTE chunk mode")),
    }
    match size {
        Some(size) if out.len() - start != size => Err(CascError::Bad("BLTE chunk size")),
        _ => Ok(()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// A BLTE file from `(mode, contents)` chunks, the way Blizzard's tools
    /// write them. Used by the other modules' tests too.
    pub fn encode(chunks: &[(u8, &[u8])]) -> Vec<u8> {
        let bodies: Vec<Vec<u8>> = chunks
            .iter()
            .map(|&(mode, data)| {
                let mut body = vec![mode];
                match mode {
                    b'Z' => {
                        let mut z = flate2::write::ZlibEncoder::new(
                            Vec::new(),
                            flate2::Compression::default(),
                        );
                        z.write_all(data).unwrap();
                        body.extend(z.finish().unwrap());
                    }
                    _ => body.extend_from_slice(data),
                }
                body
            })
            .collect();
        let mut out = b"BLTE".to_vec();
        out.extend(((12 + 24 * chunks.len()) as u32).to_be_bytes());
        out.push(0x0F);
        out.extend(&(chunks.len() as u32).to_be_bytes()[1..]);
        for (body, &(_, data)) in bodies.iter().zip(chunks) {
            out.extend((body.len() as u32).to_be_bytes());
            out.extend((data.len() as u32).to_be_bytes());
            out.extend([0; 16]);
        }
        for body in bodies {
            out.extend(body);
        }
        out
    }

    #[test]
    fn raw_and_zlib_chunks_join_in_order() {
        let file = encode(&[(b'N', b"hello "), (b'Z', b"world")]);
        assert_eq!(decode(&file, 1024).unwrap(), b"hello world");
    }

    #[test]
    fn a_table_less_file_is_one_chunk() {
        let mut file = b"BLTE\0\0\0\0N".to_vec();
        file.extend(b"abc");
        assert_eq!(decode(&file, 1024).unwrap(), b"abc");
    }

    #[test]
    fn encrypted_chunks_fail_soft() {
        let file = encode(&[(b'N', b"ok"), (b'E', b"secret")]);
        assert!(matches!(decode(&file, 1024), Err(CascError::Encrypted)));
    }

    #[test]
    fn the_cap_holds_against_the_table_and_against_zlib() {
        let file = encode(&[(b'N', &[0; 100])]);
        assert!(matches!(decode(&file, 99), Err(CascError::TooBig)));

        // A zlib bomb in a table-less file, where only the cap stops it.
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(&vec![0; 1 << 20]).unwrap();
        let mut bomb = b"BLTE\0\0\0\0Z".to_vec();
        bomb.extend(z.finish().unwrap());
        assert!(matches!(decode(&bomb, 4096), Err(CascError::TooBig)));
    }

    #[test]
    fn a_chunk_that_lies_about_its_size_is_refused() {
        let mut file = encode(&[(b'N', b"abcd")]);
        // decompressed size in the table: 4 -> 5
        file[12 + 7] = 5;
        assert!(matches!(
            decode(&file, 1024),
            Err(CascError::Bad("BLTE chunk size"))
        ));
    }

    proptest::proptest! {
        #[test]
        fn never_panics_on_garbage(data in proptest::collection::vec(proptest::num::u8::ANY, 0..512)) {
            let _ = decode(&data, 4096);
        }

        #[test]
        fn never_panics_on_a_damaged_file(at in 0usize..64, byte in proptest::num::u8::ANY) {
            let mut file = encode(&[(b'N', b"hello "), (b'Z', b"world")]);
            let at = at % file.len();
            file[at] = byte;
            let _ = decode(&file, 4096);
        }
    }
}
