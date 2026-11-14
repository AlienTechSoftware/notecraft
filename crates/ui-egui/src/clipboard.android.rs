//! The clipboard on Android, under the name `arboard` that `clipboard.rs` reads.
//!
//! `arboard` has no Android backend: Android's clipboard is the Java `ClipboardManager`
//! service, reached only through JNI, and NoteCraft has no unsafe code to make that call.
//! Until an Android shell brings the clipboard across, this stand-in answers every request
//! with the same error, so `clipboard::open` fails and the window says copy and paste do not
//! reach other apps, rather than letting each paste fail in silence. Copy and paste inside
//! NoteCraft still work through egui's own in-app clipboard.

use std::borrow::Cow;
use std::fmt;

/// Why the clipboard cannot be used. Only the cases `clipboard.rs` names are needed.
#[derive(Debug)]
pub enum Error {
    /// Never returned here; `clipboard.rs` retries on it on the desktop.
    #[allow(dead_code)]
    ClipboardOccupied,
    /// The Android clipboard is not connected to NoteCraft yet.
    ClipboardNotSupported,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ClipboardOccupied => f.write_str("the clipboard is in use by another app"),
            Error::ClipboardNotSupported => f.write_str("the Android clipboard is not connected to NoteCraft yet"),
        }
    }
}

/// A picture as `arboard` hands it over: unmultiplied RGBA, row by row.
pub struct ImageData<'a> {
    pub width: usize,
    pub height: usize,
    pub bytes: Cow<'a, [u8]>,
}

/// The clipboard. It can never be opened on Android yet.
pub struct Clipboard;

impl Clipboard {
    pub fn new() -> Result<Clipboard, Error> {
        Err(Error::ClipboardNotSupported)
    }
    pub fn get_image(&mut self) -> Result<ImageData<'static>, Error> {
        Err(Error::ClipboardNotSupported)
    }
    pub fn get_text(&mut self) -> Result<String, Error> {
        Err(Error::ClipboardNotSupported)
    }
    pub fn set_text(&mut self, _text: String) -> Result<(), Error> {
        Err(Error::ClipboardNotSupported)
    }
    pub fn set(&mut self) -> Set {
        Set
    }
    pub fn get(&mut self) -> Get {
        Get
    }
}

/// `Clipboard::set()`: the HTML flavour with its plain-text alternative.
pub struct Set;

impl Set {
    pub fn html(self, _html: String, _alt_text: Option<String>) -> Result<(), Error> {
        Err(Error::ClipboardNotSupported)
    }
}

/// `Clipboard::get()`: the HTML flavour.
pub struct Get;

impl Get {
    pub fn html(self) -> Result<String, Error> {
        Err(Error::ClipboardNotSupported)
    }
}
