//! A drawing as a file, and back again.
//!
//! What is written is ordinary SVG: one filled path per stroke, holding the outline of the
//! ribbon, because SVG has no variable-width stroke of its own and every editor fakes it
//! the same way. Any browser opens the file and draws the handwriting correctly.
//!
//! [`Flavour::Editable`], which is what a page saves, puts the pen's own readings on each
//! path as well, in our namespace (`nc:dabs`). Other programs ignore an attribute in a
//! namespace they do not know, so the file stays ordinary SVG; we read the readings back
//! and have the stroke exactly as it was drawn, which is what makes it possible to erase
//! part of it later, or change its weight, or redraw it at another size. [`Flavour::Plain`]
//! leaves them out, for a drawing that is going somewhere else.
//!
//! Reading is deliberately small and forgiving: it looks for `<path>` elements and reads
//! the attributes it put there. A file it cannot make sense of gives an error, never a
//! panic, and a file with some good strokes and some bad gives the good ones.

use crate::{Dab, Drawing, MAX_STROKES, Rgb, Stroke, Tool, shape};

/// Our namespace for the readings kept beside the drawn outline.
pub const NS: &str = "https://notecraft.org/ns/ink";

/// How much of a file this will read, in bytes. A drawing is small; anything approaching
/// this is a mistake or an attack, and is refused before it is parsed.
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

/// Which of the two a file is written as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flavour {
    /// The outlines, and the pen's readings beside them. What a page saves.
    #[default]
    Editable,
    /// The outlines alone: a picture, for sending somewhere else.
    Plain,
}

/// What went wrong reading a drawing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InkError {
    /// The file is larger than [`MAX_BYTES`].
    TooBig(usize),
    /// There is no `<svg` in it at all.
    NotSvg,
}

impl std::fmt::Display for InkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InkError::TooBig(n) => write!(f, "this drawing is {n} bytes, larger than a drawing may be"),
            InkError::NotSvg => write!(f, "this is not an SVG drawing"),
        }
    }
}

impl std::error::Error for InkError {}

/// A number as short as it can be written without anyone seeing the difference: two
/// decimals on the page is a fortieth of a point, far below a pixel.
fn num(v: f32) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let mut s = format!("{:.2}", v);
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    if s.is_empty() || s == "-0" { "0".to_string() } else { s }
}

/// The drawing as SVG.
pub fn write(drawing: &Drawing, flavour: Flavour) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\"");
    if flavour == Flavour::Editable {
        out.push_str(&format!(" xmlns:nc=\"{NS}\""));
    }
    out.push_str(&format!(" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n", w = num(drawing.width), h = num(drawing.height)));
    out.push_str("  <title>Ink</title>\n");
    for stroke in &drawing.strokes {
        let ring = shape::outline(stroke);
        if ring.is_empty() {
            continue;
        }
        let mut d = String::with_capacity(ring.len() * 12);
        for (i, (x, y)) in ring.iter().enumerate() {
            d.push_str(if i == 0 { "M" } else { "L" });
            d.push_str(&num(*x));
            d.push(' ');
            d.push_str(&num(*y));
            d.push(' ');
        }
        d.push('Z');
        out.push_str("  <path");
        if flavour == Flavour::Editable {
            out.push_str(&format!(" nc:tool=\"{}\" nc:width=\"{}\"", stroke.tool.id(), num(stroke.width)));
            if (stroke.gain - 1.0).abs() > 0.001 {
                out.push_str(&format!(" nc:gain=\"{}\"", num(stroke.gain)));
            }
            out.push_str(&format!(" nc:dabs=\"{}\"", dabs_text(&stroke.dabs)));
        }
        out.push_str(&format!(" fill=\"{}\"", stroke.colour.hex()));
        let opacity = stroke.tool.opacity();
        if opacity < 1.0 {
            out.push_str(&format!(" fill-opacity=\"{}\"", num(opacity)));
        }
        out.push_str(&format!(" d=\"{}\"/>\n", d.trim_end()));
    }
    out.push_str("</svg>\n");
    out
}

/// The readings as one attribute: `x,y,p x,y,p ...`.
fn dabs_text(dabs: &[Dab]) -> String {
    let mut out = String::with_capacity(dabs.len() * 12);
    for (i, d) in dabs.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&num(d.x));
        out.push(',');
        out.push_str(&num(d.y));
        out.push(',');
        out.push_str(&num(d.pressure));
    }
    out
}

/// Reads them back.
fn dabs_from(text: &str) -> Vec<Dab> {
    text.split_whitespace()
        .filter_map(|piece| {
            let mut parts = piece.split(',');
            let x: f32 = parts.next()?.parse().ok()?;
            let y: f32 = parts.next()?.parse().ok()?;
            let p: f32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(1.0);
            Some(Dab::new(x, y, p))
        })
        .take(crate::MAX_DABS)
        .collect()
}

