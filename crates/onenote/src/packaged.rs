//! The alternative packaging: a revision store as OneNote keeps it on OneDrive and SharePoint,
//! and as files downloaded from there are written. [MS-ONESTORE] 2.7 and 2.8, with the data
//! elements of [MS-FSSHTTPB] 2.2.1.
//!
//! The file is a packaging header and one data element package. The package holds a storage
//! index, a storage manifest, one cell manifest per object space, revision manifests, object
//! groups and the bytes of inserted files. This module reads them into the same [`Store`] the
//! desktop reader makes, so everything that reads a section reads either.
//!
//! References work differently here. In the desktop format a property names an object by a
//! compact id that a global table turns into an identity. Here each object carries its own list
//! of the identities it refers to, in the order its property set uses them ([MS-ONESTORE]
//! 2.7.8), and its object spaces and contexts as a list of cell ids. Each reference is rewritten
//! to a compact id the space can resolve exactly ([`ObjectSpace::exact_index`]).

use std::collections::BTreeMap;

use crate::bytes::Reader;
use crate::error::{OneError, Result};
use crate::guid::{CompactId, ExtendedGuid, FILE_TYPE_ONE, FILE_TYPE_TOC, Guid};
use crate::props::{PropertySet, Value, parse_object_prop_set};
use crate::store::{Header, Object, ObjectData, ObjectSpace, Store};

/// `guidCellSchemaId` of a section and of a table of contents. [MS-ONESTORE] 2.8.1.
const SCHEMA_ONE: Guid = Guid::from_parts(0x1F93_7CB4, 0xB26F, 0x445F, [0xB9, 0xF8, 0x17, 0xE2, 0x01, 0x60, 0xE4, 0x61]);
const SCHEMA_TOC: Guid = Guid::from_parts(0xE4DB_FD38, 0xE5C7, 0x408B, [0xA8, 0xA1, 0x0E, 0x7B, 0x42, 0x1E, 0x1F, 0x5F]);
/// The default context, and the GUID of the header cell's object space. [MS-ONESTORE] 2.7.1.
const DEFAULT_CONTEXT: Guid = Guid::from_parts(0x84DE_FAB9, 0xAAA3, 0x4A0D, [0xA3, 0xA8, 0x52, 0x0C, 0x77, 0xAC, 0x70, 0x73]);
const HEADER_CELL: Guid = Guid::from_parts(0x111E_4CF3, 0x7FEF, 0x4087, [0xAF, 0x6A, 0xB9, 0x54, 0x4A, 0xCD, 0x33, 0x4D]);
/// Root extended GUIDs of a revision: `n` is the root role. [MS-ONESTORE] 2.7.4.
const REVISION_ROOT: Guid = Guid::from_parts(0x4A37_17F8, 0x1C14, 0x49E7, [0x95, 0x26, 0x81, 0xD9, 0x42, 0xDE, 0x17, 0x41]);

/// Property ids a file data object carries here. [MS-ONESTORE] 2.7.6.
const FILE_DATA_GUID: u32 = 0x1C00_343E;
const FILE_DATA_EXTENSION: u32 = 0x1C00_3424;
/// `JCID.IsFileData`. [MS-ONESTORE] 2.6.14.
const JCID_FILE_DATA: u32 = 1 << 19;
/// Object partitions. [MS-ONESTORE] 2.7.6.
const PARTITION_DATA: u64 = 1;
const PARTITION_FILE: u64 = 2;
const PARTITION_JCID: u64 = 4;

/// Guards against a damaged file whose base revisions point at each other.
const MAX_REVISION_DEPTH: usize = 4096;
const MAX_ELEMENTS: usize = 2_000_000;

