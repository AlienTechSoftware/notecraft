//! Bounded speech audio and its portable PCM16 WAV representation.

pub const SAMPLE_RATE: usize = 16000;
pub const MAX_SAMPLES: usize = SAMPLE_RATE * craft_voice_core::MAX_SECONDS;

#[derive(Clone, Debug, Default)]
pub struct PcmAudio {
    samples: Vec<i16>,
}

impl PcmAudio {
    pub fn new(samples: Vec<i16>) -> Result<Self, String> {
        if samples.len() > MAX_SAMPLES {
            return Err("recording exceeds 60 seconds".into());
        }
        Ok(Self { samples })
    }
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }
    pub fn seconds(&self) -> f32 {
        self.samples.len() as f32 / SAMPLE_RATE as f32
    }

    pub fn wav(&self) -> Vec<u8> {
        let bytes = (self.samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + bytes as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + bytes).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16_u32.to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&(SAMPLE_RATE as u32).to_le_bytes());
        out.extend_from_slice(&(SAMPLE_RATE as u32 * 2).to_le_bytes());
        out.extend_from_slice(&2_u16.to_le_bytes());
        out.extend_from_slice(&16_u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&bytes.to_le_bytes());
        for sample in &self.samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }

    /// Reads RIFF PCM16 mono 16 kHz, allowing extra RIFF chunks.
    pub fn read_wav(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_SAMPLES * 2 + 65536 || bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
            return Err("expected a bounded PCM16 mono 16 kHz WAV file".into());
        }
        let read_u32 = |at: usize| -> Option<u32> { Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?)) };
        let declared = read_u32(4).ok_or("truncated WAV header")? as usize;
        if declared.checked_add(8) != Some(bytes.len()) {
            return Err("truncated or inconsistent WAV length".into());
        }
        let mut at = 12_usize;
        let mut format_ok = false;
        let mut data = None;
        while at.checked_add(8).is_some_and(|end| end <= bytes.len()) {
            let len = read_u32(at + 4).ok_or("truncated WAV chunk")? as usize;
            let end = at.checked_add(8).and_then(|n| n.checked_add(len)).ok_or("WAV chunk is too large")?;
            let chunk = bytes.get(at + 8..end).ok_or("truncated WAV chunk")?;
            match bytes.get(at..at + 4) {
                Some(b"fmt ") => {
                    format_ok =
                        chunk.len() >= 16 && chunk[..4] == [1, 0, 1, 0] && chunk[4..8] == 16000_u32.to_le_bytes() && chunk[12..16] == [2, 0, 16, 0];
                }
                Some(b"data") => {
                    if data.is_some() {
                        return Err("multiple WAV data chunks are not supported".into());
                    }
                    data = Some(chunk);
                }
                _ => {}
            }
            at = end.checked_add(len % 2).ok_or("WAV chunk is too large")?;
        }
        if at != bytes.len() {
            return Err("truncated WAV chunk header or missing chunk padding".into());
        }
        if !format_ok {
            return Err("WAV must be PCM16, mono, 16 kHz".into());
        }
        let data = data.ok_or("WAV has no audio data")?;
        if !data.len().is_multiple_of(2) {
            return Err("WAV ends in a partial sample".into());
        }
        Self::new(data.as_chunks::<2>().0.iter().copied().map(i16::from_le_bytes).collect())
    }
}

/// The loudest sample in `samples`, from 0 (silence) to 1 (full scale), for an input level meter.
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().filter(|s| s.is_finite()).fold(0.0_f32, |m, s| m.max(s.abs())).min(1.0)
}

/// Stateful linear resampling across callback boundaries, with channel averaging.
pub struct AudioBuffer {
    rate: u32,
    channels: usize,
    frame: u64,
    next: f64,
    last: f32,
    audio: PcmAudio,
}

impl AudioBuffer {
    pub fn new(rate: u32, channels: usize) -> Result<Self, String> {
        if !(8000..=384000).contains(&rate) || channels == 0 || channels > 64 {
            return Err("unsupported microphone sample rate or channel count".into());
        }
        Ok(Self { rate, channels, frame: 0, next: 0.0, last: 0.0, audio: PcmAudio::default() })
    }
    pub fn audio(&self) -> &PcmAudio {
        &self.audio
    }
    /// True means the 60-second limit was reached. Invalid input is rejected before mutation.
    pub fn push(&mut self, input: &[f32]) -> Result<bool, String> {
        if !input.len().is_multiple_of(self.channels) || input.iter().any(|s| !s.is_finite()) {
            return Err("invalid microphone sample data".into());
        }
        for frame in input.chunks_exact(self.channels) {
            let mono = frame.iter().map(|s| s.clamp(-1.0, 1.0)).sum::<f32>() / self.channels as f32;
            while self.next <= self.frame as f64 {
                if self.audio.samples.len() >= MAX_SAMPLES {
                    return Ok(true);
                }
                let fraction = if self.frame == 0 { 1.0 } else { (self.next - (self.frame - 1) as f64) as f32 };
                let value = self.last + (mono - self.last) * fraction;
                self.audio.samples.push((value.clamp(-1.0, 1.0) * 32767.0).round() as i16);
                self.next += self.rate as f64 / SAMPLE_RATE as f64;
            }
            self.last = mono;
            self.frame += 1;
        }
        Ok(self.audio.samples.len() >= MAX_SAMPLES)
    }
}
