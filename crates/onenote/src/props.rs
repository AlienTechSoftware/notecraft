//! Property sets: the data of every object in a `.one` file. [MS-ONESTORE] 2.6.1 to 2.6.9.
//!
//! A property set is a count, an array of property ids, and one stream of data that the ids
//! consume in order. Ids that point at other objects take their values from three separate
//! streams of compact ids that sit in front of the property set. What each id *means* is in
//! [MS-ONE]; this module only reads the shape.

use crate::bytes::Reader;
use crate::error::{OneError, Result};
use crate::guid::CompactId;

/// Nested property sets are legal; a damaged file must not be able to recurse forever.
const MAX_DEPTH: usize = 24;
/// No property set in a note has thousands of properties. A larger count means damage.
const MAX_PROPERTIES: usize = 4096;

/// One property: its id (meaning in [MS-ONE] 2.1.12) and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub id: u32,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// The property is present and carries nothing but itself.
    None,
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    /// Length-prefixed bytes: text, arrays of numbers, and everything else variable sized.
    Bytes(Vec<u8>),
    /// References to other objects in the same object space.
    ObjectIds(Vec<CompactId>),
    /// References to other object spaces (pages, for example).
    ObjectSpaceIds(Vec<CompactId>),
    /// References to contexts (version history).
    ContextIds(Vec<CompactId>),
    /// A nested property set, or an array of them.
    Sets(Vec<PropertySet>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropertySet {
    pub properties: Vec<Property>,
}

impl PropertySet {
    pub fn get(&self, id: u32) -> Option<&Value> {
        self.properties.iter().find(|p| p.id == id).map(|p| &p.value)
    }

    pub fn has(&self, id: u32) -> bool {
        self.get(id).is_some()
    }

    /// A whole number property, whatever width it was stored in.
    pub fn number(&self, id: u32) -> Option<u64> {
        match self.get(id)? {
            Value::U8(v) => Some(u64::from(*v)),
            Value::U16(v) => Some(u64::from(*v)),
            Value::U32(v) => Some(u64::from(*v)),
            Value::U64(v) => Some(*v),
            Value::Bool(v) => Some(u64::from(*v)),
            _ => None,
        }
    }

    pub fn bool(&self, id: u32) -> Option<bool> {
        match self.get(id)? {
            Value::Bool(v) => Some(*v),
            other => Some(matches!(other, Value::U8(1) | Value::U16(1) | Value::U32(1))),
        }
    }

    pub fn bytes(&self, id: u32) -> Option<&[u8]> {
        match self.get(id)? {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// A text property: [MS-ONE] stores these as UTF-16, sometimes NUL terminated.
    pub fn text(&self, id: u32) -> Option<String> {
        self.bytes(id).map(crate::bytes::utf16)
    }

    pub fn object_ids(&self, id: u32) -> &[CompactId] {
        match self.get(id) {
            Some(Value::ObjectIds(v)) => v,
            _ => &[],
        }
    }

    pub fn object_space_ids(&self, id: u32) -> &[CompactId] {
        match self.get(id) {
            Some(Value::ObjectSpaceIds(v)) => v,
            _ => &[],
        }
    }

    pub fn sets(&self, id: u32) -> &[PropertySet] {
        match self.get(id) {
            Some(Value::Sets(v)) => v,
            _ => &[],
        }
    }
}

/// The streams of compact ids in front of a property set, read in order as properties ask.
struct Streams {
    oids: std::vec::IntoIter<CompactId>,
    osids: std::vec::IntoIter<CompactId>,
    contexts: std::vec::IntoIter<CompactId>,
}

impl Streams {
    fn take(iter: &mut std::vec::IntoIter<CompactId>, n: usize, what: &'static str) -> Result<Vec<CompactId>> {
        let mut out = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            out.push(iter.next().ok_or(OneError::Truncated(what))?);
        }
        Ok(out)
    }
}

/// Reads an ObjectSpaceObjectPropSet: the three id streams, then the property set itself.
/// [MS-ONESTORE] 2.6.1.
pub fn parse_object_prop_set(data: &[u8]) -> Result<PropertySet> {
    let mut r = Reader::new(data, "a property set");
    let (oid_count, extended, no_osid) = stream_header(&mut r)?;
    let oids = compact_ids(&mut r, oid_count)?;
    let (mut osids, mut contexts) = (Vec::new(), Vec::new());
    let mut want_contexts = false;
    if !no_osid {
        let (n, ext, _) = stream_header(&mut r)?;
        osids = compact_ids(&mut r, n)?;
        want_contexts = ext;
    } else if extended {
        // The first stream says more streams follow, but also says the OSID stream is absent.
        // The specification does not allow both; treat it as damage rather than guessing.
        return Err(OneError::Corrupt("a property set claims extra id streams but no object space stream".into()));
    }
    if want_contexts {
        let (n, _, _) = stream_header(&mut r)?;
        contexts = compact_ids(&mut r, n)?;
    }
    let mut streams = Streams { oids: oids.into_iter(), osids: osids.into_iter(), contexts: contexts.into_iter() };
    parse_set(&mut r, &mut streams, 0)
}

/// Count, ExtendedStreamsPresent, OsidStreamNotPresent. [MS-ONESTORE] 2.6.5.
fn stream_header(r: &mut Reader<'_>) -> Result<(usize, bool, bool)> {
    let v = r.u32()?;
    let count = (v & 0x00FF_FFFF) as usize;
    if count * 4 > r.remaining() {
        return Err(OneError::Truncated("a stream of object ids"));
    }
    Ok((count, v & (1 << 30) != 0, v & (1 << 31) != 0))
}

fn compact_ids(r: &mut Reader<'_>, n: usize) -> Result<Vec<CompactId>> {
    let mut out = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        out.push(CompactId::from_u32(r.u32()?));
    }
    Ok(out)
}

/// A PropertySet: the id array, then the data each id claims. [MS-ONESTORE] 2.6.7.
fn parse_set(r: &mut Reader<'_>, s: &mut Streams, depth: usize) -> Result<PropertySet> {
    if depth > MAX_DEPTH {
        return Err(OneError::Corrupt("property sets nested too deeply".into()));
    }
    let count = usize::from(r.u16()?);
    if count > MAX_PROPERTIES || count * 4 > r.remaining() {
        return Err(OneError::Corrupt(format!("a property set claims {count} properties")));
    }
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        ids.push(r.u32()?);
    }
    let mut properties = Vec::with_capacity(count);
    for raw in ids {
        // The id keeps its type bits, so it equals the PropertyID value in the [MS-ONE] tables
        // (`CachedTitleString 0x1C001CF3`). Only the boolean value bit is masked off.
        let id = raw & 0x7FFF_FFFF;
        let kind = (raw >> 26) & 0x1F;
        let flag = raw & 0x8000_0000 != 0;
        let value = parse_value(r, s, kind, flag, depth)?;
        properties.push(Property { id, value });
    }
    Ok(PropertySet { properties })
}

