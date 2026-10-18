//! GUIDs as they sit in the file, and the few constants the format fixes.
//!
//! A GUID is 16 bytes: the first three fields are little-endian, the last eight are in order.
//! `Guid::from_parts` takes the canonical `{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}` fields, so a
//! constant below can be read against the specification without decoding bytes by hand.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    pub const ZERO: Guid = Guid([0; 16]);

    pub const fn from_parts(a: u32, b: u16, c: u16, rest: [u8; 8]) -> Guid {
        let (a, b, c) = (a.to_le_bytes(), b.to_le_bytes(), c.to_le_bytes());
        Guid([a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], rest[0], rest[1], rest[2], rest[3], rest[4], rest[5], rest[6], rest[7]])
    }

    pub fn is_zero(self) -> bool {
        self.0 == [0; 16]
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = self.0;
        write!(f, "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-", b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6])?;
        write!(f, "{:02X}{:02X}-", b[8], b[9])?;
        for x in b.iter().skip(10) {
            write!(f, "{x:02X}")?;
        }
        write!(f, "}}")
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

/// A `.one` section file. [MS-ONESTORE] 2.3.1.
pub const FILE_TYPE_ONE: Guid = Guid::from_parts(0x7B5C_52E4, 0xD88C, 0x4DA7, [0xAE, 0xB1, 0x53, 0x78, 0xD0, 0x29, 0x96, 0xD3]);
/// A `.onetoc2` notebook table of contents. [MS-ONESTORE] 2.3.1.
pub const FILE_TYPE_TOC: Guid = Guid::from_parts(0x43FF_2FA1, 0xEFD9, 0x4C76, [0x9E, 0xE2, 0x10, 0xEA, 0x57, 0x22, 0x76, 0x5F]);
/// The only file format GUID either file may carry. [MS-ONESTORE] 2.3.1.
pub const FILE_FORMAT: Guid = Guid::from_parts(0x109A_DD3F, 0x911B, 0x49F5, [0xA5, 0xD0, 0x17, 0x91, 0xED, 0xC8, 0xAE, 0xD8]);
/// Start of a FileDataStoreObject. [MS-ONESTORE] 2.6.13.
pub const FILE_DATA_HEADER: Guid = Guid::from_parts(0xBDE3_16E7, 0x2665, 0x4511, [0xA4, 0xC4, 0x8D, 0x4D, 0x0B, 0x7A, 0x9E, 0xAC]);
/// End of a FileDataStoreObject. [MS-ONESTORE] 2.6.13.
pub const FILE_DATA_FOOTER: Guid = Guid::from_parts(0x71FB_A722, 0x0F79, 0x4A0B, [0xBB, 0x13, 0x89, 0x92, 0x56, 0x42, 0x6B, 0x24]);

/// An object or revision identity: a GUID and a number. [MS-ONESTORE] 2.2.1.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ExtendedGuid {
    pub guid: Guid,
    pub n: u32,
}

impl ExtendedGuid {
    pub const ZERO: ExtendedGuid = ExtendedGuid { guid: Guid::ZERO, n: 0 };

    pub fn is_zero(self) -> bool {
        self.guid.is_zero() && self.n == 0
    }
}

impl fmt::Display for ExtendedGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}", self.guid, self.n)
    }
}

impl fmt::Debug for ExtendedGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

/// A packed reference to an [`ExtendedGuid`] through the global identification table.
/// [MS-ONESTORE] 2.2.2: the low byte is `n`, the high three bytes index the table.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct CompactId {
    pub n: u8,
    pub index: u32,
}

impl CompactId {
    pub fn from_u32(v: u32) -> CompactId {
        CompactId { n: (v & 0xFF) as u8, index: v >> 8 }
    }

    pub fn is_zero(self) -> bool {
        self.n == 0 && self.index == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_the_bytes_in_a_real_file() {
        // The first 16 bytes of every `.one` file, as seen in a real OneNote backup.
        assert_eq!(FILE_TYPE_ONE.0, [0xE4, 0x52, 0x5C, 0x7B, 0x8C, 0xD8, 0xA7, 0x4D, 0xAE, 0xB1, 0x53, 0x78, 0xD0, 0x29, 0x96, 0xD3]);
        assert_eq!(FILE_TYPE_ONE.to_string(), "{7B5C52E4-D88C-4DA7-AEB1-5378D02996D3}");
        assert_eq!(FILE_TYPE_TOC.to_string(), "{43FF2FA1-EFD9-4C76-9EE2-10EA5722765F}");
        assert_eq!(FILE_FORMAT.to_string(), "{109ADD3F-911B-49F5-A5D0-1791EDC8AED8}");
        assert_eq!(FILE_DATA_HEADER.to_string(), "{BDE316E7-2665-4511-A4C4-8D4D0B7A9EAC}");
        assert_eq!(FILE_DATA_FOOTER.to_string(), "{71FBA722-0F79-4A0B-BB13-899256426B24}");
        assert!(Guid::ZERO.is_zero() && !FILE_FORMAT.is_zero());
    }

    #[test]
    fn compact_ids_split_the_way_the_spec_says() {
        let c = CompactId::from_u32(0x0012_3401);
        assert_eq!(c.n, 1);
        assert_eq!(c.index, 0x1234);
        assert!(CompactId::from_u32(0).is_zero());
        assert_eq!(ExtendedGuid::ZERO.to_string(), "{00000000-0000-0000-0000-000000000000}, 0");
        assert!(ExtendedGuid::ZERO.is_zero());
    }
}
