//! Salsa20, the stream cipher CASC's encrypted chunks use. One small use, so
//! it lives here instead of a crate: 20 rounds, the "expand 16-byte k"
//! constants, a 16-byte key and an 8-byte nonce per chunk.

/// The four constant words of "expand 16-byte k", little-endian.
const TAU: [u32; 4] = [0x6170_7865, 0x3120_646e, 0x7962_2d36, 0x6b20_6574];

/// The Salsa20 state for one 64-byte block.
fn state(key: &[u8; 16], nonce: &[u8; 8], block: u64) -> [u32; 16] {
    let k = |i: usize| u32::from_le_bytes(key[4 * i..4 * i + 4].try_into().unwrap());
    let n = |i: usize| u32::from_le_bytes(nonce[4 * i..4 * i + 4].try_into().unwrap());
    [
        TAU[0],
        k(0),
        k(1),
        k(2),
        k(3),
        TAU[1],
        n(0),
        n(1),
        block as u32,
        (block >> 32) as u32,
        TAU[2],
        k(0),
        k(1),
        k(2),
        k(3),
        TAU[3],
    ]
}

/// A column round then a row round; ten of each pair make the 20 rounds.
fn rounds(x: &mut [u32; 16]) {
    for _ in 0..10 {
        quarter(x, 0, 4, 8, 12);
        quarter(x, 5, 9, 13, 1);
        quarter(x, 10, 14, 2, 6);
        quarter(x, 15, 3, 7, 11);
        quarter(x, 0, 1, 2, 3);
        quarter(x, 5, 6, 7, 4);
        quarter(x, 10, 11, 8, 9);
        quarter(x, 15, 12, 13, 14);
    }
}

fn quarter(x: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    x[b] ^= x[a].wrapping_add(x[d]).rotate_left(7);
    x[c] ^= x[b].wrapping_add(x[a]).rotate_left(9);
    x[d] ^= x[c].wrapping_add(x[b]).rotate_left(13);
    x[a] ^= x[d].wrapping_add(x[c]).rotate_left(18);
}

/// One keystream block: the state after the rounds, added to the input state.
fn block(key: &[u8; 16], nonce: &[u8; 8], counter: u64, out: &mut [u8; 64]) {
    let mut x = state(key, nonce, counter);
    let start = x;
    rounds(&mut x);
    for (word, i) in x.iter().zip(0..) {
        let sum = word.wrapping_add(start[i]);
        out[4 * i..4 * i + 4].copy_from_slice(&sum.to_le_bytes());
    }
}

/// XORs the keystream into `data` in place, block by block.
pub fn apply(key: &[u8; 16], nonce: &[u8; 8], data: &mut [u8]) {
    let mut stream = [0u8; 64];
    for (counter, bytes) in data.chunks_mut(64).enumerate() {
        block(key, nonce, counter as u64, &mut stream);
        for (byte, k) in bytes.iter_mut().zip(stream) {
            *byte ^= k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Salsa20 with a 16-byte key, from the eSTREAM verified vectors
    /// (set 1, vector 0): key 0x80 then zeros, a zero nonce.
    #[test]
    fn matches_the_estream_128_bit_vectors() {
        let key = {
            let mut k = [0u8; 16];
            k[0] = 0x80;
            k
        };
        let nonce = [0u8; 8];
        let mut data = [0u8; 64];
        apply(&key, &nonce, &mut data);
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
        assert_eq!(
            hex(&data),
            "4DFA5E481DA23EA09A31022050859936DA52FCEE218005164F267CB65F5CFD7F\
             2B4F97E0FF16924A52DF269515110A07F9E460BC65EF95DA58F740B7D1DBB0AA"
                .replace(' ', "")
        );
    }

    #[test]
    fn xor_is_its_own_inverse() {
        let key = [7u8; 16];
        let nonce = [9u8; 8];
        let mut data = *b"the quick brown fox jumps over the lazy dog";
        let plain = data;
        apply(&key, &nonce, &mut data);
        assert_ne!(data, plain);
        apply(&key, &nonce, &mut data);
        assert_eq!(data, plain);
    }
}
