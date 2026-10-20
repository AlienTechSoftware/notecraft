//! Bounds-checked little-endian reading. Every read returns an error instead of panicking.

use crate::error::{OneError, Result};

/// A cursor over a byte slice.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    what: &'static str,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], what: &'static str) -> Self {
        Reader { data, pos: 0, what }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(OneError::Truncated(self.what))?;
        let out = self.data.get(self.pos..end).ok_or(OneError::Truncated(self.what))?;
        self.pos = end;
        Ok(out)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?.first().copied().unwrap_or(0))
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(le(self.take(2)?) as u16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(le(self.take(4)?) as u32)
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(le(self.take(8)?))
    }

    /// An unsigned value of `n` bytes (1, 2, 4 or 8).
    pub fn uint(&mut self, n: usize) -> Result<u64> {
        Ok(le(self.take(n)?))
    }

    pub fn guid(&mut self) -> Result<[u8; 16]> {
        let mut g = [0u8; 16];
        g.copy_from_slice(self.take(16)?);
        Ok(g)
    }
}

/// Little-endian value of up to 8 bytes (extra bytes are ignored).
pub fn le(b: &[u8]) -> u64 {
    b.iter().take(8).enumerate().fold(0u64, |acc, (i, &x)| acc | (u64::from(x) << (8 * i)))
}

/// A slice of `data`, or an error if it lies outside.
pub fn slice<'a>(data: &'a [u8], start: u64, len: u64, what: &'static str) -> Result<&'a [u8]> {
    let start = usize::try_from(start).map_err(|_| OneError::Truncated(what))?;
    let len = usize::try_from(len).map_err(|_| OneError::Truncated(what))?;
    let end = start.checked_add(len).ok_or(OneError::Truncated(what))?;
    data.get(start..end).ok_or(OneError::Truncated(what))
}

/// UTF-16LE text up to the first NUL (or the end).
pub fn utf16(b: &[u8]) -> String {
    let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(units.get(..end).unwrap_or(&[]))
}

/// Text kept one byte per character ([MS-ONE] TextExtendedAscii). The bytes above 0x7F are
/// Windows-1252, the code page the writing application used; the five it leaves undefined
/// come through as the control character of the same number.
pub fn extended_ascii(b: &[u8]) -> String {
    const CP1252: [char; 32] = [
        '\u{20AC}', '\u{81}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}',
        '\u{0152}', '\u{8D}', '\u{017D}', '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
        '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{9D}', '\u{017E}', '\u{0178}',
    ];
    let end = b.iter().position(|&u| u == 0).unwrap_or(b.len());
    b[..end].iter().map(|&u| if (0x80..0xA0).contains(&u) { CP1252[usize::from(u - 0x80)] } else { char::from(u) }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_byte_text_is_not_read_as_two_byte_text() {
        // Read as UTF-16 this is the wrong characters entirely, which is how it looked in a real page.
        assert_eq!(extended_ascii(b"Book fizz"), "Book fizz");
        assert_eq!(extended_ascii(&[b'a', 0x93, b'b', 0x94, 0xE9]), "a\u{201C}b\u{201D}\u{e9}");
        assert_eq!(extended_ascii(&[b'o', b'k', 0, b'x']), "ok");
    }

    #[test]
    fn reads_little_endian_and_stops_at_the_end() {
        let mut r = Reader::new(&[1, 0, 0, 0, 2, 0, 9], "test");
        assert_eq!(r.u32().unwrap_or(0), 1);
        assert_eq!(r.u16().unwrap_or(0), 2);
        assert_eq!(r.remaining(), 1);
        assert!(r.u16().is_err());
        assert_eq!(r.u8().unwrap_or(0), 9);
        assert!(r.u8().is_err());
    }

    #[test]
    fn huge_lengths_do_not_overflow() {
        let mut r = Reader::new(&[0; 4], "test");
        assert!(r.take(usize::MAX).is_err());
        assert!(slice(&[0; 4], u64::MAX, 8, "x").is_err());
        assert!(slice(&[0; 4], 2, u64::MAX, "x").is_err());
        assert_eq!(slice(&[1, 2, 3, 4], 1, 2, "x").unwrap_or(&[]), &[2, 3]);
    }

    #[test]
    fn utf16_stops_at_nul_and_survives_bad_input() {
        assert_eq!(utf16(&[b'h', 0, b'i', 0, 0, 0, b'x', 0]), "hi");
        assert_eq!(utf16(&[b'a', 0, 0x00, 0xD8]), "a\u{FFFD}");
        assert_eq!(utf16(b"a"), "");
        assert_eq!(utf16(&[]), "");
    }
}