/// Reads a file in the alternative packaging. `allow_toc` asks for a table of contents.
pub(crate) fn read(data: &[u8], allow_toc: bool) -> Result<Store> {
    let mut r = Fs::new(data);
    let _file_type = Guid(r.r.guid()?);
    let file = Guid(r.r.guid()?);
    let legacy = Guid(r.r.guid()?);
    let _format = Guid(r.r.guid()?);
    r.r.skip(4)?; // rgbReserved
    r.start(0x7A)?; // Packaging Start
    let _storage_index = r.exguid()?;
    let schema = Guid(r.r.guid()?);
    let file_type = match schema {
        SCHEMA_ONE => FILE_TYPE_ONE,
        SCHEMA_TOC => FILE_TYPE_TOC,
        other => return Err(OneError::Corrupt(format!("the package has an unknown cell schema {other}"))),
    };
    if file_type == FILE_TYPE_TOC && !allow_toc {
        return Err(OneError::NotesetFile);
    }
    if file_type == FILE_TYPE_ONE && allow_toc {
        return Err(OneError::NotAToc("the package holds a section".into()));
    }
    let package = r.package()?;
    let header = Header::packaged(file_type, file, legacy, data.len() as u64);
    build(data, header, &package)
}

/// Everything the data element package declares, by data element id.
#[derive(Default)]
struct Package {
    /// Cell id to the cell manifest that describes it.
    cells: Vec<(CellId, ExtendedGuid)>,
    /// Revision id to the revision manifest that describes it.
    revisions: BTreeMap<ExtendedGuid, ExtendedGuid>,
    /// Root extended GUID to the cell it names (the storage manifest's root declares).
    roots: Vec<(ExtendedGuid, CellId)>,
    /// Cell manifest id to its current revision.
    cell_manifests: BTreeMap<ExtendedGuid, ExtendedGuid>,
    revision_manifests: BTreeMap<ExtendedGuid, RevisionManifest>,
    object_groups: BTreeMap<ExtendedGuid, Vec<Piece>>,
    /// Object data BLOB id to its bytes, as a range of the file.
    blobs: BTreeMap<ExtendedGuid, (u64, u64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CellId {
    context: ExtendedGuid,
    space: ExtendedGuid,
}

#[derive(Default)]
struct RevisionManifest {
    base: ExtendedGuid,
    /// Root role (the `n` of the root extended GUID) to the root object.
    roots: Vec<(u32, ExtendedGuid)>,
    groups: Vec<ExtendedGuid>,
}

/// One declared part of an object in an object group, with its data.
struct Piece {
    object: ExtendedGuid,
    partition: u64,
    refs: Vec<ExtendedGuid>,
    cells: Vec<CellId>,
    /// The part's bytes as a range of the file (none for excluded data).
    data: Option<(u64, u64)>,
    /// For a part that is an object data BLOB reference: the BLOB.
    blob: Option<ExtendedGuid>,
}

/// A stream object header. [MS-FSSHTTPB] 2.2.1.5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Head {
    Start { kind: u32, compound: bool, len: u64 },
    End { kind: u32 },
}

/// A cursor with the [MS-FSSHTTPB] basic types.
struct Fs<'a> {
    r: Reader<'a>,
    data: &'a [u8],
}

impl<'a> Fs<'a> {
    fn new(data: &'a [u8]) -> Fs<'a> {
        Fs { r: Reader::new(data, "the OneDrive packaging"), data }
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.r.pos()).copied()
    }

    /// Compact unsigned 64-bit integer: the lowest set bit of the first byte says how many
    /// bytes it takes. [MS-FSSHTTPB] 2.2.1.1.
    fn compact(&mut self) -> Result<u64> {
        let first = self.r.u8()?;
        if first == 0 {
            return Ok(0);
        }
        let width = first.trailing_zeros() as usize + 1;
        if width == 8 {
            return self.r.u64();
        }
        let mut value = u64::from(first);
        for i in 1..width {
            value |= u64::from(self.r.u8()?) << (8 * i);
        }
        Ok(value >> width)
    }

