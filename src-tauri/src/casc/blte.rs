//! BLTE, the container every CASC file is stored in: a chunk table, then
//! chunks that are each raw (`N`), zlib (`Z`) or encrypted (`E`).
//! <https://wowdev.wiki/BLTE>
//!
//! Encrypted chunks are Salsa20 under the public TACT keys (see `tact.rs`);
//! without a key table, or for a name it doesn't have, they fail with
//! `Encrypted` rather than coming back as garbage. Every size is checked
//! against `cap` before anything is allocated or inflated.
//!
//! The decryptor exists only behind the casc-dev feature. The shipped
//! reader is unencrypted-only: its `E` branch fails with `Encrypted`
//! before a key could even be asked for.

use std::io::Read;

use super::bytes::Bytes;
#[cfg(feature = "casc-dev")]
use super::tact::TactKeys;
use super::{CascError, CascResult};

/// The key table type the `E` chunks need. Only casc-dev has a real one
/// (`tact.rs`); otherwise this placeholder has no values, so `None` is the
/// only thing a caller can pass.
#[cfg(not(feature = "casc-dev"))]
pub(crate) enum TactKeys {}

/// Chunks in one file. Real files have at most a few thousand.
const MAX_CHUNKS: usize = 1 << 16;

pub fn decode_with(data: &[u8], cap: usize, keys: Option<&TactKeys>) -> CascResult<Vec<u8>> {
    let mut b = Bytes::new(data);
    if b.take(4, "BLTE magic")? != b"BLTE" {
        return Err(CascError::Bad("BLTE magic"));
    }
    let header_size = b.u32_be("BLTE header size")? as usize;
    if header_size == 0 {
        // One chunk, no table: the rest of the file.
        let mut out = Vec::new();
        chunk(b.take(b.remaining(), "BLTE chunk")?, None, cap, keys, 0, &mut out)?;
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
    for (frame, &(comp, decomp)) in table.iter().enumerate() {
        chunk(b.take(comp, "BLTE chunk")?, Some(decomp), cap, keys, frame as u32, &mut out)?;
    }
    Ok(out)
}

/// Appends one chunk's contents to `out`. `size` is what the table says it
/// decodes to (absent for a table-less file); `frame` is the chunk's
/// ordinal, which salts the encrypted chunks' nonce.
fn chunk(
    data: &[u8],
    size: Option<usize>,
    cap: usize,
    keys: Option<&TactKeys>,
    frame: u32,
    out: &mut Vec<u8>,
) -> CascResult<()> {
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
        b'Z' => inflate(body, room, out)?,
        b'E' => {
            // The casc-dev build decrypts; the shipped one stops here.
            #[cfg(feature = "casc-dev")]
            {
                let plain = decrypt(body, keys, frame)?;
                inflate(&plain, room, out)?;
            }
            #[cfg(not(feature = "casc-dev"))]
            {
                let _ = (keys, frame);
                return Err(CascError::Encrypted);
            }
        }
        _ => return Err(CascError::Bad("BLTE chunk mode")),
    }
    match size {
        Some(size) if out.len() - start != size => Err(CascError::Bad("BLTE chunk size")),
        _ => Ok(()),
    }
}

/// One byte over the room, so "too big" is told apart from "fits".
fn inflate(body: &[u8], room: usize, out: &mut Vec<u8>) -> CascResult<()> {
    let start = out.len();
    let limit = room.saturating_add(1) as u64;
    flate2::read::ZlibDecoder::new(body)
        .take(limit)
        .read_to_end(out)
        .map_err(|_| CascError::Bad("BLTE zlib"))?;
    if out.len() - start > room {
        return Err(CascError::TooBig);
    }
    Ok(())
}

