//! Writes printed sheets as a PDF file (issue #168): one page for each sheet, the sheet's picture
//! covering it exactly, at the sheet's real size.
//!
//! A PDF of pictures is the simplest file every printer, viewer and "Print to PDF" reads alike.
//! Each picture is compressed losslessly (Flate), so the words stay sharp.

use super::raster::RgbImage;
use std::io::Write as _;

/// One sheet: its picture and its size in PDF points (1/72 inch).
pub struct Page<'a> {
    pub image: &'a RgbImage,
    pub width_pt: f32,
    pub height_pt: f32,
}

/// The PDF file of `pages`, titled `title`.
pub fn write(pages: &[Page<'_>], title: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    // Objects: 1 catalog, 2 page tree, 3 information, then three for each page: the page, its
    // contents and its picture.
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| 4 + i * 3).collect();
    let object = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(out.len());
        let n = offsets.len();
        let _ = writeln!(out, "{n} 0 obj");
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    object(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    object(&mut out, &mut offsets, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).as_bytes());
    object(&mut out, &mut offsets, format!("<< /Title {} /Producer (NoteCraft) >>", text_string(title)).as_bytes());
    for (i, page) in pages.iter().enumerate() {
        let id = page_ids[i];
        let (w, h) = (number(page.width_pt), number(page.height_pt));
        object(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /XObject << /Sheet {} 0 R >> >> /Contents {} 0 R >>",
                id + 2,
                id + 1
            )
            .as_bytes(),
        );
        let contents = format!("q {w} 0 0 {h} 0 0 cm /Sheet Do Q");
        object(&mut out, &mut offsets, format!("<< /Length {} >>\nstream\n{contents}\nendstream", contents.len()).as_bytes());
        let pixels = deflate(&page.image.bytes);
        let mut picture = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
            page.image.width,
            page.image.height,
            pixels.len()
        )
        .into_bytes();
        picture.extend_from_slice(&pixels);
        picture.extend_from_slice(b"\nendstream");
        object(&mut out, &mut offsets, &picture);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
    for offset in &offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let _ = write!(out, "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1);
    out
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    // Writing into memory cannot fail.
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

/// A number as PDF writes it: no exponent, at most two decimals.
fn number(value: f32) -> String {
    let value = if value.is_finite() { value } else { 0.0 };
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A PDF text string: plain ASCII as a literal, anything else as UTF-16 with its byte order mark.
fn text_string(text: &str) -> String {
    if text.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
        let escaped: String = text
            .chars()
            .flat_map(|c| match c {
                '(' | ')' | '\\' => vec!['\\', c],
                c => vec![c],
            })
            .collect();
        format!("({escaped})")
    } else {
        let hex: String = text.encode_utf16().map(|u| format!("{u:04X}")).collect();
        format!("<FEFF{hex}>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pdf_of_two_sheets_lists_two_pages_of_the_sheet_size() {
        let a = RgbImage::white(20, 30);
        let mut b = RgbImage::white(20, 30);
        b.bytes[0] = 0;
        let pdf =
            write(&[Page { image: &a, width_pt: 595.28, height_pt: 841.89 }, Page { image: &b, width_pt: 595.28, height_pt: 841.89 }], "Notes (1)");
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 2"));
        assert_eq!(text.matches("/MediaBox [0 0 595.28 841.89]").count(), 2);
        assert!(text.contains("/Title (Notes \\(1\\))"));
        assert!(text.trim_end().ends_with("%%EOF"));
        // Every object the cross-reference table lists starts where it says.
        let xref_at: usize = text.rsplit("startxref\n").next().and_then(|t| t.lines().next()).and_then(|n| n.trim().parse().ok()).expect("startxref");
        let table = &pdf[xref_at..];
        let table = String::from_utf8_lossy(table);
        for (n, line) in table.lines().skip(3).take_while(|l| l.ends_with(" n ")).enumerate() {
            let offset: usize = line[..10].parse().expect("offset");
            assert!(pdf[offset..].starts_with(format!("{} 0 obj", n + 1).as_bytes()), "object {} at {offset}", n + 1);
        }
    }

    #[test]
    fn a_title_outside_ascii_is_written_as_utf16() {
        assert_eq!(text_string("Été"), "<FEFF00C9007400E9>");
        assert_eq!(number(12.0), "12");
        assert_eq!(number(12.5), "12.5");
    }
}
