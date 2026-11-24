//! Copy Text from Picture (issue #274): the printed text of a picture, or of one or every page
//! of a printout, shown for review before it is copied.
//!
//! The text comes from [`notecraft_engine::picture_text`]: what was read when the page was
//! opened, while the picture is unchanged, or else the picture read now with the models on this
//! computer (the setting `ocr_models`). Nothing is uploaded, and the pictures are never changed.
//! Reading runs off the window's thread; the dialog shows how far it has got, and says so plainly
//! when recognition is not set up, when a picture cannot be read, or when no text was found.

use crate::theme::Tokens;
use egui::{Align2, Context, vec2};
use notecraft_engine::PictureTextError;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

/// What the text is taken from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// One picture.
    Picture,
    /// One page of a printout.
    PrintoutPage,
    /// Every page of a printout, in order.
    Printout,
}

/// The reading, to run on a thread of its own or, where there is none, at once.
type Work = Box<dyn FnOnce() + Send>;

/// News from the reading thread.
enum News {
    /// This many pictures are read.
    Read(usize),
    /// Every picture is read: their texts in order, or why not.
    Done(Result<Vec<String>, PictureTextError>),
}

/// Where the dialog is.
#[derive(Debug, PartialEq)]
enum State {
    Reading {
        done: usize,
    },
    /// The text, for review. The person may change it before copying.
    Ready,
    /// No text was found.
    Empty,
    NotSetUp,
    Failed(String),
}

/// The dialog while it is open.
pub struct Dialog {
    scope: Scope,
    pictures: Vec<PathBuf>,
    total: usize,
    state: State,
    news: Option<Receiver<News>>,
    /// The text as read, then as the person edits it.
    pub text: String,
    copied: bool,
}

/// What a frame of the dialog asked for.
#[derive(Default, Debug, PartialEq)]
pub struct Outcome {
    pub closed: bool,
    /// Put this text on the clipboard.
    pub copy: Option<String>,
    /// Ask for the folder with the recognition models, then read again.
    pub choose_models: bool,
}

impl Dialog {
    /// Starts reading `pictures` (their files, in order) with the models in `models`.
    pub fn start(scope: Scope, pictures: Vec<PathBuf>, models: Option<PathBuf>) -> Dialog {
        let total = pictures.len();
        let (tell, news) = std::sync::mpsc::channel();
        let work: Work = {
            let pictures = pictures.clone();
            Box::new(move || {
                let mut texts = Vec::with_capacity(pictures.len());
                for (i, picture) in pictures.iter().enumerate() {
                    match notecraft_engine::picture_text(models.as_deref(), picture) {
                        Ok(text) => texts.push(text),
                        Err(e) => {
                            let _ = tell.send(News::Done(Err(e)));
                            return;
                        }
                    }
                    let _ = tell.send(News::Read(i + 1));
                }
                let _ = tell.send(News::Done(Ok(texts)));
            })
        };
        // A browser has no threads to start: there the pictures are read at once.
        let work = std::sync::Arc::new(std::sync::Mutex::new(Some(work)));
        let run = |work: &std::sync::Mutex<Option<Work>>| {
            if let Some(work) = work.lock().ok().and_then(|mut w| w.take()) {
                work();
            }
        };
        let in_thread = std::sync::Arc::clone(&work);
        if std::thread::Builder::new().name("notecraft-picture-text".into()).spawn(move || run(&in_thread)).is_err() {
            run(&work);
        }
        let mut dialog = Dialog { scope, pictures, total, state: State::Reading { done: 0 }, news: Some(news), text: String::new(), copied: false };
        dialog.take_news();
        dialog
    }