fn parse_value(r: &mut Reader<'_>, s: &mut Streams, kind: u32, flag: bool, depth: usize) -> Result<Value> {
    Ok(match kind {
        0x1 => Value::None,
        0x2 => Value::Bool(flag),
        0x3 => Value::U8(r.u8()?),
        0x4 => Value::U16(r.u16()?),
        0x5 => Value::U32(r.u32()?),
        0x6 => Value::U64(r.u64()?),
        0x7 => {
            let cb = r.u32()?;
            if cb >= 0x4000_0000 {
                return Err(OneError::Corrupt(format!("a property claims {cb} bytes of data")));
            }
            Value::Bytes(r.take(cb as usize)?.to_vec())
        }
        0x8 => Value::ObjectIds(Streams::take(&mut s.oids, 1, "an object reference")?),
        0x9 => {
            let n = count_from_data(r)?;
            Value::ObjectIds(Streams::take(&mut s.oids, n, "an array of object references")?)
        }
        0xA => Value::ObjectSpaceIds(Streams::take(&mut s.osids, 1, "an object space reference")?),
        0xB => {
            let n = count_from_data(r)?;
            Value::ObjectSpaceIds(Streams::take(&mut s.osids, n, "an array of object space references")?)
        }
        0xC => Value::ContextIds(Streams::take(&mut s.contexts, 1, "a context reference")?),
        0xD => {
            let n = count_from_data(r)?;
            Value::ContextIds(Streams::take(&mut s.contexts, n, "an array of context references")?)
        }
        // prtArrayOfPropertyValues: a count, the shared property id, then that many sets.
        0x10 => {
            let n = r.u32()? as usize;
            if n == 0 {
                Value::Sets(Vec::new())
            } else {
                if n * 2 > r.remaining() {
                    return Err(OneError::Corrupt(format!("an array claims {n} property sets")));
                }
                let _shared_id = r.u32()?;
                let mut sets = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    sets.push(parse_set(r, s, depth + 1)?);
                }
                Value::Sets(sets)
            }
        }
        0x11 => Value::Sets(vec![parse_set(r, s, depth + 1)?]),
        other => return Err(OneError::Corrupt(format!("unknown property type {other:#x}"))),
    })
}

