//! Native note windows over main-thread-owned editors.
//!
//! Deferred viewport callbacks capture only an ID. The non-Send editor, clipboard and
//! platform handles stay on the event-loop thread. A hidden root remains the lifetime owner.

use crate::NoteCraftApp;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Request {
    pub path: PathBuf,
    pub quick: bool,
}

pub(crate) use notecraft_store::coordination::Revision;

struct Child {
    app: NoteCraftApp,
    closing: bool,
}
struct Window {
    id: egui::ViewportId,
    quick: bool,
}

thread_local! {
    static CHILDREN: RefCell<BTreeMap<egui::ViewportId, Child>> = const { RefCell::new(BTreeMap::new()) };
}

/// Owns the window identities; editor handles remain on the native event-loop thread.
pub struct Host {
    pub primary: NoteCraftApp,
    windows: Vec<Window>,
    next: u64,
    primary_closed: bool,
}

impl Host {
    pub fn new(mut primary: NoteCraftApp) -> Self {
        primary.prepare_native_view();
        Self { primary, windows: Vec::new(), next: 0, primary_closed: false }
    }

    fn with_view<R>(&mut self, id: egui::ViewportId, f: impl FnOnce(&mut NoteCraftApp) -> R) -> Option<R> {
        if id == egui::ViewportId::ROOT {
            return Some(f(&mut self.primary));
        }
        if !self.windows.iter().any(|w| w.id == id) {
            return None;
        }
        CHILDREN.with(|views| views.try_borrow_mut().ok()?.get_mut(&id).map(|child| f(&mut child.app)))
    }

    /// `plain`: Ctrl+Shift+V, the text only (issue #535).
    pub fn queue_keyboard_paste(&mut self, id: egui::ViewportId, plain: bool) {
        self.with_view(id, |app| if plain { app.queue_keyboard_paste_plain() } else { app.queue_keyboard_paste() });
    }

    pub fn take_events(&mut self, id: egui::ViewportId) -> Vec<egui::Event> {
        self.with_view(id, NoteCraftApp::take_events).unwrap_or_default()
    }

