//! Catalog and load installed font families on desktop.

use std::collections::BTreeSet;
use std::sync::Arc;

#[cfg(target_os = "linux")]
#[path = "../system_fonts.lnx.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "../system_fonts.win.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "../system_fonts.mac.rs"]
mod platform;

/// A catalog of font metadata. Font bytes are loaded only when a family is requested.
pub struct Catalog {
    database: fontdb::Database,
    families: Vec<String>,
}

impl Catalog {
    /// Discover fonts available to the current desktop user.
    pub fn discover() -> Result<Self, String> {
        let catalog = Self::from_database(platform::discover());
        if catalog.families.is_empty() {
            return Err("No installed font families could be discovered".to_string());
        }
        Ok(catalog)
    }

    pub(crate) fn from_database(database: fontdb::Database) -> Self {
        let families = database
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .filter(|name| !name.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self { database, families }
    }

    pub fn families(&self) -> &[String] {
        &self.families
    }

    /// Add the regular face to its own egui family, retaining the proportional fallback chain.
    pub fn install(&self, family: &str, definitions: &mut egui::FontDefinitions) -> Result<egui::FontFamily, String> {
        if !self.families.iter().any(|name| name == family) {
            return Err(format!("Font family {family:?} is not in the installed font catalog"));
        }
        let query = fontdb::Query { families: &[fontdb::Family::Name(family)], ..Default::default() };
        let id = self.database.query(&query).ok_or_else(|| format!("No usable face was found for font family {family:?}"))?;
        let (bytes, index) = self
            .database
            .with_face_data(id, |bytes, index| (bytes.to_vec(), index))
            .ok_or_else(|| format!("Could not read the font data for family {family:?}"))?;
        let mut data = egui::FontData::from_owned(bytes);
        data.index = index;
        let name = format!("notecraft-system-font:{family}");
        let key = super::named_family(family);
        let mut chain = vec![name.clone()];
        let fallbacks =
            definitions.families.get(&egui::FontFamily::Proportional).ok_or_else(|| "The proportional font fallback chain is missing".to_string())?;
        chain.extend(fallbacks.iter().cloned());
        definitions.font_data.insert(name, Arc::new(data));
        definitions.families.insert(key.clone(), chain);
        Ok(key)
    }
}