/// Reads a drawing written by [`write`]. Strokes saved without their readings
/// ([`Flavour::Plain`]) cannot be given back as strokes, so they are skipped and the count
/// of them is returned beside the drawing: a caller can then say that the file holds ink it
/// will not be able to edit.
pub fn read(text: &str) -> Result<(Drawing, usize), InkError> {
    if text.len() > MAX_BYTES {
        return Err(InkError::TooBig(text.len()));
    }
    let Some(open) = find_tag(text, "<svg") else { return Err(InkError::NotSvg) };
    let width = attr(open, "width").and_then(|v| v.trim().parse().ok()).unwrap_or(600.0);
    let height = attr(open, "height").and_then(|v| v.trim().parse().ok()).unwrap_or(400.0);
    let mut drawing = Drawing::new(width, height);
    let mut flat = 0usize;
    for tag in tags(text, "<path") {
        if drawing.strokes.len() >= MAX_STROKES {
            break;
        }
        let Some(dabs) = attr(tag, "nc:dabs") else {
            flat += 1;
            continue;
        };
        let mut stroke = Stroke {
            tool: attr(tag, "nc:tool").and_then(|v| Tool::parse(&v)).unwrap_or_default(),
            colour: attr(tag, "fill").and_then(|v| Rgb::parse(&v)).unwrap_or(Rgb::BLACK),
            width: attr(tag, "nc:width").and_then(|v| v.trim().parse().ok()).unwrap_or(3.0),
            gain: attr(tag, "nc:gain").and_then(|v| v.trim().parse().ok()).unwrap_or(1.0),
            dabs: dabs_from(&dabs),
        }
        .sane();
        if stroke.is_empty() {
            flat += 1;
            continue;
        }
        stroke.dabs.shrink_to_fit();
        drawing.strokes.push(stroke);
    }
    Ok((drawing, flat))
}

/// The text of the first tag that begins with `open`, without its angle brackets.
fn find_tag<'a>(text: &'a str, open: &'a str) -> Option<&'a str> {
    tags(text, open).next()
}

/// Every tag beginning with `open`, each without its angle brackets.
fn tags<'a>(text: &'a str, open: &'a str) -> impl Iterator<Item = &'a str> {
    let mut from = 0usize;
    std::iter::from_fn(move || {
        loop {
            let rest = text.get(from..)?;
            let at = rest.find(open)?;
            let start = from + at + open.len();
            // `<pathological` is not `<path`: the name has to end here.
            let next = text.get(start..).and_then(|s| s.chars().next());
            let end = text.get(start..).and_then(|s| s.find('>')).map(|e| start + e);
            from = end.map_or(text.len(), |e| e + 1);
            match (next, end) {
                (Some(c), Some(e)) if c.is_whitespace() || c == '/' || c == '>' => return text.get(start..e),
                (_, Some(_)) => continue,
                _ => return None,
            }
        }
    })
}

/// The value of `name="..."` inside a tag's text, with the five XML entities put back.
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut from = 0usize;
    loop {
        let rest = tag.get(from..)?;
        let at = rest.find(name)?;
        let before = tag.get(..from + at).and_then(|s| s.chars().last());
        let after = tag.get(from + at + name.len()..)?;
        from += at + name.len();
        // The name must stand alone, so `width` does not match inside `nc:width`.
        if before.is_some_and(|c| !c.is_whitespace()) {
            continue;
        }
        let value = after.trim_start();
        let Some(value) = value.strip_prefix('=') else { continue };
        let value = value.trim_start();
        let quote = value.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let value = value.get(1..)?;
        let end = value.find(quote)?;
        return Some(unescape(value.get(..end)?));
    }
}

fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Drawing {
        let mut d = Drawing::new(300.0, 200.0);
        let mut pen = Stroke::new(Tool::Pen, Rgb(0x20, 0x20, 0x20), 4.0);
        for (x, p) in [(10.0, 0.2), (40.0, 0.6), (70.0, 1.0), (100.0, 0.4)] {
            pen.dabs.push(Dab::new(x, 50.0, p));
        }
        d.add(pen);
        let mut marker = Stroke::with_gain(Tool::Highlighter, Rgb(0xff, 0xe0, 0x30), 14.0, 1.8);
        marker.dabs.push(Dab::new(10.0, 120.0, 1.0));
        marker.dabs.push(Dab::new(180.0, 120.0, 1.0));
        d.add(marker);
        d
    }

    #[test]
    fn a_drawing_written_and_read_is_the_same_drawing() {
        let before = sample();
        let text = write(&before, Flavour::Editable);
        let (after, flat) = read(&text).expect("read it back");
        assert_eq!(flat, 0, "nothing was lost");
        assert_eq!(after.width, before.width);
        assert_eq!(after.height, before.height);
        assert_eq!(after.strokes.len(), before.strokes.len());
        for (a, b) in after.strokes.iter().zip(&before.strokes) {
            assert_eq!(a.tool, b.tool);
            assert!((a.gain - b.gain).abs() < 0.01, "the pen's setting came back too");
            assert_eq!(a.colour, b.colour);
            assert_eq!(a.width, b.width);
            assert_eq!(a.dabs.len(), b.dabs.len());
            for (x, y) in a.dabs.iter().zip(&b.dabs) {
                assert!((x.x - y.x).abs() < 0.01 && (x.y - y.y).abs() < 0.01 && (x.pressure - y.pressure).abs() < 0.01, "{x:?} is {y:?}");
            }
        }
        // Writing it again gives the same file, so saving an unchanged page changes nothing.
        assert_eq!(write(&after, Flavour::Editable), text);
    }

    #[test]
    fn the_file_is_ordinary_svg_that_anything_can_draw() {
        let text = write(&sample(), Flavour::Editable);
        assert!(text.starts_with("<?xml"), "it says what it is");
        assert!(text.contains("xmlns=\"http://www.w3.org/2000/svg\""), "in the SVG namespace");
        assert!(text.contains("<path"), "with the strokes as paths");
        assert!(text.contains("fill=\"#202020\""), "filled, because SVG has no variable-width stroke");
        assert!(text.contains("fill-opacity"), "and the highlighter is see-through");
        assert!(text.ends_with("</svg>\n"));
        // Every number in a path is a number.
        for piece in text.split("d=\"").skip(1).filter_map(|s| s.split('"').next()) {
            for word in piece.split_whitespace() {
                let word = word.trim_start_matches(['M', 'L']).trim_end_matches('Z');
                assert!(word.is_empty() || word.parse::<f32>().is_ok(), "{word:?} in {piece:?}");
            }
        }
    }

    #[test]
    fn the_plain_flavour_keeps_the_picture_and_drops_the_readings() {
        let plain = write(&sample(), Flavour::Plain);
        assert!(!plain.contains("nc:"), "no readings, and no namespace for them:\n{plain}");
        assert!(plain.contains("<path"), "but the drawing is all there");
        assert_eq!(plain.matches("<path").count(), 2, "both strokes");

        // It is still readable, and says honestly that its strokes cannot be edited.
        let (drawing, flat) = read(&plain).expect("read");
        assert!(drawing.is_empty(), "there are no strokes to give back");
        assert_eq!(flat, 2, "and it says how many were only a picture");
    }

    #[test]
    fn reading_junk_gives_an_error_or_nothing_but_never_a_panic() {
        assert_eq!(read("not a drawing"), Err(InkError::NotSvg));
        assert_eq!(read(&"x".repeat(MAX_BYTES + 1)), Err(InkError::TooBig(MAX_BYTES + 1)));

        for junk in [
            "<svg>",
            "<svg><path/></svg>",
            "<svg width=\"nonsense\" height=\"\"><path nc:dabs=\"\"/></svg>",
            "<svg><path nc:dabs=\"1,2,3,4,5 , ,, 6\"/></svg>",
            "<svg><path nc:dabs=\"NaN,inf,-inf\" nc:width=\"NaN\"/></svg>",
            "<svg><path nc:dabs='1,1,1 2,2,1' nc:tool=\"crayon\" fill=\"not a colour\"/></svg>",
            "<svg><pathological nc:dabs=\"1,1,1\"/></svg>",
            "<svg><path nc:dabs=\"1,1,1\"",
            "<svg\u{0}><path nc:dabs=\"1,1,1\"/></svg>",
        ] {
            // Either a drawing that makes sense, or a plain error. Never a panic, and
            // never a drawing with nonsense in it.
            let got = read(junk);
            if let Ok((d, _)) = got {
                assert!(d.width >= 1.0 && d.height >= 1.0, "{junk:?} gave a sheet of a sane size");
                for s in &d.strokes {
                    assert!(s.width >= crate::MIN_WIDTH && s.width <= crate::MAX_WIDTH, "{junk:?} gave width {}", s.width);
                    assert!(s.dabs.iter().all(|p| p.x.is_finite() && p.y.is_finite() && (0.0..=1.0).contains(&p.pressure)), "{junk:?}");
                }
            }
        }
    }

    #[test]
    fn an_attribute_is_read_whole_and_not_mistaken_for_another() {
        // `width` must not be found inside `nc:width`, and the other way round.
        let tag = "path nc:width=\"9\" width=\"3\" fill=\"#010203\"";
        assert_eq!(attr(tag, "nc:width").as_deref(), Some("9"));
        assert_eq!(attr(tag, "width").as_deref(), Some("3"));
        assert_eq!(attr(tag, "fill").as_deref(), Some("#010203"));
        assert_eq!(attr(tag, "height"), None);
        // Both kinds of quote, spaces around the equals, and the entities.
        assert_eq!(attr("path a = 'one two'", "a").as_deref(), Some("one two"));
        assert_eq!(attr("path a=\"&lt;&amp;&gt;\"", "a").as_deref(), Some("<&>"));
    }

    #[test]
    fn a_drawing_with_nothing_in_it_is_still_a_file() {
        let empty = Drawing::new(100.0, 80.0);
        let text = write(&empty, Flavour::Editable);
        let (back, flat) = read(&text).expect("read");
        assert_eq!(flat, 0);
        assert!(back.is_empty());
        assert_eq!((back.width, back.height), (100.0, 80.0), "the sheet is the size it was");
    }
}
