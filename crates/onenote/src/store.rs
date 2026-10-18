//! The revision store container: header, file node lists, object spaces, objects and file data.
//! [MS-ONESTORE] sections 2.1 to 2.6.
//!
//! The reader walks what is reachable from the header and keeps the last declaration of each
//! object, which is how a revision store says "this is the current state". It does not verify
//! CRCs, does not follow the free chunk list, and ignores anything it cannot reach.

use std::collections::BTreeMap;

use crate::bytes::{Reader, slice};
use crate::error::{OneError, Result};
use crate::guid::{CompactId, ExtendedGuid, FILE_DATA_FOOTER, FILE_DATA_HEADER, FILE_FORMAT, FILE_TYPE_ONE, FILE_TYPE_TOC, Guid};
use crate::props::{PropertySet, parse_object_prop_set};

const LIST_MAGIC: u64 = 0xA456_7AB1_F5F7_F4C4;
const LIST_FOOTER: u64 = 0x8BC2_15C3_8233_BA4B;
const HEADER_SIZE: u64 = 1024;
/// A `.one` file in the owner's backup folder reaches 160 MB. Anything past this is refused
/// rather than loaded into memory.
pub const MAX_FILE: u64 = 512 * 1024 * 1024;
/// Guards against a damaged file that points its lists at each other.
const MAX_FRAGMENTS: usize = 100_000;
const MAX_OBJECTS: usize = 500_000;

/// The parts of the 1024-byte header this reader uses. [MS-ONESTORE] 2.3.1.
#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub file_type: Guid,
    pub file: Guid,
    pub ancestor: Guid,
    pub transactions: u32,
    pub expected_length: u64,
    root_list: ChunkRef,
    transaction_log: ChunkRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkRef {
    stp: u64,
    cb: u64,
}

impl ChunkRef {
    /// "Nil" (all pointer bits set) and "zero" both mean there is nothing there.
    fn is_empty(self, stp_bits: u32) -> bool {
        let nil = if stp_bits >= 64 { u64::MAX } else { (1u64 << stp_bits) - 1 };
        (self.stp == nil && self.cb == 0) || (self.stp == 0 && self.cb == 0)
    }
}

/// What an object holds: a property set, or a pointer to the bytes of an inserted file.
#[derive(Clone, Debug)]
pub enum ObjectData {
    Props(PropertySet),
    /// [MS-ONESTORE] 2.5.27. `reference` is `<ifndf>{GUID}` for data inside this file, or
    /// `<file>name.onebin` for data in the sibling `onefiles` folder.
    File {
        reference: String,
        extension: String,
    },
}

/// One object of an object space: its identity, its type and its data.
#[derive(Clone, Debug)]
pub struct Object {
    pub oid: ExtendedGuid,
    /// [MS-ONESTORE] 2.6.14. The low 16 bits say what kind of object it is ([MS-ONE] 2.1.13).
    pub jcid: u32,
    pub data: ObjectData,
}

impl Object {
    pub fn kind(&self) -> u32 {
        self.jcid & 0xFFFF
    }

    pub fn props(&self) -> Option<&PropertySet> {
        match &self.data {
            ObjectData::Props(p) => Some(p),
            ObjectData::File { .. } => None,
        }
    }
}

/// A set of objects that is revised as a unit: the section itself, or one page.
#[derive(Clone, Debug, Default)]
pub struct ObjectSpace {
    pub id: ExtendedGuid,
    /// Objects by identity. A later declaration of the same identity replaces an earlier one.
    pub objects: BTreeMap<ExtendedGuid, Object>,
    /// Root objects by role: 1 is the content root, 2 the metadata root, 4 version metadata.
    pub roots: BTreeMap<u32, ExtendedGuid>,
    /// The global identification table that was in scope for this space's objects: the
    /// GUID each compact id's index stands for ([MS-ONESTORE] 2.1.3). Properties reference
    /// other objects by compact id, so this is how those references are followed.
    pub ids: BTreeMap<u32, Guid>,
    /// True when the object space is password protected, so its property sets are unreadable.
    pub encrypted: bool,
}

impl ObjectSpace {
    pub fn root(&self, role: u32) -> Option<&Object> {
        self.objects.get(self.roots.get(&role)?)
    }

    /// The identity a compact id stands for in this space.
    pub fn identity(&self, id: CompactId) -> Option<ExtendedGuid> {
        if id.is_zero() {
            return None;
        }
        Some(ExtendedGuid { guid: *self.ids.get(&id.index)?, n: u32::from(id.n) })
    }

    /// An object of this space, by the compact id a property carries.
    pub fn object(&self, id: CompactId) -> Option<&Object> {
        self.objects.get(&self.identity(id)?)
    }

    pub fn content_root(&self) -> Option<&Object> {
        self.root(1)
    }

    pub fn metadata_root(&self) -> Option<&Object> {
        self.root(2)
    }
}

/// A whole `.one` file, read.
#[derive(Clone, Debug)]
pub struct Store {
    pub header: Header,
    /// The root object space first, then the rest in file order.
    pub spaces: Vec<ObjectSpace>,
    pub root_space: ExtendedGuid,
    /// Inserted files (pictures, attachments) by their identity, as byte ranges of the file.
    pub file_data: BTreeMap<Guid, (u64, u64)>,
}

impl Store {
    pub fn space(&self, id: ExtendedGuid) -> Option<&ObjectSpace> {
        self.spaces.iter().find(|s| s.id == id)
    }

    /// The bytes of an inserted file, by the `<ifndf>{GUID}` reference an object carries.
    pub fn file_bytes<'a>(&self, data: &'a [u8], reference: &str) -> Option<&'a [u8]> {
        let guid = parse_ifndf(reference)?;
        let (stp, cb) = *self.file_data.get(&guid)?;
        slice(data, stp, cb, "an inserted file").ok()
    }
}

