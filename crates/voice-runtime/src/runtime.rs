//! Worker ownership and a one-slot, coalescing preview queue.

#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
use crate::audio::SAMPLE_RATE;
use crate::{Cancellation, Transcriber, audio::PcmAudio};
use craft_voice_core::Transcript;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::thread::JoinHandle;
#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
use std::{
    sync::{Condvar, Mutex},
    time::Duration,
};

#[derive(Debug)]
pub enum RuntimeEvent {
    Ready {
        session: u64,
    },
    Stopped {
        session: u64,
    },
    /// Recorded length so far, and the loudest input since the last report (0 to 1).
    Progress {
        session: u64,
        seconds: f32,
        level: f32,
    },
    Transcript(Transcript),
    Error {
        session: u64,
        message: String,
    },
}

pub struct Runtime {
    cancel: Cancellation,
    stop: Arc<AtomicBool>,
    events: Option<Receiver<RuntimeEvent>>,
    worker: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn from_audio(session: u64, mut provider: Box<dyn Transcriber>, audio: PcmAudio) -> Result<Self, String> {
        Self::spawn(
            move |cancel, _, tx| {
                provider.prepare(&cancel)?;
                let _ = tx.send(RuntimeEvent::Ready { session });
                let _ = tx.send(RuntimeEvent::Stopped { session });
                let text = provider.transcribe(&audio, &cancel)?;
                if !cancel.is_cancelled() {
                    let _ = tx.send(RuntimeEvent::Transcript(Transcript::new(session, 1, text, true)));
                }
                Ok(())
            },
            session,
        )
    }

    pub fn start(session: u64, provider: Box<dyn Transcriber>, device: Option<String>) -> Result<Self, String> {
        #[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
        {
            Self::spawn(move |cancel, stop, tx| record(session, provider, device, cancel, stop, tx), session)
        }
        #[cfg(not(all(feature = "microphone", not(target_arch = "wasm32"))))]
        {
            let _ = (session, provider, device);
            Err("microphone support is not enabled in this build".into())
        }
    }