/// The 4-byte element count that array properties keep in the data stream.
fn count_from_data(r: &mut Reader<'_>) -> Result<usize> {
    let n = r.u32()? as usize;
    if n > 1_000_000 {
        return Err(OneError::Corrupt(format!("an array claims {n} references")));
    }
    Ok(n)
}

#[cfg(any(test, feature = "testing"))]
#[allow(dead_code)]
pub mod tests_support {
    //! Property sets built by hand, for the tests of this crate. Every method takes the
    //! documented PropertyID (type bits included), so a test reads like the specification.

    #[derive(Default)]
    pub struct Build {
        prids: Vec<u32>,
        data: Vec<u8>,
        oids: Vec<u32>,
        osids: Vec<u32>,
        contexts: Vec<u32>,
    }

    /// The type an id carries, as [MS-ONESTORE] 2.6.6 packs it.
    pub fn kind_of(prid: u32) -> u32 {
        (prid >> 26) & 0x1F
    }

    impl Build {
        fn add(&mut self, prid: u32, want: u32, flag: bool, data: &[u8]) -> &mut Self {
            assert_eq!(kind_of(prid), want, "{prid:#010X} is not a type {want:#x} property");
            self.prids.push(prid | (u32::from(flag) << 31));
            self.data.extend_from_slice(data);
            self
        }

        pub fn none(&mut self, prid: u32) -> &mut Self {
            self.add(prid, 0x1, false, &[])
        }

        pub fn bool(&mut self, prid: u32, v: bool) -> &mut Self {
            self.add(prid, 0x2, v, &[])
        }

        pub fn u8(&mut self, prid: u32, v: u8) -> &mut Self {
            self.add(prid, 0x3, false, &[v])
        }

        pub fn u16(&mut self, prid: u32, v: u16) -> &mut Self {
            self.add(prid, 0x4, false, &v.to_le_bytes())
        }

        pub fn u32(&mut self, prid: u32, v: u32) -> &mut Self {
            self.add(prid, 0x5, false, &v.to_le_bytes())
        }

        pub fn u64(&mut self, prid: u32, v: u64) -> &mut Self {
            self.add(prid, 0x6, false, &v.to_le_bytes())
        }

        /// A length-prefixed value (type 7).
        pub fn data(&mut self, prid: u32, bytes: &[u8]) -> &mut Self {
            let mut d = (bytes.len() as u32).to_le_bytes().to_vec();
            d.extend_from_slice(bytes);
            self.add(prid, 0x7, false, &d)
        }

        /// UTF-16 text, which is how [MS-ONE] stores every string.
        pub fn text(&mut self, prid: u32, s: &str) -> &mut Self {
            let utf16: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
            self.data(prid, &utf16)
        }

        pub fn object(&mut self, prid: u32, oid: u32) -> &mut Self {
            self.oids.push(oid);
            self.add(prid, 0x8, false, &[])
        }

        pub fn objects(&mut self, prid: u32, oids: &[u32]) -> &mut Self {
            self.oids.extend_from_slice(oids);
            self.add(prid, 0x9, false, &(oids.len() as u32).to_le_bytes())
        }

        pub fn spaces(&mut self, prid: u32, osids: &[u32]) -> &mut Self {
            self.osids.extend_from_slice(osids);
            self.add(prid, 0xB, false, &(osids.len() as u32).to_le_bytes())
        }

        pub fn contexts(&mut self, prid: u32, ids: &[u32]) -> &mut Self {
            self.contexts.extend_from_slice(ids);
            self.add(prid, 0xD, false, &(ids.len() as u32).to_le_bytes())
        }