    /// Extended GUID. [MS-FSSHTTPB] 2.2.1.7.
    fn exguid(&mut self) -> Result<ExtendedGuid> {
        let first = self.r.u8()?;
        let n = if first == 0 {
            return Ok(ExtendedGuid::ZERO);
        } else if first & 0x07 == 0x04 {
            u32::from(first >> 3)
        } else if first & 0x3F == 0x20 {
            u32::from(self.r.u8()?) << 2 | u32::from(first >> 6)
        } else if first & 0x7F == 0x40 {
            let rest = u32::from(self.r.u16()?);
            rest << 1 | u32::from(first >> 7)
        } else if first == 0x80 {
            self.r.u32()?
        } else {
            return Err(OneError::Corrupt(format!("an extended GUID has the unknown form {first:#04x}")));
        };
        Ok(ExtendedGuid { guid: Guid(self.r.guid()?), n })
    }

    fn exguids(&mut self) -> Result<Vec<ExtendedGuid>> {
        let count = self.count()?;
        (0..count).map(|_| self.exguid()).collect()
    }

    /// Serial number: null, or a GUID and a 64-bit value. [MS-FSSHTTPB] 2.2.1.9.
    fn serial(&mut self) -> Result<()> {
        match self.r.u8()? {
            0 => Ok(()),
            128 => self.r.skip(24),
            other => Err(OneError::Corrupt(format!("a serial number has the unknown form {other}"))),
        }
    }

    fn cell(&mut self) -> Result<CellId> {
        Ok(CellId { context: self.exguid()?, space: self.exguid()? })
    }

    fn cells(&mut self) -> Result<Vec<CellId>> {
        let count = self.count()?;
        (0..count).map(|_| self.cell()).collect()
    }

    /// A count that has to fit in what is left of the file, at a byte or more each.
    fn count(&mut self) -> Result<usize> {
        let n = self.compact()?;
        if n > self.r.remaining() as u64 {
            return Err(OneError::Corrupt(format!("a count of {n} is larger than the file")));
        }
        Ok(n as usize)
    }

    /// A stream object header. [MS-FSSHTTPB] 2.2.1.5.
    fn head(&mut self) -> Result<Head> {
        let first = self.peek().ok_or(OneError::Truncated("a stream object header"))?;
        Ok(match first & 0x03 {
            0 => {
                let v = u32::from(self.r.u16()?);
                Head::Start { kind: (v >> 3) & 0x3F, compound: v & 0x04 != 0, len: u64::from(v >> 9) }
            }
            2 => {
                let v = self.r.u32()?;
                let mut len = u64::from(v >> 17);
                if len == 0x7FFF {
                    len = self.compact()?;
                }
                Head::Start { kind: (v >> 3) & 0x3FFF, compound: v & 0x04 != 0, len }
            }
            1 => Head::End { kind: u32::from(self.r.u8()? >> 2) },
            _ => Head::End { kind: u32::from(self.r.u16()? >> 2) },
        })
    }

    fn peek_head(&mut self) -> Result<Head> {
        let at = self.r.pos();
        let head = self.head();
        self.r = Reader::new(self.data, "the OneDrive packaging");
        self.r.skip(at)?;
        head
    }

    /// Expects the start of a stream object of `kind`, and gives its length.
    fn start(&mut self, kind: u32) -> Result<u64> {
        match self.head()? {
            Head::Start { kind: k, len, .. } if k == kind => Ok(len),
            other => Err(OneError::Corrupt(format!("expected stream object {kind:#x}, found {other:?}"))),
        }
    }

    fn end(&mut self, kind: u32) -> Result<()> {
        match self.head()? {
            Head::End { kind: k } if k == kind => Ok(()),
            other => Err(OneError::Corrupt(format!("expected the end of stream object {kind:#x}, found {other:?}"))),
        }
    }

    /// Skips a single stream object whose header was read: its length covers its data.
    fn skip_single(&mut self, head: Head) -> Result<()> {
        match head {
            Head::Start { compound: false, len, .. } => self.r.skip(usize::try_from(len).map_err(|_| OneError::Truncated("a stream object"))?),
            other => Err(OneError::Corrupt(format!("cannot skip stream object {other:?}"))),
        }
    }