    fn spawn(
        work: impl FnOnce(Cancellation, Arc<AtomicBool>, SyncSender<RuntimeEvent>) -> Result<(), String> + Send + 'static,
        session: u64,
    ) -> Result<Self, String> {
        let cancel = Cancellation::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(16);
        let token = cancel.clone();
        let stop_flag = stop.clone();
        let worker = std::thread::Builder::new()
            .name("craft-voice".into())
            .spawn(move || {
                if let Err(message) = work(token.clone(), stop_flag, tx.clone())
                    && !token.is_cancelled()
                {
                    let _ = tx.send(RuntimeEvent::Error { session, message });
                }
            })
            .map_err(|e| format!("cannot start voice worker: {e}"))?;
        Ok(Self { cancel, stop, events: Some(rx), worker: Some(worker) })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
        self.stop();
    }
    pub fn poll(&mut self) -> Vec<RuntimeEvent> {
        self.events.as_ref().map(|rx| rx.try_iter().take(32).collect()).unwrap_or_default()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.cancel();
        self.events.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn microphone_devices() -> Result<Vec<String>, String> {
    #[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
    {
        crate::capture::devices()
    }
    #[cfg(not(all(feature = "microphone", not(target_arch = "wasm32"))))]
    {
        Err("microphone support is not enabled in this build".into())
    }
}

#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
const NO_AUDIO_LIMIT: Duration = Duration::from_secs(5);
#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
const NO_AUDIO: &str = "The microphone opened but sent no audio for 5 seconds. Check that the right microphone is selected, that it is not disabled, and that your system lets desktop apps use it.";

/// Tracks actual callback delivery, separately from opening a stream or decoding speech.
#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
struct CaptureClock {
    last_audio: std::time::Instant,
    started: Option<std::time::Instant>,
}
#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
impl CaptureClock {
    fn new(now: std::time::Instant) -> Self {
        Self { last_audio: now, started: None }
    }
    fn received(&mut self, now: std::time::Instant) -> bool {
        self.last_audio = now;
        if self.started.is_none() {
            self.started = Some(now);
            true
        } else {
            false
        }
    }
    fn check(&self, now: std::time::Instant) -> Result<bool, String> {
        if now.duration_since(self.last_audio) >= NO_AUDIO_LIMIT {
            return Err(NO_AUDIO.into());
        }
        Ok(self.started.is_some_and(|at| now.duration_since(at) >= Duration::from_secs(craft_voice_core::MAX_SECONDS as u64)))
    }
}

#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
struct Work {
    audio: PcmAudio,
    final_pass: bool,
}
#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
#[derive(Default)]
struct Queue {
    work: Option<Work>,
    done: bool,
    error: Option<String>,
}
#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
type SharedQueue = Arc<(Mutex<Queue>, Condvar)>;

#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
fn record(
    session: u64,
    mut provider: Box<dyn Transcriber>,
    device: Option<String>,
    cancel: Cancellation,
    stop: Arc<AtomicBool>,
    tx: SyncSender<RuntimeEvent>,
) -> Result<(), String> {
    // Open the microphone first: words spoken while the recognizer is being checked are kept.
    let queue: SharedQueue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
    let capture_failed = Arc::new(AtomicBool::new(false));
    let failure_signal = capture_failed.clone();
    let shared = queue.clone();
    let token = cancel.clone();
    let stopped = stop.clone();
    let events = tx.clone();
    let capture = std::thread::Builder::new()
        .name("craft-microphone".into())
        .spawn(move || {
            let result = capture_audio(session, device, &token, &stopped, &events, &shared);
            finish_capture(&shared, &failure_signal, result);
        })
        .map_err(|e| format!("cannot start microphone worker: {e}"))?;
    let decoder_cancel = cancel.interrupted_by(capture_failed);
    let result = provider.prepare(&decoder_cancel).and_then(|()| decode(session, provider.as_mut(), &decoder_cancel, &stop, &tx, &queue));
    // Stop capture on all decode exits, including a provider failure. Cancellation of a
    // preview must not suppress the failure event sent by the outer worker.
    stop.store(true, Ordering::Release);
    let _ = capture.join();
    decode_outcome(result, &queue)
}

#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
fn finish_capture(queue: &SharedQueue, failed: &AtomicBool, result: Result<(), String>) {
    if let Ok(mut q) = queue.0.lock() {
        q.done = true;
        q.error = result.err();
        if q.error.is_some() {
            failed.store(true, Ordering::Release);
        }
        queue.1.notify_all();
    } else {
        failed.store(true, Ordering::Release);
    }
}

#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
fn decode_outcome<T>(result: Result<T, String>, queue: &SharedQueue) -> Result<T, String> {
    let mut q = queue.0.lock().map_err(|_| "voice preview queue unavailable")?;
    match q.error.take() {
        Some(error) => Err(error),
        None => result,
    }
}

#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
fn capture_audio(
    session: u64,
    device: Option<String>,
    cancel: &Cancellation,
    stop: &AtomicBool,
    tx: &SyncSender<RuntimeEvent>,
    queue: &SharedQueue,
) -> Result<(), String> {
    if cancel.is_cancelled() {
        return Ok(());
    }
    let capture = crate::capture::Capture::start(device.as_deref())?;
    let mut buffer = crate::audio::AudioBuffer::new(capture.rate, capture.channels)?;
    let mut preview_at = SAMPLE_RATE * 5;
    let mut reported_at = 0;
    let mut clock = CaptureClock::new(std::time::Instant::now());
    let mut loudest = 0.0_f32;
    while !cancel.is_cancelled() && !stop.load(Ordering::Acquire) {
        if clock.check(std::time::Instant::now())? {
            stop.store(true, Ordering::Release);
            break;
        }
        let Some(chunk) = capture.read()? else { continue };
        let samples = chunk.samples.get(..chunk.len).unwrap_or_default();
        if samples.is_empty() {
            continue;
        }
        if clock.received(std::time::Instant::now()) {
            let _ = tx.send(RuntimeEvent::Ready { session });
        }
        loudest = loudest.max(crate::audio::peak(samples));
        let full = buffer.push(samples)?;
        let count = buffer.audio().samples().len();
        if count >= reported_at + SAMPLE_RATE / 5 {
            let _ = tx.try_send(RuntimeEvent::Progress { session, seconds: buffer.audio().seconds(), level: loudest });
            loudest = 0.0;
            reported_at = count;
        }
        if count >= preview_at {
            let mut q = queue.0.lock().map_err(|_| "voice preview queue unavailable")?;
            q.work = Some(Work { audio: buffer.audio().clone(), final_pass: false });
            queue.1.notify_one();
            preview_at = count + SAMPLE_RATE * 5;
        }
        if full {
            stop.store(true, Ordering::Release);
        }
    }
    drop(capture); // Release the device before the final decode, not after it.
    if !cancel.is_cancelled() {
        let _ = tx.send(RuntimeEvent::Stopped { session });
        let mut q = queue.0.lock().map_err(|_| "voice preview queue unavailable")?;
        q.work = Some(Work { audio: buffer.audio().clone(), final_pass: true });
        queue.1.notify_one();
    }
    Ok(())
}

#[cfg(all(feature = "microphone", not(target_arch = "wasm32")))]
fn decode(
    session: u64,
    provider: &mut dyn Transcriber,
    cancel: &Cancellation,
    stop: &Arc<AtomicBool>,
    tx: &SyncSender<RuntimeEvent>,
    queue: &SharedQueue,
) -> Result<(), String> {
    let mut revision = 0_u64;
    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let mut q = queue.0.lock().map_err(|_| "voice preview queue unavailable")?;
        if let Some(error) = q.error.take() {
            return Err(error);
        }
        let Some(work) = q.work.take() else {
            if q.done {
                return Ok(());
            }
            let _guard = queue.1.wait_timeout(q, Duration::from_millis(20)).map_err(|_| "voice preview queue unavailable")?;
            continue;
        };
        drop(q);
        let token = if work.final_pass { cancel.clone() } else { cancel.interrupted_by(stop.clone()) };
        let result = provider.transcribe(&work.audio, &token);
        if token.is_cancelled() {
            if cancel.is_cancelled() {
                return Ok(());
            } else {
                continue;
            }
        }
        // A preview is a courtesy: one that fails is skipped and recording goes on. Only the
        // final pass decides whether the recording produced words.
        let text = match result {
            Ok(text) => text,
            Err(_) if !work.final_pass => continue,
            Err(error) => return Err(error),
        };
        revision += 1;
        let _ = tx.send(RuntimeEvent::Transcript(Transcript::new(session, revision, text, work.final_pass)));
        if work.final_pass {
            return Ok(());
        }
    }
}

#[cfg(all(test, feature = "microphone", not(target_arch = "wasm32")))]
mod capture_failure_tests {
    use super::*;
    #[cfg(unix)]
    use std::time::Instant;