/// Decrypts an encrypted chunk: key name, IV and cipher type in a header
/// (CascLib's `CascDecrypt`), then the chunk's bytes under Salsa20. The
/// frame index salts the IV, so every chunk of a file uses its own nonce.
#[cfg(feature = "casc-dev")]
fn decrypt(body: &[u8], keys: Option<&TactKeys>, frame: u32) -> CascResult<Vec<u8>> {
    let keys = keys.ok_or(CascError::Encrypted)?;
    let mut b = Bytes::new(body);
    let name_size = b.u8("BLTE key name size")?;
    if name_size != 8 {
        return Err(CascError::Bad("BLTE key name size"));
    }
    let name: [u8; 8] = b.array("BLTE key name")?;
    let iv_size = b.u8("BLTE iv size")?;
    if iv_size != 4 && iv_size != 8 {
        return Err(CascError::Bad("BLTE iv size"));
    }
    let iv_size = usize::from(iv_size);
    let mut nonce = [0u8; 8];
    nonce[..iv_size].copy_from_slice(b.take(iv_size, "BLTE iv")?);
    if b.u8("BLTE encryption type")? != b'S' {
        return Err(CascError::Bad("BLTE encryption type"));
    }
    let key = keys.get(&name).ok_or(CascError::Encrypted)?;
    for (i, byte) in nonce.iter_mut().take(4).enumerate() {
        *byte ^= u8::try_from(frame >> (8 * i)).unwrap_or(0);
    }
    let mut plain = b.take(b.remaining(), "BLTE ciphertext")?.to_vec();
    super::salsa::apply(key, &nonce, &mut plain);
    Ok(plain)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// The no-keys read, what every non-extracting caller gets.
    fn decode(data: &[u8], cap: usize) -> CascResult<Vec<u8>> {
        decode_with(data, cap, None)
    }

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

    /// A BLTE whose every chunk is encrypted under `key`, each chunk's nonce
    /// salted with its own frame index, the way Blizzard writes them.
    #[cfg(feature = "casc-dev")]
    fn encode_encrypted(name: &[u8; 8], key: &[u8; 16], plains: &[&[u8]]) -> Vec<u8> {
        let bodies: Vec<Vec<u8>> = plains
            .iter()
            .enumerate()
            .map(|(frame, plain)| {
                let mut z = flate2::write::ZlibEncoder::new(
                    Vec::new(),
                    flate2::Compression::default(),
                );
                z.write_all(plain).unwrap();
                let mut body = z.finish().unwrap();
                // The header carries the plain IV; the cipher's nonce is the
                // IV with the frame index XORed in, as in `decrypt`.
                let iv = [0u8; 8];
                let mut nonce = iv;
                for (i, byte) in nonce.iter_mut().take(4).enumerate() {
                    *byte ^= (frame >> (8 * i)) as u8;
                }
                super::super::salsa::apply(key, &nonce, &mut body);
                let mut chunk = vec![b'E', 8];
                chunk.extend(name);
                chunk.push(8);
                chunk.extend(iv);
                chunk.push(b'S');
                chunk.extend(body);
                chunk
            })
            .collect();
        let mut out = b"BLTE".to_vec();
        out.extend(((12 + 24 * plains.len()) as u32).to_be_bytes());
        out.push(0x0F);
        out.extend(&(plains.len() as u32).to_be_bytes()[1..]);
        for (body, plain) in bodies.iter().zip(plains) {
            out.extend((body.len() as u32).to_be_bytes());
            out.extend((plain.len() as u32).to_be_bytes());
            out.extend([0; 16]);
        }
        for body in bodies {
            out.extend(body);
        }
        out
    }

    #[cfg(feature = "casc-dev")]
    const KEY_NAME: &[u8; 8] = b"01234567";
    #[cfg(feature = "casc-dev")]
    const KEY: &[u8; 16] = b"abcdefghijklmnop";

    #[cfg(feature = "casc-dev")]
    fn keys_for(name: &[u8; 8], key: &[u8; 16]) -> TactKeys {
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
        super::super::tact::parse(&format!("{} {}", hex(name), hex(key)))
    }

    #[cfg(feature = "casc-dev")]
    #[test]
    fn encrypted_chunks_decrypt_frame_by_frame() {
        let file = encode_encrypted(KEY_NAME, KEY, &[b"frame zero ", b"frame one"]);
        // The nonce is per chunk, so both need their own frame index.
        let keys = keys_for(KEY_NAME, KEY);
        let plain = decode_with(&file, 1024, Some(&keys)).unwrap();
        assert_eq!(plain, b"frame zero frame one");
    }

    #[cfg(feature = "casc-dev")]
    #[test]
    fn a_name_the_keys_lack_stays_encrypted() {
        let keys = keys_for(b"00000000", KEY);
        let file = encode_encrypted(KEY_NAME, KEY, &[b"hidden"]);
        assert!(matches!(
            decode_with(&file, 1024, Some(&keys)),
            Err(CascError::Encrypted)
        ));
        // The same table, unknown key included, is fine with no E chunk.
        let plain = encode(&[(b'N', b"ok")]);
        assert_eq!(decode_with(&plain, 1024, Some(&keys)).unwrap(), b"ok");
    }

    #[cfg(feature = "casc-dev")]
    #[test]
    fn a_malformed_encrypted_header_is_bad_not_a_panic() {
        let keys = keys_for(KEY_NAME, KEY);
        // name size, iv size and cipher type are each checked in turn.
        for body in [
            [0x03u8].as_slice(),               // no name
            [8u8, 1, 2, 3, 4, 5, 6, 7, 8].as_slice(), // iv size not 4 or 8
            [8u8, 1, 2, 3, 4, 5, 6, 7, 8, 4, 1, 2, 3, 4, b'X'].as_slice(), // not Salsa20
        ] {
            let file = encode(&[(b'E', body)]);
            assert!(matches!(
                decode_with(&file, 1024, Some(&keys)),
                Err(CascError::Bad(_))
            ));
        }
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