    /// The data element package. [MS-FSSHTTPB] 2.2.1.12.
    fn package(&mut self) -> Result<Package> {
        self.start(0x15)?;
        self.r.skip(1)?; // reserved
        let mut package = Package::default();
        for _ in 0..MAX_ELEMENTS {
            match self.head()? {
                Head::End { kind: 0x15 } => return Ok(package),
                Head::Start { kind: 0x01, .. } => self.element(&mut package)?,
                other => return Err(OneError::Corrupt(format!("the data element package holds {other:?}"))),
            }
        }
        Err(OneError::Corrupt("too many data elements".into()))
    }

    /// One data element, its start header already read.
    fn element(&mut self, package: &mut Package) -> Result<()> {
        let id = self.exguid()?;
        self.serial()?;
        let kind = self.compact()?;
        match kind {
            0x01 => self.storage_index(package)?,
            0x02 => self.storage_manifest(package)?,
            0x03 => {
                self.start(0x0B)?;
                let current = self.exguid()?;
                package.cell_manifests.insert(id, current);
            }
            0x04 => {
                let manifest = self.revision_manifest()?;
                package.revision_manifests.insert(id, manifest);
            }
            0x05 => {
                let pieces = self.object_group()?;
                package.object_groups.insert(id, pieces);
            }
            0x0A => {
                let len = match self.head()? {
                    Head::Start { kind: 0x02, len, .. } => len,
                    other => return Err(OneError::Corrupt(format!("an object data BLOB starts with {other:?}"))),
                };
                let at = self.r.pos() as u64;
                self.r.skip(usize::try_from(len).map_err(|_| OneError::Truncated("an object data BLOB"))?)?;
                package.blobs.insert(id, (at, len));
            }
            // A data element fragment: part of a data element too large for one. The parts
            // are skipped; whatever needed the whole element is missing from the store.
            0x06 => {
                let head = self.head()?;
                self.skip_single(head)?;
            }
            other => return Err(OneError::Corrupt(format!("unknown data element type {other}"))),
        }
        self.end(0x01)
    }

    /// [MS-FSSHTTPB] 2.2.1.12.2.
    fn storage_index(&mut self, package: &mut Package) -> Result<()> {
        loop {
            match self.peek_head()? {
                Head::End { .. } => return Ok(()),
                Head::Start { kind: 0x11, .. } => {
                    self.head()?;
                    self.exguid()?;
                    self.serial()?;
                }
                Head::Start { kind: 0x0E, .. } => {
                    self.head()?;
                    let cell = self.cell()?;
                    let manifest = self.exguid()?;
                    self.serial()?;
                    package.cells.push((cell, manifest));
                }
                Head::Start { kind: 0x0D, .. } => {
                    self.head()?;
                    let revision = self.exguid()?;
                    let manifest = self.exguid()?;
                    self.serial()?;
                    package.revisions.insert(revision, manifest);
                }
                _ => {
                    let head = self.head()?;
                    self.skip_single(head)?;
                }
            }
        }
    }

    /// [MS-FSSHTTPB] 2.2.1.12.3.
    fn storage_manifest(&mut self, package: &mut Package) -> Result<()> {
        self.start(0x0C)?;
        self.r.skip(16)?; // the schema GUID
        loop {
            match self.peek_head()? {
                Head::Start { kind: 0x07, .. } => {
                    self.head()?;
                    let root = self.exguid()?;
                    let cell = self.cell()?;
                    package.roots.push((root, cell));
                }
                _ => return Ok(()),
            }
        }
    }

    /// [MS-FSSHTTPB] 2.2.1.12.5.
    fn revision_manifest(&mut self) -> Result<RevisionManifest> {
        self.start(0x1A)?;
        let _id = self.exguid()?;
        let mut manifest = RevisionManifest { base: self.exguid()?, ..RevisionManifest::default() };
        loop {
            match self.peek_head()? {
                Head::Start { kind: 0x0A, .. } => {
                    self.head()?;
                    let root = self.exguid()?;
                    let object = self.exguid()?;
                    if root.guid == REVISION_ROOT {
                        manifest.roots.push((root.n, object));
                    }
                }
                Head::Start { kind: 0x19, .. } => {
                    self.head()?;
                    manifest.groups.push(self.exguid()?);
                }
                _ => return Ok(manifest),
            }
        }
    }

