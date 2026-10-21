//! What is on the clipboard, including the things egui does not hand us.
//!
//! egui's only paste event is `Paste(String)`, so a picture copied from a screen clipper,
//! a browser or a paint program never reaches the window: pressing Ctrl+V with a clipped
//! screenshot on the clipboard did nothing at all, which is what the owner ran into.
//!
//! The clipboard is read here directly instead, through `arboard`. Nothing new enters the
//! build for it: eframe already uses the same crate for the text clipboard, so this is a
//! direct edge to a crate that is compiled in either way.
//!
//! Everything here answers `None` rather than failing. A clipboard is shared with every
//! other program on the machine: it can be empty, held open by something else, or hold
//! something of a kind we do not want, and none of that is an error worth a message.

/// A picture from the clipboard, as PNG bytes ready to be written beside a page.
pub struct Picture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// How large a pasted picture may be, in pixels each way. A screen clipping of a very
/// large display is still a screenshot; anything past this is a mistake or an attack, and
/// encoding it would stall the window.
const MAX_SIDE: u32 = 20_000;

/// The picture on the clipboard, encoded as PNG. `None` when there is no picture on it,
/// when the clipboard cannot be read, or when what is there is not a sane size.
pub fn picture() -> Option<Picture> {
    let mut board = arboard::Clipboard::new().ok()?;
    let image = board.get_image().ok()?;
    let (w, h) = (u32::try_from(image.width).ok()?, u32::try_from(image.height).ok()?);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    // `arboard` gives unmultiplied RGBA, which is what `image` expects.
    let buffer = image::RgbaImage::from_raw(w, h, image.bytes.into_owned())?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(buffer).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    Some(Picture { png, width: w, height: h })
}

/// Whether a picture is on the clipboard now. Used to decide whether to offer a menu item,
/// so it must be cheap enough to ask while drawing a frame and must never block.
pub fn has_picture() -> bool {
    arboard::Clipboard::new().ok().and_then(|mut b| b.get_image().ok()).is_some()
}

/// The text on the clipboard, for the menu item that pastes without the keyboard.
/// Puts text on the clipboard. The window itself does not need this - egui writes the
/// clipboard through its own platform layer - but a test that wants to prove a copy
/// happened has to be able to put something else there first.
pub fn set_text(text: &str) -> Result<(), String> {
    with_writer(|board| board.set_text(text.to_string()))
}

/// Put the same copied item on the system clipboard as HTML and plain text. Setting these
/// together lets each receiving application choose the richer form it understands.
pub fn set_html(html: &str, text: &str) -> Result<(), String> {
    with_writer(|board| board.set().html(html.to_string(), Some(text.to_string())))
}

/// The rich HTML flavor for a copied page link.
pub fn html_link(title: &str, uri: &str) -> String {
    format!("<p><a href=\"{}\">{}</a></p>", notecraft_formats::html::escape(uri), notecraft_formats::html::escape(title))
}

// X11 and Wayland applications serve clipboard data on request. Keep the writer alive
// while NoteCraft runs so the copied HTML and its fallback remain available to other apps.
thread_local! {
    static WRITER: std::cell::RefCell<Option<arboard::Clipboard>> = const { std::cell::RefCell::new(None) };
}

fn with_writer(write: impl FnOnce(&mut arboard::Clipboard) -> Result<(), arboard::Error>) -> Result<(), String> {
    WRITER.with(|writer| {
        let mut slot = writer.try_borrow_mut().map_err(|e| e.to_string())?;
        if slot.is_none() {
            *slot = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        match slot.as_mut().map(write) {
            Some(Ok(())) => Ok(()),
            Some(Err(error)) => {
                *slot = None;
                Err(error.to_string())
            }
            None => Err("clipboard is unavailable".to_string()),
        }
    })
}

pub fn text() -> Option<String> {
    with_reader(|board| board.get_text().ok().filter(|text| !text.is_empty()))
}

/// The rich HTML flavour on the clipboard, when another application supplied one.
/// Keeping this separate from [`text`] lets callers prefer formatted content while
/// retaining plain text as the fallback for applications that only offer it.
pub fn html() -> Option<String> {
    with_reader(|board| board.get().html().ok().filter(|html| !html.trim().is_empty()))
}

fn with_reader<T>(read: impl FnOnce(&mut arboard::Clipboard) -> Option<T>) -> Option<T> {
    let mut read = Some(read);
    WRITER.with(|writer| {
        if let Ok(mut slot) = writer.try_borrow_mut()
            && let Some(board) = slot.as_mut()
        {
            let read = read.take()?;
            return read(board);
        }
        let mut board = arboard::Clipboard::new().ok()?;
        let read = read.take()?;
        read(&mut board)
    })
}

/// A name for a pasted picture, so two pastes do not fight over one file. The clock is
/// enough: a person cannot paste twice in the same second by hand, and `attach_bytes`
/// makes the name unique anyway if they manage it.
pub fn picture_name() -> String {
    let now = notecraft_model::time::iso_now();
    let stamp: String = now.chars().filter(char::is_ascii_alphanumeric).collect();
    format!("pasted-{stamp}.png")
}

#[cfg(test)]
mod tests {
    use super::*;
    // std's own Instant panics on wasm32-unknown-unknown, which has no clock; web_time is
    // the same type everywhere else and performance.now()/Date.now() there.
    use web_time::Instant;

    #[test]
    fn a_pasted_pictures_name_is_a_plain_file_name() {
        let name = picture_name();
        assert!(name.starts_with("pasted-") && name.ends_with(".png"), "{name}");
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'), "nothing in it needs escaping: {name}");
        assert!(name.len() > 12, "the clock is in it: {name}");
    }

    /// The clipboard belongs to the machine the tests run on, so this cannot assert what
    /// is on it. What it can assert is that asking never panics and never blocks, whatever
    /// is there, which is the part that would break a window.
    #[test]
    fn asking_the_clipboard_is_always_safe() {
        let before = Instant::now();
        let _ = has_picture();
        let _ = picture();
        let _ = html();
        let _ = text();
        assert!(before.elapsed() < std::time::Duration::from_secs(5), "asking the clipboard does not hang the window");
    }

    #[test]
    fn a_copied_page_link_has_an_html_anchor() {
        assert_eq!(html_link("A <page>", "notecraft:/book/page?a&b"), "<p><a href=\"notecraft:/book/page?a&amp;b\">A &lt;page&gt;</a></p>");
    }
}