    /// The pictures being read, for reading them again with other models.
    pub fn pictures(&self) -> &[PathBuf] {
        &self.pictures
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Whether the pictures are still being read.
    pub fn reading(&self) -> bool {
        matches!(self.state, State::Reading { .. })
    }

    /// Whether recognition was found not set up.
    pub fn not_set_up(&self) -> bool {
        self.state == State::NotSetUp
    }

    /// The text was put on the clipboard.
    pub fn mark_copied(&mut self) {
        self.copied = true;
    }

    /// Takes what the reading thread has said since the last frame.
    fn take_news(&mut self) {
        let Some(news) = &self.news else { return };
        loop {
            match news.try_recv() {
                Ok(News::Read(done)) => self.state = State::Reading { done },
                Ok(News::Done(result)) => {
                    self.news = None;
                    self.state = match result {
                        Ok(texts) => {
                            self.text = join(&texts, self.scope);
                            if self.text.trim().is_empty() { State::Empty } else { State::Ready }
                        }
                        Err(PictureTextError::NotSetUp) => State::NotSetUp,
                        Err(PictureTextError::Failed(why)) => State::Failed(why),
                    };
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.news = None;
                    self.state = State::Failed(crate::tl_id!("ui.picture_text_reading_stopped").to_string());
                    return;
                }
            }
        }
    }

    pub fn show(&mut self, ctx: &Context, t: &Tokens) -> Outcome {
        self.take_news();
        if self.reading() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut out = Outcome::default();
        let backdrop = crate::app::modal_backdrop(ctx, "picture-text-modal-backdrop");
        let title = match self.scope {
            Scope::Picture => crate::tl_id!("ui.copy_text_from_picture"),
            Scope::PrintoutPage => crate::tl_id!("ui.copy_text_from_this_printout_page"),
            Scope::Printout => crate::tl_id!("ui.copy_text_from_all_printout_pages"),
        };
        let screen = ctx.content_rect();
        let width = (screen.width() - 52.0).clamp(1.0, 560.0);
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(crate::app::modal_frame(t))
            .max_width(width)
            .show(ctx, |ui| {
                crate::app::stay_above(ui, backdrop);
                // A fixed width: the text arriving must not move the buttons under the pointer.
                ui.set_width(width);
                crate::app::modal_title(ui, t, title);
                match &self.state {
                    State::Reading { done } => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            if self.total > 1 {
                                ui.label(crate::i18n::format_message_id(
                                    "ui.picture_text_reading_page_of",
                                    &[("done", &(done + 1).min(self.total).to_string()), ("total", &self.total.to_string())],
                                ));
                            } else {
                                ui.label(crate::tl_id!("ui.picture_text_reading"));
                            }
                        });
                    }
                    State::Ready => {
                        let label = ui.label(crate::tl_id!("ui.picture_text_review"));
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical().max_height((screen.height() * 0.5).clamp(120.0, 360.0)).show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.text).desired_width(f32::INFINITY).desired_rows(8).id_salt("picture-text-edit"),
                            )
                            .labelled_by(label.id);
                        });
                    }
                    State::Empty => {
                        ui.label(match self.scope {
                            Scope::Printout => crate::tl_id!("ui.picture_text_none_in_printout"),
                            _ => crate::tl_id!("ui.picture_text_none_found"),
                        });
                    }
                    State::NotSetUp => {
                        ui.label(crate::tl_id!("ui.picture_text_not_set_up"));
                    }
                    State::Failed(why) => {
                        ui.colored_label(t.warn, crate::i18n::format_message_id("ui.picture_text_failed", &[("error", why)]));
                    }
                }
                ui.add_space(8.0);
                ui.weak(crate::tl_id!("ui.picture_text_language"));
                if self.copied {
                    ui.label(crate::tl_id!("ui.picture_text_copied"));
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if crate::app::modal_button(
                        ui,
                        t,
                        crate::tl_id!("ui.close"),
                        matches!(self.state, State::Reading { .. } | State::Empty | State::Failed(_)),
                    )
                    .clicked()
                    {
                        out.closed = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.state == State::Ready && crate::app::modal_button(ui, t, crate::tl_id!("ui.copy_text"), true).clicked() {
                            out.copy = Some(self.text.clone());
                        }
                        if self.state == State::NotSetUp
                            && crate::app::modal_button(ui, t, crate::tl_id!("ui.choose_the_models_folder"), true).clicked()
                        {
                            out.choose_models = true;
                        }
                    });
                });
            });
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            out.closed = true;
        }
        out
    }
}

/// The texts of the pictures as one text. The pages of a printout are kept apart by a blank line.
fn join(texts: &[String], scope: Scope) -> String {
    let texts: Vec<&str> = texts.iter().map(|t| t.trim_end()).collect();
    match scope {
        Scope::Printout => texts.into_iter().filter(|t| !t.trim().is_empty()).collect::<Vec<_>>().join("\n\n"),
        _ => texts.join("\n"),
    }
}

/// The printout a picture is a page of: the file it was made from, when `src` names a printout
/// page (`assets/report.pdf.printout-002.png`).
pub fn printout_source(src: &str) -> Option<&str> {
    let (source, page) = src.rsplit_once(".printout-")?;
    let number = page.strip_suffix(".png")?;
    (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())).then_some(source)
}

/// Whether a models folder has the two files recognition needs.
pub fn models_folder_is_complete(folder: &std::path::Path) -> bool {
    folder.join("text-detection.onnx").is_file() && folder.join("text-recognition.onnx").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printout_pages_are_told_from_other_pictures() {
        assert_eq!(printout_source("assets/report.pdf.printout-002.png"), Some("assets/report.pdf"));
        assert_eq!(printout_source("assets/photo.png"), None);
        assert_eq!(printout_source("assets/report.pdf.printout-.png"), None);
        assert_eq!(printout_source("assets/report.pdf.printout-2a.png"), None);
    }

    #[test]
    fn printout_pages_are_kept_apart_and_empty_pages_dropped() {
        let texts = vec!["Page one\n".to_string(), String::new(), "Page three".to_string()];
        assert_eq!(join(&texts, Scope::Printout), "Page one\n\nPage three");
        assert_eq!(join(&["Total 4.50\n".to_string()], Scope::Picture), "Total 4.50");
    }
}