    /// [MS-FSSHTTPB] 2.2.1.12.6: declarations, then the data in the same order.
    fn object_group(&mut self) -> Result<Vec<Piece>> {
        // An optional data element hash.
        if let Head::Start { kind: 0x06, .. } = self.peek_head()? {
            let head = self.head()?;
            self.skip_single(head)?;
        }
        self.start(0x1D)?;
        let mut declared: Vec<(ExtendedGuid, u64)> = Vec::new();
        loop {
            match self.head()? {
                Head::End { kind: 0x1D } => break,
                Head::Start { kind: 0x18, .. } => {
                    let object = self.exguid()?;
                    let partition = self.compact()?;
                    self.compact()?; // data size
                    self.compact()?; // object references count
                    self.compact()?; // cell references count
                    declared.push((object, partition));
                }
                Head::Start { kind: 0x05, .. } => {
                    let object = self.exguid()?;
                    self.exguid()?; // the BLOB, named again in the reference
                    let partition = self.compact()?;
                    self.compact()?;
                    self.compact()?;
                    declared.push((object, partition));
                }
                other => return Err(OneError::Corrupt(format!("an object group declares {other:?}"))),
            }
        }
        // Optional object metadata declarations.
        if let Head::Start { kind: 0x79, .. } = self.peek_head()? {
            self.head()?;
            loop {
                match self.head()? {
                    Head::End { kind: 0x79 } => break,
                    Head::Start { kind: 0x78, .. } => {
                        self.compact()?;
                    }
                    other => return Err(OneError::Corrupt(format!("object metadata holds {other:?}"))),
                }
            }
        }
        self.start(0x1E)?;
        let mut pieces = Vec::with_capacity(declared.len());
        let mut declarations = declared.into_iter();
        loop {
            let head = self.head()?;
            let (kind, len) = match head {
                Head::End { kind: 0x1E } => break,
                Head::Start { kind, len, .. } => (kind, len),
                other => return Err(OneError::Corrupt(format!("object group data holds {other:?}"))),
            };
            let (object, partition) =
                declarations.next().ok_or_else(|| OneError::Corrupt("an object group has more data than declarations".into()))?;
            let refs = self.exguids()?;
            let cells = self.cells()?;
            let (data, blob) = match kind {
                0x16 => {
                    let size = self.compact()?;
                    let at = self.r.pos() as u64;
                    self.r.skip(usize::try_from(size).map_err(|_| OneError::Truncated("object data"))?)?;
                    (Some((at, size)), None)
                }
                0x03 => {
                    self.compact()?;
                    (None, None)
                }
                0x1C => (None, Some(self.exguid()?)),
                other => return Err(OneError::Corrupt(format!("object group data of unknown kind {other:#x} ({len} bytes)"))),
            };
            pieces.push(Piece { object, partition, refs, cells, data, blob });
        }
        Ok(pieces)
    }
}

