pub(super) fn discover() -> fontdb::Database {
    let mut database = fontdb::Database::new();
    database.load_system_fonts();
    database
}
