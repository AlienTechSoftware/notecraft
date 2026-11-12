pub(super) fn discover() -> fontdb::Database {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        super::Catalog::load_fonts_folder(&mut database, &std::path::Path::new(&local).join("Microsoft").join("Windows").join("Fonts"));
    }
    database
}
