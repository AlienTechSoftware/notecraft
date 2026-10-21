//! Shows a Mermaid diagram in the window.
//!
//! The drawing is made by `notecraft-diagram` as an SVG picture, in the page's colours, and
//! shown through egui's SVG loader. Native builds draw on a thread of their own. A browser's
//! single-threaded WebAssembly build draws smaller, bounded diagrams in the current frame. The
//! result is kept by source and colours, so an unchanged diagram is not drawn again, and a theme
//! change draws it again in the new colours.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
#[cfg(any(not(target_arch = "wasm32"), test))]
use std::time::Duration;

use egui::{Color32, Ui, vec2};
use notecraft_diagram::{Palette, draw};
// std's own Instant panics on wasm32-unknown-unknown, which has no clock; web_time is the
// same type everywhere else and performance.now()/Date.now() there.
#[cfg(any(not(target_arch = "wasm32"), test))]
use web_time::Instant;

use crate::theme::{Tokens, mix};

/// How many drawings are kept. Past this the cache starts again, which only means drawing again.
const CACHE_LIMIT: usize = 64;
/// A drawing still not ready after this long is given up on in the window. Its thread carries on
/// until the renderer returns, but the window stops waiting for it.
#[cfg(not(target_arch = "wasm32"))]
const GIVE_UP: Duration = Duration::from_secs(8);

enum Slot {
    #[cfg(not(target_arch = "wasm32"))]
    Drawing(Instant),
    /// The picture's bytes, and its own width and height.
    Ready(Arc<[u8]>, f32, f32),
    Failed(String),
}

type Cache = Arc<Mutex<HashMap<u64, Slot>>>;

/// What showing a diagram came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shown {
    /// The picture is on the screen.
    Drawn,
    /// It is being drawn; the window is asked to look again shortly.
    Drawing,
    /// It could not be drawn, and why. The source is still the page's.
    Failed(String),
}

fn hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// The page's colours, as a diagram wants them.
pub fn palette(t: &Tokens) -> Palette {
    Palette {
        background: hex(t.editor),
        fill: hex(mix(t.editor, t.accent, 0.14)),
        border: hex(t.accent),
        text: hex(t.text),
        line: hex(mix(t.editor, t.text, 0.6)),
        label_background: hex(t.editor),
        cluster_fill: hex(mix(t.editor, t.accent, 0.06)),
        cluster_border: hex(mix(t.editor, t.accent, 0.4)),
    }
}

fn key_of(source: &str, palette: &Palette) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    palette.hash(&mut h);
    h.finish()
}

fn cache(ctx: &egui::Context) -> Cache {
    let id = egui::Id::new("notecraft-diagram-cache");
    ctx.data_mut(|d| d.get_temp_mut_or_insert_with::<Cache>(id, || Arc::new(Mutex::new(HashMap::new()))).clone())
}

fn render_slot(source: &str, palette: &Palette) -> Slot {
    match draw(source, palette) {
        Ok(svg) => {
            let (w, h) = notecraft_diagram::size(&svg).unwrap_or((1.0, 1.0));
            Slot::Ready(Arc::from(svg.into_bytes()), w, h)
        }
        Err(why) => Slot::Failed(why),
    }
}

#[cfg(target_arch = "wasm32")]
fn shown(slot: &Slot) -> Shown {
    match slot {
        Slot::Ready(..) => Shown::Drawn,
        Slot::Failed(why) => Shown::Failed(why.clone()),
    }
}

