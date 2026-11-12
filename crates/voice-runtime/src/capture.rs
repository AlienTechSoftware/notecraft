//! CPAL owns native platform APIs. The realtime callback only copies into a bounded queue.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::Duration;

const CHUNK: usize = 4096;
pub(crate) struct AudioChunk {
    pub samples: [f32; CHUNK],
    pub len: usize,
}

pub(crate) struct Capture {
    _stream: cpal::Stream,
    rx: Receiver<AudioChunk>,
    overflow: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    pub rate: u32,
    pub channels: usize,
}

pub fn devices() -> Result<Vec<String>, String> {
    cpal::default_host()
        .input_devices()
        .map_err(|e| format!("cannot list microphones: {e}"))?
        .map(|d| d.name().map_err(|e| format!("cannot read microphone name: {e}")))
        .collect()
}

impl Capture {
    pub fn start(name: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = match name {
            Some(name) => host.input_devices().map_err(|e| e.to_string())?.find(|d| d.name().is_ok_and(|n| n == name)),
            None => host.default_input_device(),
        }
        .ok_or("no matching input device; check microphone permissions and device selection")?;
        let config = device.default_input_config().map_err(|e| format!("cannot open microphone: {e}"))?;
        let rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        if channels == 0 || channels > 64 {
            return Err("unsupported microphone channel count".into());
        }
        let (tx, rx) = mpsc::sync_channel(32);
        let overflow = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let stream_config: cpal::StreamConfig = config.clone().into();
        macro_rules! stream {
            ($ty:ty) => {
                build::<$ty>(&device, &stream_config, channels, tx, overflow.clone(), error.clone())?
            };
        }
        let stream = match config.sample_format() {
            cpal::SampleFormat::I8 => stream!(i8),
            cpal::SampleFormat::I16 => stream!(i16),
            cpal::SampleFormat::I32 => stream!(i32),
            cpal::SampleFormat::I64 => stream!(i64),
            cpal::SampleFormat::U8 => stream!(u8),
            cpal::SampleFormat::U16 => stream!(u16),
            cpal::SampleFormat::U32 => stream!(u32),
            cpal::SampleFormat::U64 => stream!(u64),
            cpal::SampleFormat::F32 => stream!(f32),
            cpal::SampleFormat::F64 => stream!(f64),
            _ => return Err("unsupported microphone sample format".into()),
        };
        stream.play().map_err(|e| format!("cannot start microphone: {e}"))?;
        Ok(Self { _stream: stream, rx, overflow, error, rate, channels })
    }

    pub fn read(&self) -> Result<Option<AudioChunk>, String> {
        if self.overflow.load(Ordering::Acquire) {
            return Err("microphone buffer overflow; recording stopped".into());
        }
        if let Some(error) = self.error.lock().map_err(|_| "microphone error state unavailable")?.take() {
            return Err(error);
        }
        match self.rx.recv_timeout(Duration::from_millis(20)) {
            Ok(chunk) => Ok(Some(chunk)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("microphone disconnected".into()),
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    tx: SyncSender<AudioChunk>,
    overflow: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let width = CHUNK / channels * channels;
                for samples in data.chunks(width) {
                    let mut chunk = AudioChunk { samples: [0.0; CHUNK], len: samples.len() };
                    for (out, &sample) in chunk.samples.iter_mut().zip(samples) {
                        *out = sample.to_sample::<f32>();
                    }
                    if tx.try_send(chunk).is_err() {
                        overflow.store(true, Ordering::Release);
                        break;
                    }
                }
            },
            move |e| {
                if let Ok(mut slot) = error.lock() {
                    *slot = Some(format!("microphone failed: {e}"));
                }
            },
            None,
        )
        .map_err(|e| format!("cannot create microphone stream: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioBuffer, SAMPLE_RATE, peak};
    use std::time::Instant;
    /// Real CPAL host/device. Speak for two seconds. A virtual native device may serve recorded
    /// input; label that distinction in acceptance evidence rather than claiming hardware coverage.
    #[test]
    #[ignore = "requires a native audio input device; speak into the selected microphone"]
    fn actual_microphone_delivers_samples_and_releases_the_device() {
        let device = std::env::var("VOICE_CAPTURE_DEVICE").ok();
        let capture = Capture::start(device.as_deref()).expect("open native input device");
        let mut audio = AudioBuffer::new(capture.rate, capture.channels).expect("capture format");
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut callbacks = 0;
        let mut loudest = 0.0_f32;
        while audio.audio().samples().len() < SAMPLE_RATE * 2 && Instant::now() < deadline {
            if let Some(chunk) = capture.read().expect("read native callbacks") {
                callbacks += 1;
                loudest = loudest.max(peak(&chunk.samples[..chunk.len]));
                audio.push(&chunk.samples[..chunk.len]).expect("convert native samples");
            }
        }
        eprintln!(
            "native capture: callbacks={callbacks}, source_rate={}, source_channels={}, pcm16_samples={}, peak={loudest}",
            capture.rate,
            capture.channels,
            audio.audio().samples().len()
        );
        assert!(callbacks > 0, "device must deliver callbacks");
        assert!(audio.audio().samples().len() >= SAMPLE_RATE * 2, "device must deliver two seconds of sample data");
        assert!(loudest > 0.001, "speak or supply non-silent recorded input");
        drop(capture);
        let reopened = Capture::start(device.as_deref()).expect("device reopens after cleanup");
        drop(reopened);
    }
}
