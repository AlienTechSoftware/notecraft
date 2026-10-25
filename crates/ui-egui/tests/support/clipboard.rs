//! A test double for the system clipboard. It stands in for the platform clipboard behind the
//! `Board` boundary, so a test that uses it neither reads nor writes the clipboard the user
//! shares with other programs. This is the one exemption to the no-mocks rule (see `AGENTS.md`);
//! it lives here, in the test folder, and product code has no hook for it.

use notecraft_ui_egui::clipboard::{Board, Picture};
use std::sync::{Arc, Mutex};

/// An in-memory clipboard. Clones share one clipboard, so a test keeps one clone to put text on
/// it and gives the other to the app.
#[derive(Clone, Default)]
pub struct MemoryBoard(Arc<Mutex<Held>>);

#[derive(Default)]
struct Held {
    text: Option<String>,
    html: Option<String>,
}

impl MemoryBoard {
    pub fn new() -> MemoryBoard {
        MemoryBoard::default()
    }

    /// Puts plain text on the clipboard, as another program would.
    pub fn put_text(&self, text: &str) {
        if let Ok(mut held) = self.0.lock() {
            held.text = Some(text.to_string());
            held.html = None;
        }
    }
}

impl Board for MemoryBoard {
    fn text(&mut self) -> Option<String> {
        self.0.lock().ok().and_then(|held| held.text.clone()).filter(|text| !text.is_empty())
    }

    fn html(&mut self) -> Option<String> {
        self.0.lock().ok().and_then(|held| held.html.clone())
    }

    fn picture(&mut self) -> Option<Picture> {
        None
    }

    fn has_picture(&mut self) -> bool {
        false
    }

    fn set_text(&mut self, text: &str) -> Result<(), String> {
        self.put_text(text);
        Ok(())
    }

    fn set_html(&mut self, html: &str, text: &str) -> Result<(), String> {
        if let Ok(mut held) = self.0.lock() {
            held.text = Some(text.to_string());
            held.html = Some(html.to_string());
        }
        Ok(())
    }
}
