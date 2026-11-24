//! Window requests, independent views, and recovery of concurrent page edits.

use super::*;

impl NoteCraftApp {
    /// Native viewports share one egui context; each view must restore its own widget theme.
    pub(crate) fn prepare_native_view(&mut self) {
        self.applied_theme = None;
        self.native_hosted = true;
    }

    pub(crate) fn take_native_close(&self) -> bool {
        self.native_close.replace(false)
    }

    pub(crate) fn request_window_close(&self, ctx: &Context) {
        if self.native_hosted {
            self.native_close.set(true);
            ctx.request_repaint();
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Requests are consumed by the native host, which owns window lifetime.
    pub(crate) fn control_enabled(&self) -> bool {
        self.control.is_some()
    }

    pub(crate) fn start_window_control(&mut self, ctx: Context) -> Result<(), String> {
        let key_dir = self.session.settings.path().parent().ok_or("settings file has no parent folder")?;
        let (server, receiver) = crate::control::start(0, key_dir, ctx)?;
        self.control_server = Some(server);
        self.control = Some(receiver);
        Ok(())
    }

    pub(crate) fn has_window_requests(&self) -> bool {
        !self.window_requests.is_empty()
    }

    pub fn take_window_requests(&mut self) -> Vec<crate::windows::Request> {
        std::mem::take(&mut self.window_requests)
    }

    pub fn current_page(&self) -> Option<&Path> {
        self.editor.as_ref().map(|ed| ed.dir.as_path())
    }
    pub fn has_page_conflict(&self) -> bool {
        self.page_conflict
    }

    pub(super) fn request_window(&mut self, quick: bool) {
        if cfg!(any(target_arch = "wasm32", target_os = "android", target_os = "ios")) {
            self.say(crate::tl_id!("ui.window_native_only"));
            return;
        }
        self.save_now();
        if self.is_dirty() || self.page_conflict {
            return;
        }
        let p = self.current_page().map(|p| params(&[("path", &path_str(p))])).unwrap_or_default();
        let id = if quick { "window.quick-note" } else { "window.new" };
        match self.session.run(id, &if quick { Params::new() } else { p }) {
            Ok(Output::Path(path)) => self.window_requests.push(crate::windows::Request { path, quick }),
            Ok(_) => {}
            Err(e) => self.fail(&e),
        }
    }

    /// Build another independent view using the same notebook and settings boundaries.
    pub fn child_window(&self, request: &crate::windows::Request) -> Result<Self, String> {
        let session = Session::new(self.session.root(), self.session.settings.clone()).map_err(|e| e.to_string())?;
        let mut child = Self::new(session);
        child.secondary_window = true;
        child.restored = true;
        child.frameless = false;
        child.ui_scale_pending = false;
        if request.quick {
            child.full_page = true;
            child.ribbon_collapsed = true;
        }
        if notecraft_store::tree::page_file(&request.path).is_some() {
            child.open_path(&path_str(&request.path));
            if child.status_is_error {
                return Err(child.status.clone());
            }
        }
        Ok(child)
    }

    /// Finish the title after the body, since a matching first heading is also renamed.
    pub(super) fn save_pending_title(&mut self) {
        let Some((dir, title, expected)) = self
            .editor
            .as_ref()
            .filter(|ed| ed.title_buf.trim() != if ed.titled { ed.title.as_str() } else { "" })
            .map(|ed| (ed.dir.clone(), ed.title_buf.trim().to_string(), ed.saved_body.clone()))
        else {
            return;
        };
        match self.session.run_with_page("node.title", &params(&[("path", &path_str(&dir)), ("title", &title)]), &dir, &expected) {
            Ok((_, snapshot)) => {
                if let Some(ed) = &mut self.editor {
                    ed.titled = !title.is_empty();
                    ed.title = if title.is_empty() { notecraft_store::tree::UNTITLED_PAGE.to_string() } else { title };
                    if let Some(page) = &snapshot {
                        ed.saved_body = page.body.clone();
                        ed.body = page.body.clone();
                        ed.blocks = notecraft_formats::boxes::read(ed.format, &ed.body).0;
                        if let Some(caret) = &mut ed.caret
                            && caret.cell.is_none()
                            && let Some(block) = ed.blocks.get(caret.block)
                        {
                            caret.text = pageedit::caret_text(block);
                            caret.inline = None;
                        }
                    }
                }
                self.page_revision = snapshot.and_then(|s| s.revision);
            }
            Err(e) => self.fail(&e),
        }
    }

    /// Detect another view's writes before dispatching any input to this view.
    pub fn check_page_changes(&mut self) {
        if cfg!(any(target_arch = "wasm32", target_os = "android", target_os = "ios")) {
            return;
        }
        // This window's own writes under way are not another's: looked at once they are done,
        // and external reloads wait until any in-progress stroke finishes.
        if self.page_conflict
            || self.saver.busy()
            || self.ink_saving.is_some()
            || self.page_revision.is_none()
            || self.pad.as_ref().is_some_and(crate::inkpad::Pad::stroke_in_progress)
        {
            return;
        }
        if self.pad.as_ref().is_some_and(|pad| pad.dirty && !pad.matches_disk()) {
            self.page_conflict = true;
            return;
        }
        let Some(ed) = &self.editor else {
            return;
        };
        let dir = ed.dir.clone();
        let revision = crate::windows::Revision::read(&dir);
        if let Ok(now) = &revision
            && self.page_revision.as_ref() == Some(now)
        {
            return;
        }
        let unavailable = match &revision {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => {
                self.page_conflict = true;
                return;
            }
            Ok(_) => !Self::editor_matches_location(ed),
        };
        if unavailable {
            if self.is_dirty() {
                self.page_conflict = true;
                return;
            }
            if let Some(moved) = self.live_page_location() {
                self.nav.relocate(&dir, &moved);
                self.expand_to(&moved);
                self.reload_clean_view(&moved);
            } else {
                // A clean view has no edits to recover. A deleted page must not trap
                // its window in a dialog that requires a file that no longer exists.
                self.selected = notebook_for(&dir);
                self.editor = None;
                self.pad = None;
                self.page_revision = None;
            }
            return;
        }
        if let Ok(now) = revision
            && self.page_revision.as_ref().is_some_and(|old| old.same_page(&now))
            && self.pad.as_ref().is_some_and(crate::inkpad::Pad::matches_disk)
        {
            // This view just saved its ink, including Undo/Redo before a body save.
            self.page_revision = Some(now);
            return;
        }
        if self.is_dirty() {
            self.page_conflict = true;
        } else {
            self.reload_clean_view(&dir);
        }
    }

    fn editor_matches_location(ed: &Editor) -> bool {
        notecraft_store::tree::read_node(&ed.dir)
            .is_some_and(|node| node.kind == NodeKind::Page && ed.page_id.as_ref().is_none_or(|id| id == &node.id))
    }

    fn live_page_location(&mut self) -> Option<PathBuf> {
        let editor = self.editor.as_ref()?;
        if Self::editor_matches_location(editor) {
            return Some(editor.dir.clone());
        }
        let id = editor.page_id.clone()?;
        self.session.refresh();
        let mut remaining: Vec<_> = self.session.tree().iter().collect();
        let mut found = None;
        while let Some(node) = remaining.pop() {
            if node.info.kind == NodeKind::Page && node.info.id == id {
                if found.is_some() {
                    // Do not pick between two copies with an ambiguous identity.
                    return None;
                }
                found = Some(node.path.clone());
            }
            remaining.extend(&node.children);
        }
        found
    }

    fn reload_clean_view(&mut self, dir: &Path) {
        let zoom = self.zoom_percent;
        let pan = self.canvas_pan;
        let caret = self.editor.as_ref().and_then(|e| e.caret.clone());
        self.page_revision = None;
        self.open_page_inner(dir);
        self.zoom_percent = zoom;
        self.canvas_pan = pan;
        if let Some(ed) = &mut self.editor {
            ed.caret = caret.and_then(|mut c| {
                let block = ed.blocks.get(c.block)?;
                if c.cell.is_some() || !pageedit::is_writable(block) {
                    return None;
                }
                c.text = pageedit::caret_text(block);
                // The refreshed spans own formatting and literal identity too.
                c.inline = None;
                let count = c.text.chars().count();
                c.selection = (c.selection.0.min(count), c.selection.1.min(count));
                c.place = c.place.map(|p| p.min(count));
                Some(c)
            });
        }
    }

    /// Explicitly discard local unsaved changes and show the latest disk version.
    pub fn reload_conflicted_page(&mut self) -> bool {
        let Some(dir) = self.live_page_location() else {
            return false;
        };
        if notecraft_store::page::load_page(&dir).is_err() {
            return false;
        }
        if let Some(ed) = &mut self.editor {
            ed.dirty_since = None;
        }
        if let Some(pad) = &mut self.pad {
            pad.dirty = false;
        }
        if let Some(old) = self.current_page().map(Path::to_path_buf) {
            self.nav.relocate(&old, &dir);
        }
        self.expand_to(&dir);
        self.page_conflict = false;
        self.page_revision = None;
        self.open_page_inner(&dir);
        !self.status_is_error
    }

    /// Preserve this editor and its ink as an ordinary sibling page without replacing the newer page.
    pub fn keep_conflicted_copy(&mut self) -> Result<PathBuf, String> {
        let dir = self.live_page_location().ok_or("the original page is no longer available")?;
        let parent = dir.parent().ok_or("the page has no section")?;
        let Output::Path(copy) =
            self.session.run("node.copy", &params(&[("path", &path_str(&dir)), ("to", &path_str(parent))])).map_err(|e| e.to_string())?
        else {
            return Err("the copy was not created".into());
        };
        let Some(ed) = &mut self.editor else {
            return Err("no page is open".into());
        };
        let body = ed.body_to_save();
        notecraft_store::page::save_body(&copy, &body).map_err(|e| e.to_string())?;
        self.session.run("node.title", &params(&[("path", &path_str(&copy)), ("title", ed.title_buf.trim())])).map_err(|e| e.to_string())?;
        if let Some(pad) = &self.pad {
            pad.save_copy_to(&copy).map_err(|e| e.to_string())?;
        }
        ed.dirty_since = None;
        self.page_conflict = false;
        self.page_revision = None;
        self.open_page(&copy);
        Ok(copy)
    }

    pub(super) fn conflict_screen(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.poll_control(&ctx);
        self.apply_theme(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.heading(crate::tl_id!("ui.window_conflict_title"));
            ui.label(crate::tl_id!("ui.window_conflict_detail"));
            if ui.button(crate::tl_id!("ui.window_keep_copy")).clicked()
                && let Err(e) = self.keep_conflicted_copy()
            {
                self.status = e;
                self.status_is_error = true;
            }
            if ui.button(crate::tl_id!("ui.window_reload")).clicked() {
                self.reload_conflicted_page();
            }
            if self.status_is_error {
                ui.label(egui::RichText::new(&self.status).color(self.tokens.warn));
            }
        });
    }
}
