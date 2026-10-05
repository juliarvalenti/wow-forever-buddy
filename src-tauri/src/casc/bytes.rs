//! A bounds-checked reader over untrusted bytes. Every read either fits or
//! returns `Bad`, so a parser built on it can't index past the end or panic.

use super::{CascError, CascResult};

pub struct Bytes<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Bytes<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Bytes { buf, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn take(&mut self, n: usize, what: &'static str) -> CascResult<&'a [u8]> {
        if n > self.remaining() {
            return Err(CascError::Bad(what));
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    pub fn skip(&mut self, n: usize, what: &'static str) -> CascResult<()> {
        self.take(n, what).map(|_| ())
    }

    pub fn array<const N: usize>(&mut self, what: &'static str) -> CascResult<[u8; N]> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N, what)?);
        Ok(out)
    }

    pub fn u8(&mut self, what: &'static str) -> CascResult<u8> {
        Ok(self.array::<1>(what)?[0])
    }

    pub fn u16_be(&mut self, what: &'static str) -> CascResult<u16> {
        Ok(u16::from_be_bytes(self.array(what)?))
    }

    pub fn u16_le(&mut self, what: &'static str) -> CascResult<u16> {
        Ok(u16::from_le_bytes(self.array(what)?))
    }

    pub fn u32_be(&mut self, what: &'static str) -> CascResult<u32> {
        Ok(u32::from_be_bytes(self.array(what)?))
    }

    pub fn u32_le(&mut self, what: &'static str) -> CascResult<u32> {
        Ok(u32::from_le_bytes(self.array(what)?))
    }

    /// An unsigned big-endian integer of `n` (at most 8) bytes.
    pub fn uint_be(&mut self, n: usize, what: &'static str) -> CascResult<u64> {
        debug_assert!(n <= 8);
        Ok(self
            .take(n, what)?
            .iter()
            .fold(0u64, |acc, &b| (acc << 8) | u64::from(b)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_in_order_and_refuses_past_the_end() {
        let mut b = Bytes::new(&[1, 0, 2, 0, 0, 0, 3, 4, 5]);
        assert_eq!(b.u8("a").unwrap(), 1);
        assert_eq!(b.u16_be("b").unwrap(), 2);
        assert_eq!(b.u32_le("c").unwrap(), 0x0300_0000);
        assert_eq!(b.uint_be(2, "d").unwrap(), 0x0405);
        assert_eq!(b.remaining(), 0);
        assert!(matches!(b.u8("e"), Err(CascError::Bad("e"))));
        // A failed read doesn't move.
        assert_eq!(b.pos(), 9);
    }

    #[test]
    fn huge_lengths_are_refused_not_overflowed() {
        let mut b = Bytes::new(&[0; 4]);
        b.skip(1, "a").unwrap();
        assert!(b.take(usize::MAX, "b").is_err());
    }
}