/// Turns the package into object spaces.
fn build(data: &[u8], header: Header, package: &Package) -> Result<Store> {
    let data_root = package
        .roots
        .iter()
        .find(|(root, _)| root.guid == DEFAULT_CONTEXT && root.n == 2)
        .map(|(_, cell)| cell.space)
        .ok_or_else(|| OneError::Corrupt("the package names no root object space".into()))?;
    let default_context = ExtendedGuid { guid: DEFAULT_CONTEXT, n: 1 };
    let mut spaces: Vec<ObjectSpace> = Vec::new();
    let mut file_data = BTreeMap::new();
    for (cell, manifest) in &package.cells {
        // The header cell is the desktop header again; other contexts are older versions.
        if cell.space.guid == HEADER_CELL || cell.context != default_context {
            continue;
        }
        let Some(revision) = package.cell_manifests.get(manifest) else { continue };
        let mut space = ObjectSpace::default();
        space.id = cell.space;
        // The revision and its bases, newest first.
        let mut chain = Vec::new();
        let mut at = *revision;
        while !at.is_zero() && chain.len() < MAX_REVISION_DEPTH {
            let Some(manifest) = package.revisions.get(&at).and_then(|id| package.revision_manifests.get(id)) else { break };
            chain.push(manifest);
            at = manifest.base;
        }
        let mut roots = BTreeMap::new();
        for manifest in &chain {
            for (role, object) in &manifest.roots {
                roots.entry(*role).or_insert(*object);
            }
        }
        space.encrypted = roots.contains_key(&3);
        space.roots = roots;
        // Oldest first, so a later declaration of an object replaces an earlier one.
        for manifest in chain.iter().rev() {
            for group in &manifest.groups {
                if let Some(pieces) = package.object_groups.get(group) {
                    objects(data, pieces, package, &mut space, &mut file_data);
                }
            }
        }
        spaces.push(space);
    }
    if let Some(i) = spaces.iter().position(|s| s.id == data_root) {
        spaces.swap(0, i);
    }
    Ok(Store { header, spaces, root_space: data_root, file_data })
}

/// The parts one object is declared in: its type, its property set and, for an inserted
/// file, the BLOB that holds the file's bytes.
#[derive(Default)]
struct Parts<'p> {
    jcid: Option<u32>,
    props: Option<&'p Piece>,
    blob: Option<ExtendedGuid>,
}

