//! Rotate and Flip for a picture on the page (issue #273).
//!
//! A picture is turned by writing a turned copy beside the page and pointing the page at it,
//! not by changing the file the page named: the original stays in the page's `assets` folder,
//! so Undo has the very file it pointed at before, and a picture shared with another page or
//! linked from elsewhere is never changed under it. Neither page format has a way to say
//! "drawn turned" that the other could read, and a turned file reads the same everywhere.

use std::io::Cursor;

/// One of the four turns the picture menu offers, as OneNote and Word have them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    /// A quarter turn clockwise.
    Right,
    /// A quarter turn anticlockwise.
    Left,
    /// Mirrored left to right.
    FlipHorizontal,
    /// Mirrored top to bottom.
    FlipVertical,
}

impl Turn {
    /// True for a quarter turn, which swaps the picture's width and height.
    pub fn swaps_sides(self) -> bool {
        matches!(self, Turn::Right | Turn::Left)
    }

    /// What the turned copy's name says it is.
    fn tag(self) -> &'static str {
        match self {
            Turn::Right | Turn::Left => "rotated",
            Turn::FlipHorizontal | Turn::FlipVertical => "flipped",
        }
    }
}

/// Turns the picture in `bytes`. Returns the turned picture's bytes and the extension it is
/// written with: a JPEG stays a JPEG (a photo would grow several times over as a PNG), and
/// everything else becomes a PNG, which loses nothing. A picture that cannot be read (an SVG,
/// a JPEG XL, a damaged file) is an error, and the page is left as it was.
pub fn turn(bytes: &[u8], how: Turn) -> Result<(Vec<u8>, &'static str), String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let picture = image::load_from_memory_with_format(bytes, format).map_err(|e| e.to_string())?;
    let turned = match how {
        Turn::Right => picture.rotate90(),
        Turn::Left => picture.rotate270(),
        Turn::FlipHorizontal => picture.fliph(),
        Turn::FlipVertical => picture.flipv(),
    };
    let mut out = Cursor::new(Vec::new());
    if format == image::ImageFormat::Jpeg {
        let rgb = turned.to_rgb8();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
        encoder.encode_image(&rgb).map_err(|e| e.to_string())?;
        return Ok((out.into_inner(), "jpg"));
    }
    turned.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok((out.into_inner(), "png"))
}

/// The name the turned copy is offered under: `photo.png` becomes `photo-rotated.png`. A
/// copy of a copy keeps one tag, so turning a picture four times does not grow its name four
/// times; the store adds a number when the name is taken.
pub fn turned_name(src: &str, how: Turn, ext: &str) -> String {
    let file = src.rsplit('/').next().unwrap_or(src);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let mut base = stem;
    for tag in ["-rotated", "-flipped"] {
        if let Some(cut) = base.find(tag) {
            base = &base[..cut];
        }
    }
    let base = if base.is_empty() { "picture" } else { base };
    format!("{base}-{}.{ext}", how.tag())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3 by 2 picture whose top-left pixel is red and the rest white.
    fn picture() -> Vec<u8> {
        let mut img = image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 255, 255, 255]));
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img).write_to(&mut out, image::ImageFormat::Png).expect("png");
        out.into_inner()
    }

    fn read(bytes: &[u8]) -> image::RgbaImage {
        image::load_from_memory(bytes).expect("read").to_rgba8()
    }

    fn red_at(img: &image::RgbaImage) -> (u32, u32) {
        img.enumerate_pixels().find(|(_, _, p)| p.0 == [255, 0, 0, 255]).map(|(x, y, _)| (x, y)).expect("the red pixel")
    }

    #[test]
    fn each_turn_moves_the_corner_where_it_should() {
        let src = picture();
        for (how, size, corner) in [
            (Turn::Right, (2, 3), (1, 0)),
            (Turn::Left, (2, 3), (0, 2)),
            (Turn::FlipHorizontal, (3, 2), (2, 0)),
            (Turn::FlipVertical, (3, 2), (0, 1)),
        ] {
            let (bytes, ext) = turn(&src, how).expect("turned");
            assert_eq!(ext, "png");
            let img = read(&bytes);
            assert_eq!(img.dimensions(), size, "{how:?}");
            assert_eq!(red_at(&img), corner, "{how:?}");
            assert_eq!(how.swaps_sides(), size != (3, 2));
        }
        // Four quarter turns are the picture again.
        let mut bytes = src.clone();
        for _ in 0..4 {
            bytes = turn(&bytes, Turn::Right).expect("turned").0;
        }
        assert_eq!(read(&bytes), read(&src));
    }

    #[test]
    fn a_jpeg_stays_a_jpeg_and_nonsense_is_refused() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 2, image::Rgb([10, 200, 30])));
        let mut jpeg = Cursor::new(Vec::new());
        img.write_to(&mut jpeg, image::ImageFormat::Jpeg).expect("jpeg");
        let (bytes, ext) = turn(&jpeg.into_inner(), Turn::Left).expect("turned");
        assert_eq!(ext, "jpg");
        assert_eq!(image::guess_format(&bytes).expect("format"), image::ImageFormat::Jpeg);
        assert_eq!(read(&bytes).dimensions(), (2, 4));
        assert!(turn(b"<svg xmlns='http://www.w3.org/2000/svg'/>", Turn::Right).is_err());
    }

    #[test]
    fn the_copy_is_named_after_the_picture_once() {
        assert_eq!(turned_name("assets/photo.png", Turn::Right, "png"), "photo-rotated.png");
        assert_eq!(turned_name("assets/photo-rotated.png", Turn::Left, "png"), "photo-rotated.png");
        assert_eq!(turned_name("assets/photo-rotated-2.jpg", Turn::FlipVertical, "jpg"), "photo-flipped.jpg");
        assert_eq!(turned_name("assets/scan.webp", Turn::FlipHorizontal, "png"), "scan-flipped.png");
        assert_eq!(turned_name("assets/.png", Turn::Right, "png"), "picture-rotated.png");
    }
}
