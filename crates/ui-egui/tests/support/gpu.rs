//! One offscreen GPU device for a whole test binary.
//!
//! `Harness::builder().wgpu()` creates a new wgpu instance, adapter, device and queue for every
//! window. The UI suite opens some two hundred windows, so it spent much of its time asking the
//! driver for devices: on the hosted macOS runners that made the suite several times slower than
//! on Linux. The same fix the icon tests use (`icons_visual.rs`) applies here: the adapter,
//! device and queue are shared, and each window still gets its own `Renderer` with its own
//! textures and pipelines, so nothing drawn in one test can leak into another.
//!
//! Reading a picture back from that shared device is done one window at a time: two readbacks in
//! flight on the same queue at once came back blank (the highlight and ink tests). Take a picture
//! with [`render`], never `Harness::render` directly. Only the readback waits its turn; building
//! windows, clicking and typing still run on every test thread, which is most of the suite's time.

use egui_kittest::Harness;
use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state, default_wgpu_setup};
use egui_wgpu::{RenderState, Renderer, RendererOptions};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

fn shared() -> &'static RenderState {
    static STATE: OnceLock<RenderState> = OnceLock::new();
    STATE.get_or_init(|| create_render_state(default_wgpu_setup(), RendererOptions::PREDICTABLE))
}

/// A fresh renderer on the shared device, for `HarnessBuilder::renderer`. It renders exactly as
/// `HarnessBuilder::wgpu()` does: same adapter choice, same `RendererOptions::PREDICTABLE`.
pub fn renderer() -> WgpuTestRenderer {
    let base = shared();
    let renderer = Renderer::new(&base.device, base.target_format, RendererOptions::PREDICTABLE);
    WgpuTestRenderer::from_render_state(RenderState { renderer: Arc::new(egui::mutex::RwLock::new(renderer)), ..base.clone() })
}

/// The window's picture, read back while no other test is reading one from the shared device.
pub fn render<State>(h: &mut Harness<'_, State>) -> Result<image::RgbaImage, String> {
    static READBACK: Mutex<()> = Mutex::new(());
    // A test that panicked mid-readback leaves the device as usable as before; the next one goes on.
    let _turn = READBACK.lock().unwrap_or_else(PoisonError::into_inner);
    h.render()
}
