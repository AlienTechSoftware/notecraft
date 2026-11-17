//! Automatic Shapes (issue #294) against the real strokes in the checked-in Welcome notebook,
//! in both page formats: written letters are never turned into shapes, and the shapes page's
//! drawn shapes are read as what they are.

use notecraft_ink::recognise::recognise;
use std::path::Path;

fn kinds(format: &str, page: &str) -> Vec<Option<&'static str>> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/demo/DemoNotebook").join(format).join("10-en/how-to").join(page).join("ink.svg");
    let text = std::fs::read_to_string(&path).expect("demo ink");
    let (drawing, _) = notecraft_ink::svg::read(&text).expect("readable ink");
    drawing.strokes.iter().map(|s| recognise(s).map(|r| r.id())).collect()
}

#[test]
fn handwriting_stays_writing_in_both_formats() {
    for format in ["html", "cmgfm"] {
        let found = kinds(format, "handwriting-and-drawing");
        // The first twelve strokes are the word written by hand.
        assert!(found.len() > 12, "{format}: {found:?}");
        assert!(found[..12].iter().all(Option::is_none), "{format}: letters were taken for shapes: {found:?}");
        assert!(found.contains(&Some("ellipse")), "{format}: the drawn circle is read as one: {found:?}");
    }
}

#[test]
fn the_shapes_page_reads_as_its_shapes_and_leaves_the_rest() {
    for format in ["html", "cmgfm"] {
        let found = kinds(format, "shapes");
        for kind in ["line", "rectangle", "ellipse", "triangle"] {
            assert!(found.contains(&Some(kind)), "{format}: no {kind} in {found:?}");
        }
        assert!(found.iter().any(Option::is_none), "{format}: a star, diamond or arrow is not taken for anything: {found:?}");
    }
}