/// What the cache says about a diagram, starting a drawing when it knows nothing.
pub fn state(ctx: &egui::Context, source: &str, palette: &Palette) -> Shown {
    let key = key_of(source, palette);
    let cache = cache(ctx);
    let Ok(mut slots) = cache.lock() else { return Shown::Failed("The drawings cannot be kept".to_string()) };
    match slots.get(&key) {
        Some(Slot::Ready(..)) => return Shown::Drawn,
        Some(Slot::Failed(why)) => return Shown::Failed(why.clone()),
        #[cfg(not(target_arch = "wasm32"))]
        Some(Slot::Drawing(since)) if since.elapsed() > GIVE_UP => {
            return Shown::Failed("The diagram took too long to draw".to_string());
        }
        #[cfg(not(target_arch = "wasm32"))]
        Some(Slot::Drawing(_)) => {
            ctx.request_repaint_after(Duration::from_millis(100));
            return Shown::Drawing;
        }
        None => {}
    }
    if slots.len() >= CACHE_LIMIT {
        slots.clear();
    }
    #[cfg(target_arch = "wasm32")]
    {
        let slot = render_slot(source, palette);
        let result = shown(&slot);
        slots.insert(key, slot);
        return result;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        slots.insert(key, Slot::Drawing(Instant::now()));
        let (source, palette, done, ctx2) = (source.to_owned(), palette.clone(), cache.clone(), ctx.clone());
        let started = std::thread::Builder::new().name("diagram".into()).spawn(move || {
            let slot = render_slot(&source, &palette);
            if let Ok(mut slots) = done.lock() {
                slots.insert(key, slot);
            }
            ctx2.request_repaint();
        });
        if started.is_err() {
            slots.insert(key, Slot::Failed("The diagram could not be drawn".to_string()));
            return Shown::Failed("The diagram could not be drawn".to_string());
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        Shown::Drawing
    }
}

/// How many of `sources` are drawn, being drawn, and could not be, as the cache stands. It starts
/// nothing: it is for looking, from a script or a test.
pub fn counts(ctx: &egui::Context, sources: &[String], t: &Tokens) -> (usize, usize, usize) {
    let palette = palette(t);
    let cache = cache(ctx);
    let Ok(slots) = cache.lock() else { return (0, 0, sources.len()) };
    let (mut drawn, mut drawing, mut failed) = (0, 0, 0);
    for source in sources {
        match slots.get(&key_of(source, &palette)) {
            Some(Slot::Ready(..)) => drawn += 1,
            Some(Slot::Failed(_)) => failed += 1,
            _ => drawing += 1,
        }
    }
    (drawn, drawing, failed)
}

/// Draws the diagram `source` in `ui`, as wide as its own size or the room allows, and says what
/// came of it. The caller scrolls: a tall drawing is not squeezed to fit.
pub fn show(ui: &mut Ui, source: &str, t: &Tokens) -> Shown {
    show_zoomed(ui, source, t, 1.0)
}

/// Draws the diagram at page zoom while keeping the available layout and ribbon unchanged.
pub fn show_zoomed(ui: &mut Ui, source: &str, t: &Tokens, zoom: f32) -> Shown {
    let palette = palette(t);
    let shown = state(ui.ctx(), source, &palette);
    match &shown {
        Shown::Drawn => {
            let key = key_of(source, &palette);
            let ready = cache(ui.ctx()).lock().ok().and_then(|c| match c.get(&key) {
                Some(Slot::Ready(b, w, h)) => Some((b.clone(), *w, *h)),
                _ => None,
            });
            if let Some((bytes, w, h)) = ready {
                let uri = format!("bytes://diagram-{key:016x}.svg");
                // At its own size, so its writing is the size it was set at; made smaller only
                // when it is wider than the room, or when its picture would be bigger than the
                // graphics card can hold (a window that asks for that panics). The size is given
                // outright, so the loader never works one out for itself.
                let (ppp, side) = (ui.ctx().pixels_per_point(), ui.ctx().input(|i| i.max_texture_side) as f32);
                let scale = notecraft_diagram::fit(w * zoom, h * zoom, ui.available_width(), side, ppp);
                ui.add(egui::Image::from_bytes(uri, egui::load::Bytes::Shared(bytes)).fit_to_exact_size(vec2(w * zoom * scale, h * zoom * scale)));
            }
        }
        Shown::Drawing => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new("Drawing...").color(t.muted));
            });
        }
        Shown::Failed(why) => {
            ui.label(egui::RichText::new(format!("Not drawn: {why}")).small().color(t.warn));
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diagram_is_drawn_off_the_frame_and_then_kept() {
        let ctx = egui::Context::default();
        let pal = Palette::default();
        let src = "flowchart TD\n  A[Start] --> B[Done]";
        // The first look starts the drawing and does not wait for it.
        #[cfg(not(target_arch = "wasm32"))]
        assert_eq!(state(&ctx, src, &pal), Shown::Drawing);
        #[cfg(target_arch = "wasm32")]
        assert_eq!(state(&ctx, src, &pal), Shown::Drawn);
        let began = Instant::now();
        let end = loop {
            match state(&ctx, src, &pal) {
                Shown::Drawing if began.elapsed() < Duration::from_secs(20) => std::thread::sleep(Duration::from_millis(10)),
                other => break other,
            }
        };
        assert_eq!(end, Shown::Drawn, "a flowchart is drawn");
        assert_eq!(state(&ctx, src, &pal), Shown::Drawn, "and kept");
    }

    #[test]
    fn a_broken_diagram_fails_with_a_reason_and_is_remembered_as_failed() {
        let ctx = egui::Context::default();
        let pal = Palette::default();
        let src = "flowchart TD\n  A[unclosed --> B(\n  C -->\n  graph ??? \n";
        let began = Instant::now();
        let end = loop {
            match state(&ctx, src, &pal) {
                Shown::Drawing if began.elapsed() < Duration::from_secs(20) => std::thread::sleep(Duration::from_millis(10)),
                other => break other,
            }
        };
        assert!(matches!(end, Shown::Failed(ref why) if !why.is_empty()), "{end:?}");
    }

    #[test]
    fn the_palette_follows_the_theme() {
        let (light, dark) =
            (crate::theme::Theme::DEFAULT.tokens(), crate::theme::Theme { mode: crate::theme::Mode::Dark, ..crate::theme::Theme::DEFAULT }.tokens());
        let (a, b) = (palette(&light), palette(&dark));
        assert_ne!(a.background, b.background);
        assert_ne!(a.text, b.text);
        assert!(a.background.starts_with('#') && a.background.len() == 7);
        assert_ne!(key_of("x", &a), key_of("x", &b), "a theme change draws again");
    }
}
