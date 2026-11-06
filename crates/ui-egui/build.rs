use std::{env, fs, path::PathBuf};

fn unescape(value: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            _ => return None,
        }
    }
    Some(out)
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let catalog = manifest.join("../../i18n/en.tsv");
    println!("cargo:rerun-if-changed={}", catalog.display());
    let text = fs::read_to_string(&catalog).expect("read English UI catalog");
    let mut entries = Vec::new();
    for line in text.lines() {
        let mut columns = line.splitn(3, '\t');
        if columns.next() != Some("@id") {
            continue;
        }
        let Some(id) = columns.next().and_then(unescape) else { continue };
        let Some(source) = columns.next().and_then(unescape) else { continue };
        if id != "language.name" {
            entries.push((source, id));
        }
    }
    entries.sort();
    let mut generated = String::from("pub fn english_text(id: &str) -> Option<&'static str> { match id {\n");
    for (source, id) in &entries {
        generated.push_str(&format!("{:?} => Some({:?}),\n", id, source));
    }
    generated.push_str("_ => None, } }\n");

    let mut by_source = entries.clone();
    by_source.sort();
    by_source.dedup_by(|left, right| left.0 == right.0);
    generated.push_str("pub fn source_id(source: &str) -> Option<&'static str> { match source {\n");
    for (source, id) in by_source {
        generated.push_str(&format!("{:?} => Some({:?}),\n", source, id));
    }
    generated.push_str("_ => None, } }\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("out dir")).join("i18n_source_ids.rs");
    fs::write(out, generated).expect("write generated source IDs");
}