/// The objects of one object group, into the space.
fn objects(data: &[u8], pieces: &[Piece], package: &Package, space: &mut ObjectSpace, file_data: &mut BTreeMap<Guid, (u64, u64)>) {
    let mut by_object: Vec<(ExtendedGuid, Parts<'_>)> = Vec::new();
    for piece in pieces {
        let at = match by_object.iter().position(|(oid, _)| *oid == piece.object) {
            Some(i) => i,
            None => {
                by_object.push((piece.object, Parts::default()));
                by_object.len() - 1
            }
        };
        let Some((_, parts)) = by_object.get_mut(at) else { continue };
        match piece.partition {
            PARTITION_JCID => {
                let bytes = piece.data.and_then(|(start, len)| bytes_at(data, start, len));
                parts.jcid = bytes.map(|b| crate::bytes::le(b) as u32);
            }
            PARTITION_DATA => parts.props = Some(piece),
            PARTITION_FILE => parts.blob = piece.blob,
            _ => {}
        }
    }
    for (oid, parts) in by_object {
        // The type is static metadata, sent with an object's first version only; a later
        // revision sends the new data alone. An object never given a type is not one of the
        // section's (the header cell's root is such an object).
        let Some(jcid) = parts.jcid.or_else(|| space.objects.get(&oid).map(|o| o.jcid)) else { continue };
        let Some(piece) = parts.props else { continue };
        let read =
            piece.data.and_then(|(start, len)| bytes_at(data, start, len)).ok_or(OneError::Truncated("object data")).and_then(parse_object_prop_set);
        let Ok(mut props) = read else {
            space.damaged += 1;
            continue;
        };
        rewrite_references(&mut props, &piece.refs, &piece.cells, space);
        let object_data = if jcid & JCID_FILE_DATA != 0 {
            let extension = props.text(FILE_DATA_EXTENSION).unwrap_or_default();
            let guid = props.bytes(FILE_DATA_GUID).and_then(|b| <[u8; 16]>::try_from(b).ok()).map(Guid);
            let range = parts.blob.and_then(|b| package.blobs.get(&b)).copied();
            match (guid, range) {
                (Some(guid), Some(range)) => {
                    file_data.insert(guid, range);
                    ObjectData::File { reference: format!("<ifndf>{guid}"), extension }
                }
                _ => ObjectData::File { reference: "<invfdo>".to_string(), extension },
            }
        } else {
            ObjectData::Props(props)
        };
        space.objects.insert(oid, Object { oid, jcid, data: object_data });
    }
}

fn bytes_at(data: &[u8], start: u64, len: u64) -> Option<&[u8]> {
    let start = usize::try_from(start).ok()?;
    let end = start.checked_add(usize::try_from(len).ok()?)?;
    data.get(start..end)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Refs {
    Objects,
    Spaces,
    Contexts,
}

/// Rewrites the compact ids of a property set to ones the space resolves exactly. The object
/// references take the object's identities in order, and the object space and context
/// references take its cells in order, spaces first. [MS-ONESTORE] 2.7.8.
fn rewrite_references(props: &mut PropertySet, refs: &[ExtendedGuid], cells: &[CellId], space: &mut ObjectSpace) {
    let mut ids = Vec::new();
    collect(props, Refs::Objects, &mut ids);
    // The identities may leave the null references out.
    let skip_null = refs.len() != ids.len() && refs.len() == ids.iter().filter(|id| !id.is_zero()).count();
    let mut objects = refs.iter().copied();
    rewrite(props, Refs::Objects, skip_null, &mut objects, space);
    // Cells serve both lists: the object space references take theirs first.
    let mut spaces = Vec::new();
    collect(props, Refs::Spaces, &mut spaces);
    let split = spaces.len().min(cells.len());
    let mut space_ids = cells.iter().take(split).map(|c| c.space);
    rewrite(props, Refs::Spaces, false, &mut space_ids, space);
    let mut contexts = cells.iter().skip(split).map(|c| c.context);
    rewrite(props, Refs::Contexts, false, &mut contexts, space);
}

fn collect(props: &PropertySet, which: Refs, out: &mut Vec<CompactId>) {
    for property in &props.properties {
        match (&property.value, which) {
            (Value::ObjectIds(v), Refs::Objects) | (Value::ObjectSpaceIds(v), Refs::Spaces) | (Value::ContextIds(v), Refs::Contexts) => {
                out.extend(v.iter().copied())
            }
            (Value::Sets(sets), _) => sets.iter().for_each(|s| collect(s, which, out)),
            _ => {}
        }
    }
}

fn rewrite(props: &mut PropertySet, which: Refs, skip_null: bool, with: &mut dyn Iterator<Item = ExtendedGuid>, space: &mut ObjectSpace) {
    for property in &mut props.properties {
        match (&mut property.value, which) {
            (Value::ObjectIds(v), Refs::Objects) | (Value::ObjectSpaceIds(v), Refs::Spaces) | (Value::ContextIds(v), Refs::Contexts) => {
                for id in v.iter_mut() {
                    if skip_null && id.is_zero() {
                        continue;
                    }
                    *id = match with.next() {
                        Some(e) if e.is_zero() => CompactId::default(),
                        Some(e) => CompactId { n: 0, index: space.exact_index(e) },
                        // More references than identities: one that finds nothing.
                        None => CompactId { n: 0, index: u32::MAX },
                    };
                }
            }
            (Value::Sets(sets), _) => sets.iter_mut().for_each(|s| rewrite(s, which, skip_null, with, space)),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs(bytes: &[u8]) -> Fs<'_> {
        Fs::new(bytes)
    }

    /// [MS-FSSHTTPB] 2.2.1.1: the lowest set bit of the first byte gives the width.
    #[test]
    fn compact_integers_take_every_width_the_spec_gives() {
        assert_eq!(fs(&[0x00]).compact().ok(), Some(0));
        assert_eq!(fs(&[0x01 | (0x7F << 1)]).compact().ok(), Some(0x7F));
        // 14 bits: type 2 in the low two bits.
        let v = (0x3FFFu16 << 2) | 0b10;
        assert_eq!(fs(&v.to_le_bytes()).compact().ok(), Some(0x3FFF));
        // 21 bits: type 4 in the low three bits.
        let v = (0x1F_FFFFu32 << 3) | 0b100;
        assert_eq!(fs(&v.to_le_bytes()[..3]).compact().ok(), Some(0x1F_FFFF));
        // 28 bits: type 8.
        let v = (0x0ABC_DEF1u32 << 4) | 0b1000;
        assert_eq!(fs(&v.to_le_bytes()).compact().ok(), Some(0x0ABC_DEF1));
        // 49 bits: type 64 in the low seven bits.
        let v = (0x1_2345_6789_ABCDu64 << 7) | 0x40;
        assert_eq!(fs(&v.to_le_bytes()[..7]).compact().ok(), Some(0x1_2345_6789_ABCD));
        // 64 bits: a 0x80 byte, then the value.
        let mut b = vec![0x80];
        b.extend_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(fs(&b).compact().ok(), Some(u64::MAX));
        assert!(fs(&[0x02]).compact().is_err(), "cut short");
    }

    /// [MS-FSSHTTPB] 2.2.1.7: null, then 5, 10, 17 and 32 bit values before the GUID.
    #[test]
    fn extended_guids_take_every_form_the_spec_gives() {
        let guid = [7u8; 16];
        assert_eq!(fs(&[0]).exguid().ok(), Some(ExtendedGuid::ZERO));
        let with = |head: &[u8]| {
            let mut b = head.to_vec();
            b.extend_from_slice(&guid);
            fs(&b).exguid().ok()
        };
        assert_eq!(with(&[(0x1F << 3) | 0x04]), Some(ExtendedGuid { guid: Guid(guid), n: 0x1F }));
        let v = (0x3FFu16 << 6) | 0x20;
        assert_eq!(with(&v.to_le_bytes()), Some(ExtendedGuid { guid: Guid(guid), n: 0x3FF }));
        let v = (0x1_FFFFu32 << 7) | 0x40;
        assert_eq!(with(&v.to_le_bytes()[..3]), Some(ExtendedGuid { guid: Guid(guid), n: 0x1_FFFF }));
        let mut b = vec![0x80];
        b.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        assert_eq!(with(&b), Some(ExtendedGuid { guid: Guid(guid), n: 0xDEAD_BEEF }));
        assert!(fs(&[0x03]).exguid().is_err(), "no such form");
    }

    /// [MS-FSSHTTPB] 2.2.1.5: 16 and 32 bit starts, 8 and 16 bit ends.
    #[test]
    fn stream_object_headers_read_their_type_compound_bit_and_length() {
        // 16-bit start: type 0x01, compound, length 5.
        let v: u16 = (5 << 9) | (0x01 << 3) | 0x04;
        assert_eq!(fs(&v.to_le_bytes()).head().ok(), Some(Head::Start { kind: 0x01, compound: true, len: 5 }));
        // 32-bit start: type 0x7A, compound, length 300.
        let v: u32 = (300 << 17) | (0x7A << 3) | 0x04 | 0x02;
        assert_eq!(fs(&v.to_le_bytes()).head().ok(), Some(Head::Start { kind: 0x7A, compound: true, len: 300 }));
        // 32-bit start with the large length that follows it.
        let v: u32 = (0x7FFF << 17) | (0x02 << 3) | 0x02;
        let mut b = v.to_le_bytes().to_vec();
        b.extend_from_slice(&[(1 << 1) | 1]);
        assert_eq!(fs(&b).head().ok(), Some(Head::Start { kind: 0x02, compound: false, len: 1 }));
        // 8-bit end of a data element, and the packaging's 16-bit end, 0x1EB.
        assert_eq!(fs(&[(0x01 << 2) | 0x01]).head().ok(), Some(Head::End { kind: 0x01 }));
        assert_eq!(fs(&0x01EBu16.to_le_bytes()).head().ok(), Some(Head::End { kind: 0x7A }));
    }

    /// A file that claims the packaging but holds nothing after the header is refused with
    /// an error, never a panic.
    #[test]
    fn a_cut_short_package_is_an_error() {
        let mut data = vec![0u8; 64];
        data[48..64].copy_from_slice(&crate::guid::FILE_FORMAT_PACKAGED.0);
        assert!(read(&data, false).is_err());
        data.extend_from_slice(&[0, 0, 0, 0]);
        assert!(read(&data, false).is_err());
    }
}