        /// An array of property sets (type 0x10), each built by the closure.
        pub fn sets(&mut self, prid: u32, n: u32, each: impl Fn(&mut Build)) -> &mut Self {
            let mut body = Vec::new();
            let mut inner = Build::default();
            for _ in 0..n {
                let mut one = Build::default();
                each(&mut one);
                body.extend_from_slice(&one.set_bytes());
                inner.oids.extend_from_slice(&one.oids);
                inner.osids.extend_from_slice(&one.osids);
                inner.contexts.extend_from_slice(&one.contexts);
            }
            let mut d = n.to_le_bytes().to_vec();
            if n > 0 {
                // The shared id is present only when the array has elements.
                d.extend_from_slice(&(0x11u32 << 26).to_le_bytes());
                d.extend_from_slice(&body);
            }
            self.oids.extend_from_slice(&inner.oids);
            self.osids.extend_from_slice(&inner.osids);
            self.contexts.extend_from_slice(&inner.contexts);
            self.add(prid, 0x10, false, &d)
        }

        /// Just the PropertySet part: count, ids, data.
        fn set_bytes(&self) -> Vec<u8> {
            let mut out = (self.prids.len() as u16).to_le_bytes().to_vec();
            for p in &self.prids {
                out.extend_from_slice(&p.to_le_bytes());
            }
            out.extend_from_slice(&self.data);
            out
        }