    fn add_window(&mut self, mut app: NoteCraftApp, quick: bool) {
        app.prepare_native_view();
        self.next += 1;
        let id = egui::ViewportId::from_hash_of(("note-window", self.next));
        CHILDREN.with(|views| views.borrow_mut().insert(id, Child { app, closing: false }));
        self.windows.push(Window { id, quick });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.primary_closed {
            self.primary.prepare_native_view();
            self.primary.show(ui);
        }
        self.show_windows(&ctx);
        if ctx.data_mut(|data| data.remove_temp::<bool>(egui::Id::new("update-process-close"))).unwrap_or(false) {
            self.request_update_close(&ctx);
        }
        // A child requested in this pass must exist before deciding whether the root may exit.
        if !self.primary_closed && (self.primary.take_native_close() || ctx.input(|i| i.viewport().close_requested())) {
            self.primary.save();
            if self.primary.is_dirty() || self.primary.has_page_conflict() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else if !self.windows.is_empty() {
                self.primary_closed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Save every editor before committing the process to an update restart.
    fn request_update_close(&mut self, ctx: &egui::Context) {
        self.primary.save();
        let mut ready = !self.primary.is_dirty() && !self.primary.has_page_conflict();
        CHILDREN.with(|views| {
            let mut views = views.borrow_mut();
            for child in views.values_mut() {
                child.app.save();
                ready &= !child.app.is_dirty() && !child.app.has_page_conflict();
            }
            if ready {
                for child in views.values_mut() {
                    child.closing = true;
                }
            }
        });
        if ready {
            self.primary.request_window_close(ctx);
            for window in &self.windows {
                ctx.send_viewport_cmd_to(window.id, egui::ViewportCommand::Close);
            }
        } else {
            self.primary_closed = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            self.primary.alert(
                "update-close",
                "NoteCraft",
                "Save or resolve conflicts in all note and Quick Note windows, then choose Restart to Update again.",
            );
        }
        ctx.request_repaint();
    }

    /// The logic pass queues a real render pass; it never initializes native viewports itself.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if self.primary_closed {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    fn show_windows(&mut self, ctx: &egui::Context) {
        let mut requests: Vec<_> = self.primary.take_window_requests().into_iter().map(|r| (egui::ViewportId::ROOT, r)).collect();
        let mut closed = Vec::new();
        CHILDREN.with(|views| {
            let mut views = views.borrow_mut();
            for window in &self.windows {
                if let Some(child) = views.get_mut(&window.id) {
                    requests.extend(child.app.take_window_requests().into_iter().map(|r| (window.id, r)));
                    if child.closing {
                        closed.push(window.id);
                    }
                }
            }
        });
        for (source, request) in requests {
            let created =
                self.with_view(source, |view| view.child_window(&request)).unwrap_or_else(|| Err("the note window is no longer available".into()));
            match created {
                Ok(mut child) => {
                    match crate::clipboard::open() {
                        Ok(board) => child = child.with_clipboard(board),
                        Err(e) => child.alert("clipboard-open", "NoteCraft", &e),
                    }
                    if self.primary.control_enabled()
                        && let Err(e) = child.start_window_control(ctx.clone())
                    {
                        child.alert("control-open", "NoteCraft", &e);
                    }
                    self.primary.session_mut().refresh();
                    CHILDREN.with(|views| {
                        for child in views.borrow_mut().values_mut() {
                            child.app.session_mut().refresh();
                        }
                    });
                    self.add_window(child, request.quick);
                    ctx.request_repaint();
                }
                Err(e) => {
                    self.with_view(source, |view| view.alert("window-open", "NoteCraft", &e));
                }
            }
        }
        self.windows.retain(|window| !closed.contains(&window.id));
        CHILDREN.with(|views| {
            let mut views = views.borrow_mut();
            for id in closed {
                views.remove(&id);
            }
        });
        for window in &self.windows {
            let id = window.id;
            let size = if window.quick { [520.0, 440.0] } else { [1100.0, 760.0] };
            let title = if window.quick { crate::tl_id!("ui.window_quick_title") } else { "NoteCraft" };
            ctx.show_viewport_deferred(
                id,
                egui::ViewportBuilder::default().with_title(title).with_inner_size(size).with_min_inner_size([360.0, 280.0]),
                move |ui, _class| {
                    render_child(id, ui);
                },
            );
        }
        if self.primary_closed && self.windows.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if !self.windows.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}

fn render_child(id: egui::ViewportId, ui: &mut egui::Ui) {
    // Each viewport polls peer changes and its own control channel independently.
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
    CHILDREN.with(|views| {
        let Ok(mut views) = views.try_borrow_mut() else {
            ui.label(crate::tl_id!("ui.window_owner_unavailable"));
            return;
        };
        let Some(child) = views.get_mut(&id) else {
            ui.label(crate::tl_id!("ui.window_owner_unavailable"));
            return;
        };
        if !child.closing {
            child.app.prepare_native_view();
            child.app.show(ui);
            if child.app.take_native_close() || ui.input(|i| i.viewport().close_requested()) {
                child.app.save();
                if child.app.is_dirty() || child.app.has_page_conflict() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
                } else {
                    child.closing = true;
                }
            }
        }
        if child.closing || child.app.has_window_requests() {
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}

impl Drop for Host {
    fn drop(&mut self) {
        CHILDREN.with(|views| {
            if let Ok(mut views) = views.try_borrow_mut() {
                for window in &self.windows {
                    views.remove(&window.id);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A managed child must save before the host disposes its native window.
    #[test]
    fn managed_close_saves_then_queues_without_a_backend_close_command() {
        let t = notecraft_store::testing::TempDir::new("managed-window-close");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = notecraft_engine::Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&sample.html_page.to_string_lossy());
        app.set_body("<p>retained before closing</p>");
        let mut host = Host::new(app);
        let ctx = egui::Context::default();
        let (request, reply) = crate::control::ControlRequest::new("app.quit", serde_json::json!({}));
        let mut request = Some(request);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            if let Some(request) = request.take() {
                host.primary.handle_control(ui.ctx(), request);
            }
        });
        output.textures_delta.clear();
        assert!(reply.recv().expect("reply").is_ok());
        assert!(host.primary.take_native_close(), "the host owns the queued close request");
        assert!(notecraft_store::page::load_page(&sample.html_page).expect("saved page").body.contains("retained before closing"));
        assert!(
            !output.viewport_output.values().any(|v| v.commands.iter().any(|c| matches!(c, egui::ViewportCommand::Close))),
            "the host must dispose of immediate viewports after the callback"
        );
    }
    #[test]
    fn update_close_requests_every_window() {
        let t = notecraft_store::testing::TempDir::new("update-window-close");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = notecraft_engine::Session::new(&t.path().join("notebooks"), settings).expect("session");
        let primary = NoteCraftApp::new(session);
        let child = primary.child_window(&Request { path: primary.session().root().to_path_buf(), quick: true }).expect("child");
        let mut host = Host::new(primary);
        host.add_window(child, true);
        let ctx = egui::Context::default();
        host.request_update_close(&ctx);
        assert!(host.primary.take_native_close());
        CHILDREN.with(|views| assert!(views.borrow().values().all(|child| child.closing)));
    }

    #[test]
    fn a_hidden_root_requests_a_render_pass_for_its_children() {
        let t = notecraft_store::testing::TempDir::new("hidden-window-render");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = notecraft_engine::Session::new(&t.path().join("notebooks"), settings).expect("session");
        let primary = NoteCraftApp::new(session);
        let child = primary.child_window(&Request { path: primary.session().root().to_path_buf(), quick: true }).expect("child");
        let mut host = Host::new(primary);
        host.primary_closed = true;
        host.add_window(child, true);
        let ctx = egui::Context::default();
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| host.logic(ctx));
        assert!(
            output.viewport_commands.values().flatten().any(|c| matches!(c, egui::ViewportCommand::RequestPaintWhileHidden)),
            "new native windows must be initialized from a full rendering pass"
        );
    }
}