/// `<ifndf>{B0F45...}` to the GUID it names. Returns `None` for `<file>` and `<invfdo>`.
fn parse_ifndf(reference: &str) -> Option<Guid> {
    let inner = reference.strip_prefix("<ifndf>")?.trim();
    let hex: Vec<u8> = inner.trim_start_matches('{').trim_end_matches('}').bytes().filter(u8::is_ascii_hexdigit).collect();
    if hex.len() != 32 {
        return None;
    }
    let mut n = [0u8; 16];
    for (i, pair) in hex.chunks_exact(2).enumerate() {
        let s = std::str::from_utf8(pair).ok()?;
        n[i] = u8::from_str_radix(s, 16).ok()?;
    }
    // Canonical text order to the file's byte order: the first three fields are little-endian.
    Some(Guid([n[3], n[2], n[1], n[0], n[5], n[4], n[7], n[6], n[8], n[9], n[10], n[11], n[12], n[13], n[14], n[15]]))
}

/// Reads the header and checks that this is a `.one` section file.
pub fn read_header(data: &[u8]) -> Result<Header> {
    if (data.len() as u64) < HEADER_SIZE {
        return Err(OneError::NotASection(format!("the file is only {} bytes", data.len())));
    }
    let mut r = Reader::new(data, "the header");
    let file_type = Guid(r.guid()?);
    let file = Guid(r.guid()?);
    let _legacy = r.guid()?;
    let format = Guid(r.guid()?);
    if file_type == FILE_TYPE_TOC {
        return Err(OneError::NotesetFile);
    }
    if file_type != FILE_TYPE_ONE {
        return Err(OneError::NotASection(format!("the file starts with {file_type}")));
    }
    if format != FILE_FORMAT {
        return Err(OneError::NotASection(format!("unknown format {format}")));
    }
    r.skip(16)?; // four version codes
    r.skip(16)?; // legacy free chunk list and transaction log
    let transactions = r.u32()?;
    r.skip(28)?; // legacy and flag fields
    let ancestor = Guid(r.guid()?);
    r.skip(4)?; // crcName
    let _hashed = chunk_ref64x32(&mut r)?;
    let transaction_log = chunk_ref64x32(&mut r)?;
    let root_list = chunk_ref64x32(&mut r)?;
    let _free = chunk_ref64x32(&mut r)?;
    let expected_length = r.u64()?;
    Ok(Header { file_type, file, ancestor, transactions, expected_length, root_list, transaction_log })
}

fn chunk_ref64x32(r: &mut Reader<'_>) -> Result<ChunkRef> {
    let stp = r.u64()?;
    let cb = u64::from(r.u32()?);
    Ok(ChunkRef { stp, cb })
}

/// Reads a whole `.one` file.
pub fn read(data: &[u8]) -> Result<Store> {
    if data.len() as u64 > MAX_FILE {
        return Err(OneError::TooLarge(data.len() as u64));
    }
    let header = read_header(data)?;
    let mut w = Walk { data, limits: Limits::default(), counts: BTreeMap::new(), file_data: BTreeMap::new() };
    w.counts = transaction_counts(data, header).unwrap_or_default();
    let mut spaces: Vec<ObjectSpace> = Vec::new();
    let mut root_space = ExtendedGuid::ZERO;
    if header.root_list.is_empty(64) {
        return Err(OneError::Corrupt("the file has no root file node list".into()));
    }
    for node in w.list(header.root_list)? {
        match node.id {
            0x004 => root_space = extended_guid(&node.data, 0)?,
            0x008 => {
                let gosid = extended_guid(&node.data, node.ref_len)?;
                let mut space = ObjectSpace { id: gosid, ..ObjectSpace::default() };
                w.object_space(node.chunk, &mut space)?;
                spaces.push(space);
            }
            0x090 => w.file_data_list(node.chunk)?,
            _ => {}
        }
    }
    if let Some(i) = spaces.iter().position(|s| s.id == root_space) {
        spaces.swap(0, i);
    }
    Ok(Store { header, spaces, root_space, file_data: w.file_data })
}

/// How many nodes of each list the last committed transaction left valid.
/// [MS-ONESTORE] 2.3.3. Anything past that is an unfinished write and is ignored.
fn transaction_counts(data: &[u8], header: Header) -> Option<BTreeMap<u32, u32>> {
    if header.transaction_log.is_empty(64) {
        return None;
    }
    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
    let mut at = header.transaction_log;
    let mut committed = 0u32;
    for _ in 0..MAX_FRAGMENTS {
        let fragment = slice(data, at.stp, at.cb, "the transaction log").ok()?;
        let mut r = Reader::new(fragment, "a transaction log fragment");
        while r.remaining() >= 8 && committed < header.transactions {
            let src = r.u32().ok()?;
            let value = r.u32().ok()?;
            if src == 1 {
                committed += 1; // a sentinel entry ends one transaction
            } else {
                counts.insert(src, value);
            }
        }
        if committed >= header.transactions || fragment.len() < 12 {
            break;
        }
        let mut tail = Reader::new(fragment.get(fragment.len() - 12..)?, "the next log fragment");
        let next = ChunkRef { stp: tail.u64().ok()?, cb: u64::from(tail.u32().ok()?) };
        if next.is_empty(64) || next == at {
            break;
        }
        at = next;
    }
    Some(counts)
}

/// One file node, flattened: its type, the bytes after any chunk reference, and that reference.
struct Node {
    id: u16,
    /// The node's own data, starting at the chunk reference if it has one.
    data: Vec<u8>,
    /// Bytes of the chunk reference at the start of `data` (0 when the node has none).
    ref_len: usize,
    chunk: ChunkRef,
}

#[derive(Debug)]
struct Limits {
    fragments: usize,
    objects: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { fragments: MAX_FRAGMENTS, objects: MAX_OBJECTS }
    }
}

struct Walk<'a> {
    data: &'a [u8],
    limits: Limits,
    counts: BTreeMap<u32, u32>,
    file_data: BTreeMap<Guid, (u64, u64)>,
}

