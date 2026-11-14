//! The picture formats the page draws, beyond what egui_extras reads by itself.
//!
//! egui_extras draws pictures through the `image` crate. JPEG XL (issue #29) is not one of the
//! formats `image` builds in, so its decoder, `jxl-oxide`, is registered with `image` as a
//! decoding hook for `.jxl` and for JPEG XL's two signatures. Once it is, a `.jxl` in a page's
//! `assets` folder is drawn like a PNG: the file loader's extension check, the page's own byte
//! loader and `image`'s format guessing all find it.

/// Registers the picture decoders `image` does not have, once for the whole process: the hooks
/// are global.
pub fn register_formats() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = jxl_oxide::integration::register_image_decoding_hook();
    });
}

/// egui_extras' picture loaders, with every format above registered first.
pub fn install(ctx: &egui::Context) {
    register_formats();
    egui_extras::install_image_loaders(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 24 by 16, left half red, right half blue, lossless.
    const TWO_COLOURS: &[u8] = include_bytes!("../tests/fixtures/two-colours.jxl");

    #[test]
    fn a_jpeg_xl_picture_decodes_by_its_signature() {
        register_formats();
        let picture = image::ImageReader::new(std::io::Cursor::new(TWO_COLOURS))
            .with_guessed_format()
            .expect("read")
            .decode()
            .expect("JPEG XL decodes once the hook is registered")
            .to_rgba8();
        assert_eq!(picture.dimensions(), (24, 16));
        assert_eq!(picture.get_pixel(2, 8).0, [255, 0, 0, 255], "the left half is red");
        assert_eq!(picture.get_pixel(21, 8).0, [0, 0, 255, 255], "the right half is blue");
    }

    #[test]
    fn junk_with_a_jpeg_xl_name_is_an_error_not_a_panic() {
        register_formats();
        let junk = [0xFF_u8, 0x0A, 1, 2, 3];
        let read = image::ImageReader::new(std::io::Cursor::new(&junk[..])).with_guessed_format().expect("read");
        assert!(read.decode().is_err());
    }

    /// The same path a picture in a page's `assets` folder takes when it is drawn.
    #[test]
    fn a_jpeg_xl_attachment_loads_through_the_file_loader() {
        use notecraft_store::testing::TempDir;
        let t = TempDir::new("pictures-jxl");
        let page = t.path().join("page");
        std::fs::create_dir_all(page.join("assets")).expect("assets");
        std::fs::write(page.join("assets").join("two-colours.jxl"), TWO_COLOURS).expect("jxl");
        let uri = crate::preview::image_uri(&page, "assets/two-colours.jxl").expect("uri");
        let ctx = egui::Context::default();
        install(&ctx);
        let mut last = String::new();
        for _ in 0..100 {
            match ctx.try_load_image(&uri, egui::SizeHint::default()) {
                Ok(egui::load::ImagePoll::Ready { image }) => {
                    assert_eq!(image.size, [24, 16]);
                    return;
                }
                Ok(egui::load::ImagePoll::Pending { .. }) => last = "pending".into(),
                Err(e) => panic!("{uri}: {e}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("never ready: {last}");
    }
}