    #[cfg(unix)]
    #[test]
    fn capture_failure_interrupts_real_process_and_wins_over_decoder_error() {
        let queue: SharedQueue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let failed = Arc::new(AtomicBool::new(false));
        let token = Cancellation::default().interrupted_by(failed.clone());
        let mut child = std::process::Command::new("/bin/sleep").arg("10").spawn().unwrap();
        let q = queue.clone();
        let flag = failed.clone();
        let reporter = std::thread::spawn(move || finish_capture(&q, &flag, Err("microphone disconnected".into())));
        let started = Instant::now();
        let result = crate::whisper::wait_child(&mut child, &token, Duration::from_secs(3));
        reporter.join().unwrap();
        assert_eq!(decode_outcome(result, &queue), Err("microphone disconnected".to_string()));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn recording_requires_samples_and_stalled_audio_fails_after_five_seconds() {
        let start = std::time::Instant::now();
        let mut clock = CaptureClock::new(start);
        assert!(clock.started.is_none());
        assert_eq!(clock.check(start + Duration::from_secs(4)), Ok(false));
        assert!(clock.check(start + Duration::from_secs(5)).is_err());
        assert!(clock.received(start + Duration::from_secs(4)));
        assert!(!clock.received(start + Duration::from_secs(5)));
        assert_eq!(clock.started, Some(start + Duration::from_secs(4)));
        assert_eq!(clock.check(start + Duration::from_secs(9)), Ok(false));
        assert!(clock.check(start + Duration::from_secs(10)).is_err());
        clock.received(start + Duration::from_secs(64));
        assert_eq!(clock.check(start + Duration::from_secs(64)), Ok(true));
    }
}