impl Walk<'_> {
    /// Every file node of one list, following its fragments.
    fn list(&mut self, start: ChunkRef) -> Result<Vec<Node>> {
        let mut out = Vec::new();
        let mut at = start;
        let mut seen = Vec::new();
        let mut list_id = None;
        let mut allowed = u32::MAX;
        loop {
            if self.limits.fragments == 0 {
                return Err(OneError::Corrupt("too many file node list fragments".into()));
            }
            self.limits.fragments -= 1;
            if seen.contains(&at.stp) {
                return Err(OneError::Corrupt("a file node list points back at itself".into()));
            }
            seen.push(at.stp);
            let fragment = slice(self.data, at.stp, at.cb, "a file node list")?;
            if fragment.len() < 36 {
                return Err(OneError::Truncated("a file node list fragment"));
            }
            let mut r = Reader::new(fragment, "a file node list header");
            if r.u64()? != LIST_MAGIC {
                return Err(OneError::Corrupt(format!("no file node list at offset {}", at.stp)));
            }
            let id = r.u32()?;
            let _sequence = r.u32()?;
            if *list_id.get_or_insert(id) != id {
                return Err(OneError::Corrupt("a file node list changes identity between fragments".into()));
            }
            if let Some(&n) = self.counts.get(&id) {
                allowed = n;
            }
            let body_end = fragment.len() - 20;
            let mut tail = Reader::new(fragment.get(body_end..).unwrap_or(&[]), "a file node list footer");
            let next = ChunkRef { stp: tail.u64()?, cb: u64::from(tail.u32()?) };
            if tail.u64()? != LIST_FOOTER {
                return Err(OneError::Corrupt(format!("the file node list at {} has no footer", at.stp)));
            }
            let mut terminated = false;
            while r.pos() + 4 <= body_end && (out.len() as u32) < allowed {
                let start = r.pos();
                let word = r.u32()?;
                let id = (word & 0x3FF) as u16;
                let size = ((word >> 10) & 0x1FFF) as usize;
                if id == 0x0FF || (id == 0 && size == 0) {
                    terminated = true;
                    break;
                }
                if size < 4 || start + size > body_end {
                    return Err(OneError::Corrupt(format!("a file node of {size} bytes does not fit its fragment")));
                }
                let base = (word >> 27) & 0xF;
                let body = fragment.get(start + 4..start + size).unwrap_or(&[]);
                let (chunk, ref_len) = if base == 1 || base == 2 {
                    let stp_format = ((word >> 23) & 0x3) as usize;
                    let cb_format = ((word >> 25) & 0x3) as usize;
                    read_chunk_ref(body, stp_format, cb_format)?
                } else {
                    (ChunkRef { stp: 0, cb: 0 }, 0)
                };
                out.push(Node { id, data: body.to_vec(), ref_len, chunk });
                r.skip(size - 4)?;
            }
            if !terminated || next.is_empty(64) || (out.len() as u32) >= allowed {
                return Ok(out);
            }
            at = next;
        }
    }

    /// An object space manifest list: find its revision manifest list and read the last one.
    fn object_space(&mut self, at: ChunkRef, space: &mut ObjectSpace) -> Result<()> {
        let mut revisions = None;
        for node in self.list(at)? {
            match node.id {
                0x00C => {}                            // the start node repeats the identity
                0x010 => revisions = Some(node.chunk), // all but the last are ignored
                _ => {}
            }
        }
        let Some(list) = revisions else { return Ok(()) };
        self.revisions(list, space)
    }

    /// A revision manifest list. Later revisions revise earlier ones, so reading the list in
    /// order and keeping the last declaration of each object leaves the current state.
    fn revisions(&mut self, at: ChunkRef, space: &mut ObjectSpace) -> Result<()> {
        let mut table = GlobalIds::default();
        let mut in_revision = false;
        let mut skip = false;
        for node in self.list(at)? {
            match node.id {
                0x014 => {} // list start
                0x01E | 0x01F => {
                    // A revision manifest: 20 bytes rid, 20 ridDependent, 4 role, 2 encryption.
                    in_revision = true;
                    let encrypted = node.data.get(44..46).map(|b| b != [0, 0]).unwrap_or(false);
                    let role = u32::from_le_bytes([
                        node.data.get(40).copied().unwrap_or(0),
                        node.data.get(41).copied().unwrap_or(0),
                        node.data.get(42).copied().unwrap_or(0),
                        node.data.get(43).copied().unwrap_or(0),
                    ]);
                    // Role 1 is the view the app shows. Other roles are version history.
                    skip = role != 1;
                    space.encrypted |= encrypted && !skip;
                }
                0x07C => space.encrypted = true,
                0x01C => in_revision = false,
                0x022 => table = GlobalIds::default(),
                0x024 => table.add(&node.data)?,
                0x0B0 if in_revision && !skip => {
                    let group = node.chunk;
                    self.object_group(group, &mut table, space)?;
                }
                0x05A if in_revision && !skip => {
                    let oid = extended_guid(&node.data, 0)?;
                    let role = u32::from_le_bytes([
                        node.data.get(20).copied().unwrap_or(0),
                        node.data.get(21).copied().unwrap_or(0),
                        node.data.get(22).copied().unwrap_or(0),
                        node.data.get(23).copied().unwrap_or(0),
                    ]);
                    space.roots.insert(role, oid);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// An object group: the objects themselves.
    fn object_group(&mut self, at: ChunkRef, table: &mut GlobalIds, space: &mut ObjectSpace) -> Result<()> {
        let mut group_table = table.clone();
        for node in self.list(at)? {
            // The space keeps every entry it sees, so a property's compact id can be
            // followed after the walk, when the table itself is long gone.
            space.ids.extend(group_table.by_index.iter().map(|(k, v)| (*k, *v)));
            match node.id {
                0x0B4 | 0x0B8 => {} // group start and end
                0x022 => group_table = GlobalIds::default(),
                0x024 => group_table.add(&node.data)?,
                // A property set object: chunk reference, then oid, jcid, flags, reference count.
                0x0A4 | 0x0A5 | 0x0C4 | 0x0C5 => {
                    let mut r = Reader::new(&node.data, "an object declaration");
                    r.skip(node.ref_len)?;
                    let oid = group_table.resolve(CompactId::from_u32(r.u32()?))?;
                    let jcid = r.u32()?;
                    let data = if space.encrypted {
                        ObjectData::Props(PropertySet::default())
                    } else {
                        let bytes = slice(self.data, node.chunk.stp, node.chunk.cb, "an object's property set")?;
                        ObjectData::Props(parse_object_prop_set(bytes)?)
                    };
                    self.insert(space, Object { oid, jcid, data })?;
                }
                // An inserted file: no chunk reference, the data is named by a string.
                0x072 | 0x073 => {
                    let mut r = Reader::new(&node.data, "a file data declaration");
                    let oid = group_table.resolve(CompactId::from_u32(r.u32()?))?;
                    let jcid = r.u32()?;
                    r.skip(if node.id == 0x072 { 1 } else { 4 })?; // reference count
                    let reference = string_in_storage(&mut r)?;
                    let extension = string_in_storage(&mut r).unwrap_or_default();
                    self.insert(space, Object { oid, jcid, data: ObjectData::File { reference, extension } })?;
                }
                _ => {}
            }
        }
        space.ids.extend(group_table.by_index.iter().map(|(k, v)| (*k, *v)));
        Ok(())
    }

    fn insert(&mut self, space: &mut ObjectSpace, object: Object) -> Result<()> {
        if self.limits.objects == 0 {
            return Err(OneError::Corrupt("too many objects".into()));
        }
        self.limits.objects -= 1;
        space.objects.insert(object.oid, object);
        Ok(())
    }

    /// The file data store: the bytes of every inserted file kept inside this file.
    fn file_data_list(&mut self, at: ChunkRef) -> Result<()> {
        for node in self.list(at)? {
            if node.id != 0x094 {
                continue;
            }
            let guid = Guid(node.data.get(node.ref_len..node.ref_len + 16).and_then(|b| <[u8; 16]>::try_from(b).ok()).unwrap_or([0; 16]));
            let object = slice(self.data, node.chunk.stp, node.chunk.cb, "an inserted file")?;
            let mut r = Reader::new(object, "an inserted file");
            if Guid(r.guid()?) != FILE_DATA_HEADER {
                return Err(OneError::Corrupt("an inserted file has the wrong header".into()));
            }
            let length = r.u64()?;
            r.skip(12)?; // unused and reserved
            let start = node.chunk.stp + 36;
            if slice(self.data, start, length, "an inserted file")?.len() as u64 != length {
                return Err(OneError::Truncated("an inserted file"));
            }
            // The whole structure ends on an 8-byte boundary, so the footer starts at the next
            // multiple of 8 after the data. [MS-ONESTORE] 2.6.13.
            let footer_at = (start + length).next_multiple_of(8);
            let footer = slice(self.data, footer_at, 16, "the end of an inserted file")?;
            if Guid(<[u8; 16]>::try_from(footer).unwrap_or([0; 16])) != FILE_DATA_FOOTER {
                return Err(OneError::Corrupt("an inserted file has the wrong footer".into()));
            }
            self.file_data.insert(guid, (start, length));
        }
        Ok(())
    }
}

/// The table that turns a 4-byte compact id into a full identity. [MS-ONESTORE] 2.1.3.
#[derive(Clone, Default)]
struct GlobalIds {
    by_index: BTreeMap<u32, Guid>,
}

impl GlobalIds {
    fn add(&mut self, data: &[u8]) -> Result<()> {
        let mut r = Reader::new(data, "a global id table entry");
        let index = r.u32()?;
        self.by_index.insert(index, Guid(r.guid()?));
        Ok(())
    }

    fn resolve(&self, id: CompactId) -> Result<ExtendedGuid> {
        if id.is_zero() {
            return Ok(ExtendedGuid::ZERO);
        }
        let guid = *self.by_index.get(&id.index).ok_or_else(|| OneError::Corrupt(format!("no global id {} in the table", id.index)))?;
        Ok(ExtendedGuid { guid, n: u32::from(id.n) })
    }
}

fn extended_guid(data: &[u8], at: usize) -> Result<ExtendedGuid> {
    let mut r = Reader::new(data.get(at..).ok_or(OneError::Truncated("an identity"))?, "an identity");
    Ok(ExtendedGuid { guid: Guid(r.guid()?), n: r.u32()? })
}

/// A chunk reference whose field widths the node header chose. [MS-ONESTORE] 2.4.3.
fn read_chunk_ref(body: &[u8], stp_format: usize, cb_format: usize) -> Result<(ChunkRef, usize)> {
    let (stp_len, stp_shift) = match stp_format {
        0 => (8, 1),
        1 => (4, 1),
        2 => (2, 8),
        _ => (4, 8),
    };
    let (cb_len, cb_shift) = match cb_format {
        0 => (4, 1),
        1 => (8, 1),
        2 => (1, 8),
        _ => (2, 8),
    };
    let mut r = Reader::new(body, "a chunk reference");
    let stp = r.uint(stp_len)?;
    let cb = r.uint(cb_len)?;
    // A "nil" pointer is all bits set, and must stay that way rather than being multiplied.
    let all_set = stp == if stp_len >= 8 { u64::MAX } else { (1u64 << (stp_len * 8)) - 1 };
    let stp = if all_set { u64::MAX } else { stp.saturating_mul(stp_shift) };
    Ok((ChunkRef { stp, cb: cb.saturating_mul(cb_shift) }, stp_len + cb_len))
}

/// A counted UTF-16 string. [MS-ONESTORE] 2.2.3: the count is characters, not bytes.
fn string_in_storage(r: &mut Reader<'_>) -> Result<String> {
    let cch = r.u32()? as usize;
    let bytes = cch.checked_mul(2).ok_or(OneError::Truncated("a string"))?;
    Ok(crate::bytes::utf16(r.take(bytes)?))
}

#[cfg(test)]
pub mod build {
    //! A builder for synthetic `.one` files, so the tests do not need a real notebook (the
    //! owner's files hold personal data and stay off the repository).

    use super::*;
    use crate::props::tests_support::Build;

    /// The JCID flag that says an object holds a property set ([MS-ONESTORE] 2.6.14).
    pub const JCID_PROP_SET: u32 = 0x0002_0000; // IsPropertySet

    #[derive(Default)]
    pub struct One {
        pub data: Vec<u8>,
        next_list: u32,
        counts: Vec<(u32, u32)>,
    }

    pub struct Decl {
        pub oid: u32,
        pub jcid: u32,
        pub props: Vec<u8>,
    }

    impl One {
        pub fn new() -> One {
            One { data: vec![0; HEADER_SIZE as usize], next_list: 0x10, counts: Vec::new() }
        }

        fn put(&mut self, bytes: &[u8]) -> ChunkRef {
            while !self.data.len().is_multiple_of(8) {
                self.data.push(0);
            }
            let stp = self.data.len() as u64;
            self.data.extend_from_slice(bytes);
            ChunkRef { stp, cb: bytes.len() as u64 }
        }

        /// A file node with a chunk reference (8-byte pointer, 4-byte size) and extra data.
        fn node(id: u16, base: u32, chunk: Option<ChunkRef>, rest: &[u8]) -> Vec<u8> {
            let mut body = Vec::new();
            if let Some(c) = chunk {
                body.extend_from_slice(&c.stp.to_le_bytes());
                body.extend_from_slice(&(c.cb as u32).to_le_bytes());
            }
            body.extend_from_slice(rest);
            let size = 4 + body.len();
            let word = u32::from(id) | ((size as u32) << 10) | (base << 27) | (1 << 31);
            let mut out = word.to_le_bytes().to_vec();
            out.extend_from_slice(&body);
            out
        }

        /// Wraps file nodes in a fragment and writes it.
        fn fragment(&mut self, nodes: Vec<Vec<u8>>) -> ChunkRef {
            let id = self.next_list;
            self.next_list += 1;
            self.counts.push((id, nodes.len() as u32));
            let mut body = Vec::new();
            body.extend_from_slice(&LIST_MAGIC.to_le_bytes());
            body.extend_from_slice(&id.to_le_bytes());
            body.extend_from_slice(&0u32.to_le_bytes());
            for n in nodes {
                body.extend_from_slice(&n);
            }
            body.extend_from_slice(&u64::MAX.to_le_bytes()); // nextFragment: nil
            body.extend_from_slice(&0u32.to_le_bytes());
            body.extend_from_slice(&LIST_FOOTER.to_le_bytes());
            self.put(&body)
        }

        pub fn guid(n: u8) -> Guid {
            let mut g = [0u8; 16];
            g[0] = n;
            g[15] = 0xAA;
            Guid(g)
        }

        fn egid(n: u8) -> Vec<u8> {
            let mut out = Self::guid(n).0.to_vec();
            out.extend_from_slice(&1u32.to_le_bytes());
            out
        }

        /// An object group holding the given declarations, all under one global id table.
        fn group(&mut self, decls: &[Decl]) -> ChunkRef {
            self.group_with_ids(decls, &[])
        }

        /// The same, with extra entries in the global id table: the identities of other
        /// object spaces, which a property references by compact id.
        fn group_with_ids(&mut self, decls: &[Decl], extra: &[u32]) -> ChunkRef {
            let mut nodes = vec![One::node(0x0B4, 0, None, &Self::egid(9)), One::node(0x022, 0, None, &[])];
            let mut indexes: Vec<u32> = decls.iter().map(|d| d.oid >> 8).chain(extra.iter().copied()).collect();
            indexes.sort_unstable();
            indexes.dedup();
            for i in &indexes {
                let mut e = i.to_le_bytes().to_vec();
                e.extend_from_slice(&Self::guid(*i as u8).0);
                nodes.push(One::node(0x024, 0, None, &e));
            }
            nodes.push(One::node(0x028, 0, None, &[]));
            for d in decls {
                let blob = self.put(&d.props);
                let mut rest = d.oid.to_le_bytes().to_vec();
                rest.extend_from_slice(&d.jcid.to_le_bytes());
                rest.push(0); // flags
                rest.push(1); // reference count
                nodes.push(One::node(0x0A4, 1, Some(blob), &rest));
            }
            nodes.push(One::node(0x0B8, 0, None, &[]));
            self.fragment(nodes)
        }

        /// One object space: its objects and the root object of role 1.
        pub fn space(&mut self, gosid: u8, decls: &[Decl], root_oid: u32) -> ChunkRef {
            self.space_with(gosid, decls, root_oid, false)
        }

        /// One object space whose objects reference other object spaces by compact id.
        pub fn space_ids(&mut self, gosid: u8, decls: &[Decl], root_oid: u32, extra_ids: &[u32]) -> ChunkRef {
            let group = self.group_with_ids(decls, extra_ids);
            self.space_from_group(gosid, group, root_oid, false)
        }

        /// The same, with the revision manifest's encryption flag set: a protected section.
        pub fn space_with(&mut self, gosid: u8, decls: &[Decl], root_oid: u32, encrypted: bool) -> ChunkRef {
            let group = self.group(decls);
            self.space_from_group(gosid, group, root_oid, encrypted)
        }

        fn space_from_group(&mut self, gosid: u8, group: ChunkRef, root_oid: u32, encrypted: bool) -> ChunkRef {
            let mut manifest = Vec::new();
            manifest.extend_from_slice(&Self::egid(gosid)); // rid
            manifest.extend_from_slice(&[0; 20]); // no dependency
            manifest.extend_from_slice(&1u32.to_le_bytes()); // role 1
            manifest.extend_from_slice(&if encrypted { 2u16 } else { 0 }.to_le_bytes());
            let mut root = Self::guid((root_oid >> 8) as u8).0.to_vec();
            root.extend_from_slice(&(root_oid & 0xFF).to_le_bytes());
            root.extend_from_slice(&1u32.to_le_bytes()); // role 1
            let mut revision_nodes = vec![One::node(0x014, 0, None, &Self::egid(gosid)), One::node(0x01E, 0, None, &manifest)];
            revision_nodes.push(One::node(0x0B0, 2, Some(group), &Self::egid(9)));
            revision_nodes.push(One::node(0x05A, 0, None, &root));
            revision_nodes.push(One::node(0x01C, 0, None, &[]));
            let revisions = self.fragment(revision_nodes);
            let space_nodes = vec![One::node(0x00C, 0, None, &Self::egid(gosid)), One::node(0x010, 2, Some(revisions), &[])];
            self.fragment(space_nodes)
        }

        /// An inserted file, returning the node that the file data store list needs.
        pub fn file_data(&mut self, guid: Guid, bytes: &[u8]) -> Vec<u8> {
            let mut object = FILE_DATA_HEADER.0.to_vec();
            object.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            object.extend_from_slice(&[0; 12]);
            object.extend_from_slice(bytes);
            while !object.len().is_multiple_of(8) {
                object.push(0);
            }
            object.extend_from_slice(&FILE_DATA_FOOTER.0);
            let chunk = self.put(&object);
            One::node(0x094, 1, Some(chunk), &guid.0)
        }

        /// Writes the root list and the header. `spaces` are `(gosid, list)` pairs.
        pub fn finish(mut self, spaces: &[(u8, ChunkRef)], root: u8, file_data: Vec<Vec<u8>>) -> Vec<u8> {
            let store = if file_data.is_empty() { None } else { Some(self.fragment(file_data)) };
            let mut nodes = Vec::new();
            for (gosid, list) in spaces {
                nodes.push(One::node(0x008, 2, Some(*list), &Self::egid(*gosid)));
            }
            nodes.push(One::node(0x004, 0, None, &Self::egid(root)));
            if let Some(s) = store {
                nodes.push(One::node(0x090, 2, Some(s), &[]));
            }
            let root_list = self.fragment(nodes);
            // The transaction log: one entry per list, then a sentinel.
            let mut log = Vec::new();
            for (id, count) in &self.counts {
                log.extend_from_slice(&id.to_le_bytes());
                log.extend_from_slice(&count.to_le_bytes());
            }
            log.extend_from_slice(&1u32.to_le_bytes());
            log.extend_from_slice(&0u32.to_le_bytes());
            log.extend_from_slice(&[0xFF; 8]); // nextFragment: nil
            log.extend_from_slice(&0u32.to_le_bytes());
            let log_ref = self.put(&log);

            let mut h = Vec::with_capacity(HEADER_SIZE as usize);
            h.extend_from_slice(&FILE_TYPE_ONE.0);
            h.extend_from_slice(&Self::guid(0x77).0);
            h.extend_from_slice(&[0; 16]);
            h.extend_from_slice(&FILE_FORMAT.0);
            for _ in 0..4 {
                h.extend_from_slice(&0x2Au32.to_le_bytes());
            }
            h.extend_from_slice(&[0; 16]);
            h.extend_from_slice(&1u32.to_le_bytes()); // cTransactionsInLog
            h.extend_from_slice(&[0; 28]);
            h.extend_from_slice(&[0; 16]); // guidAncestor
            h.extend_from_slice(&0u32.to_le_bytes()); // crcName
            let put_ref = |h: &mut Vec<u8>, c: ChunkRef| {
                h.extend_from_slice(&c.stp.to_le_bytes());
                h.extend_from_slice(&(c.cb as u32).to_le_bytes());
            };
            put_ref(&mut h, ChunkRef { stp: u64::MAX, cb: 0 }); // hashed chunk list
            put_ref(&mut h, log_ref);
            put_ref(&mut h, root_list);
            put_ref(&mut h, ChunkRef { stp: u64::MAX, cb: 0 }); // free chunk list
            let total = self.data.len() as u64;
            h.extend_from_slice(&total.to_le_bytes());
            h.resize(HEADER_SIZE as usize, 0);
            self.data.splice(0..HEADER_SIZE as usize, h);
            self.data
        }
    }

    /// A whole section file with pages on it, built the way OneNote lays one out
    /// ([MS-ONE] 2.1.14, 2.2.17, 2.2.18): a section node listing a page series, the series
    /// pointing at one object space per page with its metadata beside it.
    #[derive(Default)]
    pub struct OneNote {
        pages: Vec<(String, u8, Vec<Line>)>,
    }

    /// One line of text on a page, with its marks.
    #[derive(Clone)]
    pub struct Line {
        pub text: String,
        pub bold: bool,
        pub italic: bool,
    }

    // The property ids and object types the builder writes. The reader has its own copies,
    // so a wrong constant on either side fails a test instead of passing silently.
    const P_ELEMENT_CHILDREN: u32 = 0x2400_1C20;
    const P_CONTENT_CHILDREN: u32 = 0x2400_1C1F;
    const P_CHILD_SPACES: u32 = 0x2C00_1D63;
    const P_META_ABOVE: u32 = 0x2400_3442;
    const P_CACHED_TITLE: u32 = 0x1C00_1CF3;
    const P_PAGE_LEVEL: u32 = 0x1400_1DFF;
    const P_TEXT: u32 = 0x1C00_1C22;
    const P_RUN_FORMATTING: u32 = 0x2400_1E13;
    const P_BOLD: u32 = 0x0800_1C04;
    const P_ITALIC: u32 = 0x0800_1C05;
    const J_SECTION: u32 = 0x0007;
    const J_PAGE_SERIES: u32 = 0x0008;
    const J_PAGE: u32 = 0x000B;
    const J_OUTLINE: u32 = 0x000C;
    const J_OUTLINE_ELEMENT: u32 = 0x000D;
    const J_RICH_TEXT: u32 = 0x000E;
    const J_PAGE_META: u32 = 0x0030;
    const J_PAGE_MANIFEST: u32 = 0x0037;
    const J_STYLE: u32 = 0x004D;

    impl OneNote {
        pub fn new() -> OneNote {
            OneNote::default()
        }

        pub fn line(text: &str) -> Line {
            Line { text: text.to_string(), bold: false, italic: false }
        }

        pub fn styled(text: &str, bold: bool, italic: bool) -> Line {
            Line { text: text.to_string(), bold, italic }
        }

        /// Adds a page. `level` is OneNote's page level, 1 for a page and 2 or 3 for a
        /// sub-page of the page above it.
        pub fn page(&mut self, title: &str, level: u8, lines: &[Line]) {
            self.pages.push((title.to_string(), level, lines.to_vec()));
        }

        pub fn with_page(mut self, title: &str, lines: &[Line]) -> OneNote {
            self.page(title, 1, lines);
            self
        }

        /// Writes the file.
        pub fn finish(self) -> Vec<u8> {
            let mut one = One::new();
            let mut spaces = Vec::new();
            // One object space per page: manifest, page, outline, elements and their text.
            for (i, (_, _, lines)) in self.pages.iter().enumerate() {
                let gosid = (10 + i) as u8;
                let mut decls = Vec::new();
                let (manifest_oid, page_oid, outline_oid) = (0x0000_0101, 0x0000_0201, 0x0000_0301);
                let mut manifest = Build::default();
                manifest.objects(P_CONTENT_CHILDREN, &[page_oid]);
                decls.push(Decl { oid: manifest_oid, jcid: JCID_PROP_SET | J_PAGE_MANIFEST, props: manifest.finish() });
                let mut page = Build::default();
                page.objects(P_ELEMENT_CHILDREN, &[outline_oid]);
                decls.push(Decl { oid: page_oid, jcid: JCID_PROP_SET | J_PAGE, props: page.finish() });
                // One outline element per line, each holding one rich text node.
                let element_oids: Vec<u32> = (0..lines.len()).map(|n| ((4 + n * 3) as u32) << 8 | 1).collect();
                let mut outline = Build::default();
                outline.objects(P_ELEMENT_CHILDREN, &element_oids);
                decls.push(Decl { oid: outline_oid, jcid: JCID_PROP_SET | J_OUTLINE, props: outline.finish() });
                for (n, line) in lines.iter().enumerate() {
                    let element_oid = element_oids.get(n).copied().unwrap_or(0);
                    let text_oid = ((5 + n * 3) as u32) << 8 | 1;
                    let style_oid = ((6 + n * 3) as u32) << 8 | 1;
                    let mut element = Build::default();
                    element.objects(P_CONTENT_CHILDREN, &[text_oid]);
                    decls.push(Decl { oid: element_oid, jcid: JCID_PROP_SET | J_OUTLINE_ELEMENT, props: element.finish() });
                    let mut text = Build::default();
                    text.text(P_TEXT, &line.text);
                    text.objects(P_RUN_FORMATTING, &[style_oid]);
                    decls.push(Decl { oid: text_oid, jcid: JCID_PROP_SET | J_RICH_TEXT, props: text.finish() });
                    let mut style = Build::default();
                    style.bool(P_BOLD, line.bold);
                    style.bool(P_ITALIC, line.italic);
                    decls.push(Decl { oid: style_oid, jcid: JCID_PROP_SET | J_STYLE, props: style.finish() });
                }
                let list = one.space(gosid, &decls, manifest_oid);
                spaces.push((gosid, list));
            }
            // The section space: the section node, one page series, and the page metadata.
            let (section_oid, series_oid) = (0x0000_0101, 0x0000_0201);
            let meta_oids: Vec<u32> = (0..self.pages.len()).map(|i| ((3 + i) as u32) << 8 | 1).collect();
            let space_ids: Vec<u32> = (0..self.pages.len()).map(|i| ((10 + i) as u32) << 8 | 1).collect();
            let mut decls = Vec::new();
            let mut section = Build::default();
            section.objects(P_ELEMENT_CHILDREN, &[series_oid]);
            decls.push(Decl { oid: section_oid, jcid: JCID_PROP_SET | J_SECTION, props: section.finish() });
            let mut series = Build::default();
            series.spaces(P_CHILD_SPACES, &space_ids);
            series.objects(P_META_ABOVE, &meta_oids);
            decls.push(Decl { oid: series_oid, jcid: JCID_PROP_SET | J_PAGE_SERIES, props: series.finish() });
            for (i, (title, level, _)) in self.pages.iter().enumerate() {
                let mut meta = Build::default();
                meta.text(P_CACHED_TITLE, title);
                meta.u32(P_PAGE_LEVEL, u32::from(*level));
                decls.push(Decl { oid: meta_oids.get(i).copied().unwrap_or(0), jcid: JCID_PROP_SET | J_PAGE_META, props: meta.finish() });
            }
            // The page spaces are named by compact id, so their identities join the table.
            let extra: Vec<u32> = (0..self.pages.len()).map(|i| (10 + i) as u32).collect();
            let section_list = one.space_ids(1, &decls, section_oid, &extra);
            spaces.insert(0, (1, section_list));
            one.finish(&spaces, 1, Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build::{Decl, JCID_PROP_SET, One};
    use super::*;
    use crate::props::tests_support::Build;

    fn simple() -> Vec<u8> {
        let mut one = One::new();
        let mut title = Build::default();
        title.text(0x1C00_1CF3, "My page");
        let mut child = Build::default();
        child.u32(0x1400_1DFF, 2);
        let decls = vec![
            Decl { oid: 0x0000_0101, jcid: JCID_PROP_SET | 0x0007, props: title.finish() },
            Decl { oid: 0x0000_0201, jcid: JCID_PROP_SET | 0x000B, props: child.finish() },
        ];
        let space = one.space(1, &decls, 0x0000_0101);
        one.finish(&[(1, space)], 1, Vec::new())
    }

    #[test]
    fn reads_the_header_of_a_section_file() {
        let data = simple();
        let h = read_header(&data).expect("header");
        assert_eq!(h.file_type, FILE_TYPE_ONE);
        assert_eq!(h.transactions, 1);
        assert_eq!(h.expected_length, data.len() as u64, "the header's length matches the file");
    }

    #[test]
    fn a_table_of_contents_file_is_named_as_such() {
        let mut data = simple();
        data.splice(0..16, FILE_TYPE_TOC.0);
        assert!(matches!(read_header(&data), Err(OneError::NotesetFile)));
        assert!(matches!(read(&data), Err(OneError::NotesetFile)));
    }

    #[test]
    fn other_files_are_refused() {
        assert!(matches!(read_header(&[]), Err(OneError::NotASection(_))));
        assert!(matches!(read_header(&[0; 2048]), Err(OneError::NotASection(_))));
        let mut wrong_format = simple();
        wrong_format.splice(48..64, [0u8; 16]);
        assert!(matches!(read_header(&wrong_format), Err(OneError::NotASection(_))));
    }

    #[test]
    fn reads_objects_and_their_property_sets() {
        let data = simple();
        let store = read(&data).expect("read");
        assert_eq!(store.spaces.len(), 1);
        let space = store.spaces.first().expect("a space");
        assert_eq!(space.objects.len(), 2);
        assert!(!space.encrypted);
        let root = space.content_root().expect("a content root");
        assert_eq!(root.kind(), 0x0007);
        assert_eq!(root.props().and_then(|p| p.text(0x1C00_1CF3)).as_deref(), Some("My page"));
        let other = space.objects.values().find(|o| o.kind() == 0x000B).expect("the second object");
        assert_eq!(other.props().and_then(|p| p.number(0x1400_1DFF)), Some(2));
        assert_eq!(store.root_space, space.id);
        assert!(store.space(space.id).is_some());
    }

    #[test]
    fn several_object_spaces_are_kept_with_the_root_first() {
        let mut one = One::new();
        let mut a = Build::default();
        a.u32(0x1400_1DFF, 10);
        let mut b = Build::default();
        b.u32(0x1400_1DFF, 20);
        let page = one.space(2, &[Decl { oid: 0x0000_0301, jcid: JCID_PROP_SET | 0x000B, props: b.finish() }], 0x0000_0301);
        let section = one.space(1, &[Decl { oid: 0x0000_0101, jcid: JCID_PROP_SET | 0x0007, props: a.finish() }], 0x0000_0101);
        let data = one.finish(&[(2, page), (1, section)], 1, Vec::new());
        let store = read(&data).expect("read");
        assert_eq!(store.spaces.len(), 2);
        assert_eq!(store.spaces.first().map(|s| s.id), Some(store.root_space), "the root object space comes first");
        assert_eq!(store.spaces.first().and_then(|s| s.content_root()).map(Object::kind), Some(0x0007));
    }

    #[test]
    fn reads_an_inserted_file_and_its_reference() {
        let mut one = One::new();
        let picture = b"\x89PNG\r\n\x1a\nnot really a picture";
        let guid = One::guid(0x5A);
        let node = one.file_data(guid, picture);
        let mut props = Build::default();
        props.u32(0x1400_1DFF, 1);
        let decls = vec![Decl { oid: 0x0000_0101, jcid: JCID_PROP_SET | 0x0011, props: props.finish() }];
        let space = one.space(1, &decls, 0x0000_0101);
        let data = one.finish(&[(1, space)], 1, vec![node]);
        let store = read(&data).expect("read");
        assert_eq!(store.file_data.len(), 1);
        let reference = format!("<ifndf>{guid}");
        assert_eq!(store.file_bytes(&data, &reference), Some(&picture[..]));
        assert_eq!(store.file_bytes(&data, "<file>abc.onebin"), None);
        assert_eq!(store.file_bytes(&data, "<invfdo>"), None);
        assert_eq!(store.file_bytes(&data, "<ifndf>{not-a-guid}"), None);
    }

    #[test]
    fn a_damaged_file_is_an_error_not_a_panic() {
        let good = simple();
        // Truncate at every 64th byte: nothing may panic.
        for cut in (1024..good.len()).step_by(64) {
            let _ = read(good.get(..cut).unwrap_or(&[]));
        }
        // Flip the root list pointer to nonsense.
        let mut bad = good.clone();
        bad.splice(172..180, u64::MAX.to_le_bytes());
        assert!(read(&bad).is_err());
        // Break a fragment's magic number.
        let mut no_magic = good.clone();
        let stp = u64::from_le_bytes(good.get(172..180).and_then(|b| <[u8; 8]>::try_from(b).ok()).unwrap_or([0; 8]));
        no_magic.splice(stp as usize..stp as usize + 8, [0u8; 8]);
        assert!(matches!(read(&no_magic), Err(OneError::Corrupt(_))));
        // A file that claims to be enormous.
        assert!(matches!(read_header(&good).map(|h| h.expected_length), Ok(n) if n > 1024));
    }

    #[test]
    fn junk_of_every_length_never_panics() {
        for len in [0usize, 1, 15, 16, 1023, 1024, 1025, 4096] {
            let _ = read(&vec![0u8; len]);
            let _ = read(&vec![0xFFu8; len]);
            let mut pattern: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let _ = read(&pattern);
            if pattern.len() >= 16 {
                pattern.splice(0..16, FILE_TYPE_ONE.0);
                let _ = read(&pattern);
            }
        }
    }

    #[test]
    fn chunk_references_decode_every_width() {
        // 2-byte compressed pointer, 1-byte compressed size: both multiplied by 8.
        let body = [2u8, 0, 3];
        let (c, len) = read_chunk_ref(&body, 2, 2).expect("ref");
        assert_eq!((c.stp, c.cb, len), (16, 24, 3));
        // A nil pointer stays nil rather than being multiplied.
        let nil = [0xFFu8, 0xFF, 0];
        let (c, _) = read_chunk_ref(&nil, 2, 2).expect("ref");
        assert_eq!(c.stp, u64::MAX);
        assert!(c.is_empty(64));
        assert!(ChunkRef { stp: 0, cb: 0 }.is_empty(64));
        assert!(read_chunk_ref(&[0], 0, 0).is_err());
    }

    #[test]
    fn an_encrypted_object_space_is_reported_not_read() {
        let mut one = One::new();
        let mut props = Build::default();
        props.u32(0x1400_1DFF, 1);
        let space = one.space_with(1, &[Decl { oid: 0x0000_0101, jcid: JCID_PROP_SET, props: props.finish() }], 0x0000_0101, true);
        let data = one.finish(&[(1, space)], 1, Vec::new());
        let store = read(&data).expect("read");
        let space = store.spaces.first().expect("a space");
        assert!(space.encrypted, "the password protected space is flagged");
        // Its property sets are not read: the specification says they must be left alone.
        assert!(space.objects.values().all(|o| o.props().is_some_and(|p| p.properties.is_empty())));
    }

    #[test]
    fn strings_in_storage_are_counted_in_characters() {
        let mut bytes = 3u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[b'o', 0, b'n', 0, b'e', 0]);
        let mut r = Reader::new(&bytes, "test");
        assert_eq!(string_in_storage(&mut r).as_deref(), Ok("one"));
        let mut short = Reader::new(&[9, 0, 0, 0, b'x', 0], "test");
        assert!(string_in_storage(&mut short).is_err());
    }
}