        /// The whole ObjectSpaceObjectPropSet: the id streams, then the property set.
        pub fn finish(&self) -> Vec<u8> {
            let mut out = Vec::new();
            let contexts = !self.contexts.is_empty();
            let osids = !self.osids.is_empty() || contexts;
            let mut header = self.oids.len() as u32;
            if !osids {
                header |= 1 << 31; // OsidStreamNotPresent
            }
            out.extend_from_slice(&header.to_le_bytes());
            for o in &self.oids {
                out.extend_from_slice(&o.to_le_bytes());
            }
            if osids {
                let mut h = self.osids.len() as u32;
                if contexts {
                    h |= 1 << 30; // ExtendedStreamsPresent
                }
                out.extend_from_slice(&h.to_le_bytes());
                for o in &self.osids {
                    out.extend_from_slice(&o.to_le_bytes());
                }
                if contexts {
                    out.extend_from_slice(&(self.contexts.len() as u32).to_le_bytes());
                    for c in &self.contexts {
                        out.extend_from_slice(&c.to_le_bytes());
                    }
                }
            }
            out.extend_from_slice(&self.set_bytes());
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::{Build, kind_of};
    use super::*;

    // PropertyIDs from the [MS-ONE] 2.1.12 table, used here as real examples.
    const CACHED_TITLE_STRING: u32 = 0x1C00_1CF3;
    const RICH_EDIT_TEXT_UNICODE: u32 = 0x1C00_1C22;
    const PAGE_LEVEL: u32 = 0x1400_1DFF;
    const BOLD: u32 = 0x0800_1C04;
    const ITALIC: u32 = 0x0800_1C05;
    const CHARSET: u32 = 0x0C00_1D01;
    const FONT_SIZE: u32 = 0x1000_1C0B;
    const TOPOLOGY_CREATION_TIME: u32 = 0x1800_1C65;
    const PICTURE_CONTAINER: u32 = 0x2000_1C3F;
    const ELEMENT_CHILD_NODES: u32 = 0x2400_1C20;
    const CHILD_GRAPH_SPACE_ELEMENT_NODES: u32 = 0x2C00_1D63;
    const VERSION_HISTORY_CONTEXT_NODES: u32 = 0x3400_347B;
    const TEXT_RUN_DATA: u32 = 0x4000_3499;
    const LAYOUT_TIGHT_LAYOUT: u32 = 0x0800_1C00;

    /// The documented ids carry their own type, so a reader needs no table to parse them.
    #[test]
    fn documented_ids_decode_to_the_documented_types() {
        for (id, want) in [
            (LAYOUT_TIGHT_LAYOUT, 0x2),
            (BOLD, 0x2),
            (CHARSET, 0x3),
            (FONT_SIZE, 0x4),
            (PAGE_LEVEL, 0x5),
            (TOPOLOGY_CREATION_TIME, 0x6),
            (CACHED_TITLE_STRING, 0x7),
            (PICTURE_CONTAINER, 0x8),
            (ELEMENT_CHILD_NODES, 0x9),
            (CHILD_GRAPH_SPACE_ELEMENT_NODES, 0xB),
            (VERSION_HISTORY_CONTEXT_NODES, 0xD),
            (TEXT_RUN_DATA, 0x10),
        ] {
            assert_eq!(kind_of(id), want, "{id:#010X}");
        }
    }

    #[test]
    fn reads_every_property_type() {
        let mut b = Build::default();
        b.none(0x0400_0001)
            .bool(BOLD, true)
            .bool(ITALIC, false)
            .u8(CHARSET, 2)
            .u16(FONT_SIZE, 11)
            .u32(PAGE_LEVEL, 3)
            .u64(TOPOLOGY_CREATION_TIME, 0x1234_5678_9ABC)
            .text(CACHED_TITLE_STRING, "My page")
            .object(PICTURE_CONTAINER, 0x0000_0101)
            .objects(ELEMENT_CHILD_NODES, &[0x0000_0201, 0x0000_0301]);
        let set = parse_object_prop_set(&b.finish()).expect("parse");
        assert_eq!(set.get(0x0400_0001), Some(&Value::None));
        assert_eq!(set.bool(BOLD), Some(true));
        assert_eq!(set.bool(ITALIC), Some(false));
        assert_eq!(set.number(CHARSET), Some(2));
        assert_eq!(set.number(FONT_SIZE), Some(11));
        assert_eq!(set.number(PAGE_LEVEL), Some(3));
        assert_eq!(set.number(TOPOLOGY_CREATION_TIME), Some(0x1234_5678_9ABC));
        assert_eq!(set.text(CACHED_TITLE_STRING).as_deref(), Some("My page"));
        assert_eq!(set.object_ids(PICTURE_CONTAINER).first().map(|c| c.index), Some(1));
        assert_eq!(set.object_ids(ELEMENT_CHILD_NODES).len(), 2);
        assert_eq!(set.object_ids(ELEMENT_CHILD_NODES).get(1).map(|c| c.index), Some(3));
        assert_eq!(set.object_ids(PAGE_LEVEL).len(), 0, "a property of another type is not a reference");
        assert!(!set.has(RICH_EDIT_TEXT_UNICODE));
        assert_eq!(set.text(RICH_EDIT_TEXT_UNICODE), None);
    }

    #[test]
    fn reads_object_space_and_context_streams() {
        let mut b = Build::default();
        b.spaces(CHILD_GRAPH_SPACE_ELEMENT_NODES, &[0x0000_1001, 0x0000_1101])
            .contexts(VERSION_HISTORY_CONTEXT_NODES, &[0x0000_2001])
            .u32(PAGE_LEVEL, 1);
        let set = parse_object_prop_set(&b.finish()).expect("parse");
        let spaces = set.object_space_ids(CHILD_GRAPH_SPACE_ELEMENT_NODES);
        assert_eq!(spaces.len(), 2);
        assert_eq!(spaces.first().map(|c| c.index), Some(0x10));
        assert!(matches!(set.get(VERSION_HISTORY_CONTEXT_NODES), Some(Value::ContextIds(v)) if v.len() == 1));
        assert_eq!(set.number(PAGE_LEVEL), Some(1), "properties after the reference streams still line up");
    }

    #[test]
    fn reads_nested_arrays_of_property_sets() {
        let mut b = Build::default();
        b.sets(TEXT_RUN_DATA, 3, |run| {
            run.bool(BOLD, true);
            run.text(RICH_EDIT_TEXT_UNICODE, "run");
        });
        let set = parse_object_prop_set(&b.finish()).expect("parse");
        let runs = set.sets(TEXT_RUN_DATA);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs.first().and_then(|s| s.bool(BOLD)), Some(true));
        assert_eq!(runs.first().and_then(|s| s.text(RICH_EDIT_TEXT_UNICODE)).as_deref(), Some("run"));
        assert_eq!(set.sets(BOLD).len(), 0);
    }

    /// References inside nested sets come from the same streams, in order.
    #[test]
    fn nested_sets_take_their_references_in_order() {
        let mut b = Build::default();
        b.object(PICTURE_CONTAINER, 0x0000_0101).sets(TEXT_RUN_DATA, 2, |run| {
            run.object(PICTURE_CONTAINER, 0x0000_0201);
        });
        let set = parse_object_prop_set(&b.finish()).expect("parse");
        assert_eq!(set.object_ids(PICTURE_CONTAINER).first().map(|c| c.index), Some(1));
        let runs = set.sets(TEXT_RUN_DATA);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs.first().map(|s| s.object_ids(PICTURE_CONTAINER).len()), Some(1));
        assert_eq!(runs.first().and_then(|s| s.object_ids(PICTURE_CONTAINER).first()).map(|c| c.index), Some(2));
    }

