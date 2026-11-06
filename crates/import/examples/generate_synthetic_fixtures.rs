use std::io;
use std::path::Path;

#[path = "../tests/support/synthetic_corpus.rs"]
mod synthetic_corpus;

fn main() -> io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic");
    synthetic_corpus::generate(&root)
}
