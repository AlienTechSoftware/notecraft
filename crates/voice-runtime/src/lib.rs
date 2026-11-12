#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Host-independent capture and transcription. Providers never receive notebook access.

pub mod audio;
#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
mod capture;
mod runtime;
mod temporary;
pub mod whisper;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub use runtime::{Runtime, RuntimeEvent, microphone_devices};
pub use whisper::WhisperConfig;

/// A process/provider can observe cancellation without blocking its owner.
#[derive(Clone, Default)]
pub struct Cancellation {
    cancelled: Arc<AtomicBool>,
    interrupts: Vec<Arc<AtomicBool>>,
}

impl Cancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || self.interrupts.iter().any(|x| x.load(Ordering::Acquire))
    }
    #[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
    pub(crate) fn interrupted_by(&self, flag: Arc<AtomicBool>) -> Self {
        let mut next = self.clone();
        next.interrupts.push(flag);
        next
    }
}

/// Implementations recognize audio only. Authorization and edits stay in the host.
///
/// Operations must observe cancellation promptly and bound blocking I/O and output. Runtime
/// drops join workers to release capture and process files. A provider that ignores
/// cancellation can block its owner on drop. The bundled Whisper provider polls cancellation
/// every 10 ms and enforces a timeout.
pub trait Transcriber: Send + 'static {
    fn capabilities(&self) -> craft_voice_core::Capabilities;
    fn prepare(&mut self, cancel: &Cancellation) -> Result<(), String>;
    fn transcribe(&mut self, audio: &audio::PcmAudio, cancel: &Cancellation) -> Result<String, String>;
}