    #[test]
    fn an_empty_array_of_sets_has_no_shared_id() {
        let mut b = Build::default();
        b.sets(TEXT_RUN_DATA, 0, |_| {}).u32(PAGE_LEVEL, 9);
        let set = parse_object_prop_set(&b.finish()).expect("parse");
        assert_eq!(set.sets(TEXT_RUN_DATA).len(), 0);
        assert_eq!(set.number(PAGE_LEVEL), Some(9));
    }

    #[test]
    fn damaged_property_sets_are_errors_not_panics() {
        assert!(parse_object_prop_set(&[]).is_err());
        assert!(parse_object_prop_set(&[0; 3]).is_err());
        let no_streams = 0x8000_0000u32;
        // A property count far larger than the data.
        let mut junk = no_streams.to_le_bytes().to_vec();
        junk.extend_from_slice(&0xFFFFu16.to_le_bytes());
        assert!(parse_object_prop_set(&junk).is_err());
        // An unknown property type.
        let mut bad = no_streams.to_le_bytes().to_vec();
        bad.extend_from_slice(&1u16.to_le_bytes());
        bad.extend_from_slice(&(0x1Fu32 << 26).to_le_bytes());
        assert!(matches!(parse_object_prop_set(&bad), Err(OneError::Corrupt(_))));
        // A length that runs off the end.
        let mut long = no_streams.to_le_bytes().to_vec();
        long.extend_from_slice(&1u16.to_le_bytes());
        long.extend_from_slice(&CACHED_TITLE_STRING.to_le_bytes());
        long.extend_from_slice(&0x3FFF_FFFFu32.to_le_bytes());
        assert!(matches!(parse_object_prop_set(&long), Err(OneError::Truncated(_))));
        // A length the specification forbids.
        let mut huge = no_streams.to_le_bytes().to_vec();
        huge.extend_from_slice(&1u16.to_le_bytes());
        huge.extend_from_slice(&CACHED_TITLE_STRING.to_le_bytes());
        huge.extend_from_slice(&0x4000_0000u32.to_le_bytes());
        assert!(matches!(parse_object_prop_set(&huge), Err(OneError::Corrupt(_))));
        // A reference with no stream entry behind it.
        let mut missing = no_streams.to_le_bytes().to_vec();
        missing.extend_from_slice(&1u16.to_le_bytes());
        missing.extend_from_slice(&PICTURE_CONTAINER.to_le_bytes());
        assert!(matches!(parse_object_prop_set(&missing), Err(OneError::Truncated(_))));
        // Extra streams promised and the object space stream denied at the same time.
        let mut both = (0x8000_0000u32 | (1 << 30)).to_le_bytes().to_vec();
        both.extend_from_slice(&0u16.to_le_bytes());
        assert!(matches!(parse_object_prop_set(&both), Err(OneError::Corrupt(_))));
    }

    #[test]
    fn deep_nesting_stops() {
        let mut data = Vec::new();
        for _ in 0..40 {
            data.extend_from_slice(&1u16.to_le_bytes());
            data.extend_from_slice(&(0x11u32 << 26).to_le_bytes());
        }
        data.extend_from_slice(&0u16.to_le_bytes());
        let mut whole = 0x8000_0000u32.to_le_bytes().to_vec();
        whole.extend_from_slice(&data);
        assert!(matches!(parse_object_prop_set(&whole), Err(OneError::Corrupt(_))));
    }

    #[test]
    fn text_survives_a_nul_terminator_and_odd_bytes() {
        let mut b = Build::default();
        b.data(CACHED_TITLE_STRING, b"h\x00i\x00\x00\x00");
        let set = parse_object_prop_set(&b.finish()).expect("parse");
        assert_eq!(set.text(CACHED_TITLE_STRING).as_deref(), Some("hi"));
        let mut odd = Build::default();
        odd.data(CACHED_TITLE_STRING, b"h");
        let set = parse_object_prop_set(&odd.finish()).expect("parse");
        assert_eq!(set.text(CACHED_TITLE_STRING).as_deref(), Some(""));
    }
}
