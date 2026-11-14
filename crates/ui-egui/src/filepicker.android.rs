//! File and folder dialogs on Android, under the name `rfd` that `app.rs` calls.
//!
//! `rfd` has no Android backend. Android's pickers (the Storage Access Framework) are Java
//! activities that answer through `onActivityResult`, which a native activity cannot receive
//! without a Java shell and JNI. Until that shell exists, every dialog here picks nothing and
//! leaves a note, which the window shows in its status line, so a button that needs a picker
//! says why nothing happened instead of doing nothing.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static NOTICE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The note left by the last dialog that could not open, taken once.
pub fn take_notice() -> Option<String> {
    NOTICE.with(|notice| notice.borrow_mut().take())
}

fn unavailable() {
    let note = crate::tl_id!("ui.android_no_file_picker").to_string();
    log::info!("file dialog requested on Android: {note}");
    NOTICE.with(|notice| *notice.borrow_mut() = Some(note));
}

/// The subset of `rfd::FileDialog` that NoteCraft uses.
#[derive(Default)]
pub struct FileDialog;

impl FileDialog {
    pub fn new() -> FileDialog {
        FileDialog
    }
    pub fn set_title(self, _title: impl Into<String>) -> FileDialog {
        self
    }
    pub fn set_directory(self, _path: impl AsRef<Path>) -> FileDialog {
        self
    }
    pub fn set_file_name(self, _name: impl Into<String>) -> FileDialog {
        self
    }
    pub fn add_filter(self, _name: impl Into<String>, _extensions: &[impl ToString]) -> FileDialog {
        self
    }
    pub fn pick_file(self) -> Option<PathBuf> {
        unavailable();
        None
    }
    pub fn pick_folder(self) -> Option<PathBuf> {
        unavailable();
        None
    }
    pub fn save_file(self) -> Option<PathBuf> {
        unavailable();
        None
    }
}
