//! Catalog and load installed font families on desktop.

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
    /// The scripts each family's regular face covers, read once when the catalog is built.
    scripts: std::collections::BTreeMap<String, Vec<crate::font_picker::Script>>,
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
        // The first face of each family stands for it, as the family's regular face.
        let mut first_face = std::collections::BTreeMap::new();
        for face in database.faces() {
            if let Some((name, _)) = face.families.first()
                && !name.is_empty()
            {
                first_face.entry(name.clone()).or_insert(face.id);
            }
        }
        let scripts = first_face
            .iter()
            .map(|(name, id)| {
                let covered = database
                    .with_face_data(*id, |data, index| {
                        ttf_parser::Face::parse(data, index).map(|face| crate::font_picker::scripts_of(|c| face.glyph_index(c).is_some())).ok()
                    })
                    .flatten()
                    .unwrap_or_default();
                (name.clone(), covered)
            })
            .collect();
        let families = first_face.into_keys().collect();
        Self { database, families, scripts }
    }

    /// The scripts the family covers, or none when the family is not in the catalog.
    pub fn scripts(&self, family: &str) -> Vec<crate::font_picker::Script> {
        self.scripts.get(family).cloned().unwrap_or_default()
    }

    /// Adds every font file in `folder` to `database`. Windows keeps per-user fonts in a folder the
    /// system scan does not read, so the catalog reads it as well.
    #[cfg(any(test, target_os = "windows"))]
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
