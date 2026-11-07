//! Notebook and section properties stored in a OneNote table of contents.

use crate::error::{OneError, Result};
use crate::guid::{FILE_TYPE_TOC, Guid};
use crate::props::PropertySet;
use crate::store::{Object, Store};

// Compatibility mapping corroborated by the public onenote.rs and OfficeIMO parsers.
// Microsoft's published MS-ONE property table does not list this numeric identifier.
pub(crate) const NOTEBOOK_COLOR_COMPATIBILITY_ID: u32 = 0x1400_1CBE;
const TOC_CHILDREN: u32 = 0x2400_1CF6;
const FILE_IDENTITY: u32 = 0x1C00_1D94;
const FOLDER_CHILD_FILENAME: u32 = 0x1C00_1D6B;
const ORDERING_ID: u32 = 0x1400_1CB9;
const MAX_TOC_ENTRIES: usize = 10_000;

/// Color and section references read from a `.onetoc2` file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Toc {
    /// Notebook color, if the TOC declares a defined value.
    pub color: Option<u32>,
    /// Direct section entries in the TOC.
    pub sections: Vec<TocSection>,
}

/// A section reference in a notebook TOC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TocSection {
    pub filename: String,
    pub identity: Option<Guid>,
    pub order: Option<u32>,
    pub color: Option<u32>,
}

/// Reads the notebook root color and direct section entries from a parsed TOC store.
pub(crate) fn parse(store: &Store) -> Result<Toc> {
    if store.header.file_type != FILE_TYPE_TOC {
        return Err(OneError::NotAToc("the file is not a OneNote table of contents".into()));
    }
    let Some(space) = store.spaces.first() else {
        return Err(OneError::Corrupt("the table of contents has no object space".into()));
    };
    let Some(root) = space.content_root() else {
        return Err(OneError::Corrupt("the table of contents has no content root".into()));
    };
    if root.kind() != 0x0001 {
        return Err(OneError::Corrupt("the table-of-contents root has an unexpected object type".into()));
    }
    let Some(properties) = root.props() else {
        return Err(OneError::Corrupt("the table-of-contents root has no properties".into()));
    };

    let mut toc = Toc { color: defined_color(properties), sections: Vec::new() };
    let section_ids = properties.object_ids(TOC_CHILDREN);
    if section_ids.len() > MAX_TOC_ENTRIES {
        return Err(OneError::Corrupt(format!("the table of contents claims {} entries", section_ids.len())));
    }
    for id in section_ids {
        let Some(object) = space.object(*id).filter(|object| object.kind() == 0x0001) else { continue };
        let Object { data: crate::store::ObjectData::Props(entry), .. } = object else { continue };
        let Some(filename) = entry.text(FOLDER_CHILD_FILENAME).filter(|name| !name.trim().is_empty()) else { continue };
        let identity = entry.bytes(FILE_IDENTITY).and_then(|bytes| <[u8; 16]>::try_from(bytes).ok()).map(Guid);
        toc.sections.push(TocSection {
            filename,
            identity,
            order: entry.number(ORDERING_ID).and_then(|n| u32::try_from(n).ok()),
            color: defined_color(entry),
        });
    }
    Ok(toc)
}

fn defined_color(properties: &PropertySet) -> Option<u32> {
    properties.number(NOTEBOOK_COLOR_COMPATIBILITY_ID).and_then(|value| u32::try_from(value).ok()).filter(|value| *value != u32::MAX)
}
