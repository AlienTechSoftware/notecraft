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

    /// Adds every font file in `folder` to `database`. Windows keeps per-user fonts in a folder the
    /// system scan does not read, so the catalog reads it as well.
    pub(crate) fn load_fonts_folder(database: &mut fontdb::Database, folder: &std::path::Path) {
        if folder.is_dir() {
            database.load_fonts_dir(folder);
        }
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

#[cfg(test)]
mod folder_tests {
    use super::Catalog;

    /// A font folder the catalog reads: the real Noto Sans Hebrew file, copied into a fresh folder.
    #[test]
    fn a_fonts_folder_adds_its_families_to_the_catalog() {
        let folder = std::env::temp_dir().join(format!("notecraft-fonts-folder-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/NotoSansHebrew.ttf");
        std::fs::copy(&source, folder.join("NotoSansHebrew.ttf")).expect("the font file");
        let mut database = fontdb::Database::new();
        Catalog::load_fonts_folder(&mut database, &folder);
        let catalog = Catalog::from_database(database);
        let _ = std::fs::remove_dir_all(&folder);
        assert!(catalog.families().iter().any(|name| name == "Noto Sans Hebrew"), "the family is listed: {:?}", catalog.families());
    }
}
