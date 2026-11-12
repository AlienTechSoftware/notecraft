use craft_voice_runtime::audio::{AudioBuffer, MAX_SAMPLES, PcmAudio, peak};

#[test]
fn the_level_meter_reads_the_loudest_sample() {
    assert_eq!(peak(&[]), 0.0);
    assert_eq!(peak(&[0.0; 64]), 0.0);
    assert_eq!(peak(&[0.1, -0.5, 0.25]), 0.5);
    assert_eq!(peak(&[2.0, f32::NAN]), 1.0);
}

#[test]
fn stereo_48khz_is_mixed_and_resampled_across_chunks() {
    let mut buffer = AudioBuffer::new(48000, 2).unwrap();
    buffer.push(&[0.0, 0.0, 1.0, 1.0]).unwrap();
    buffer.push(&[1.0, 1.0, 1.0, 1.0, 0.0, 0.0, -1.0, -1.0]).unwrap();
    assert_eq!(buffer.audio().samples(), &[0, 32767]);
    let before = buffer.audio().samples().to_vec();
    assert!(buffer.push(&[f32::NAN, 0.0]).is_err());
    assert_eq!(buffer.audio().samples(), before);
}

#[test]
fn wav_round_trip_keeps_signed_pcm_and_rejects_truncation() {
    let audio = PcmAudio::new(vec![-32768, -1, 0, 1, 32767]).unwrap();
    let wav = audio.wav();
    assert_eq!(PcmAudio::read_wav(&wav).unwrap().samples(), &[-32768, -1, 0, 1, 32767]);
    assert!(PcmAudio::read_wav(&wav[..wav.len() - 1]).is_err());
    let mut wrong_rate = wav;
    wrong_rate[24..28].copy_from_slice(&48000u32.to_le_bytes());
    assert!(PcmAudio::read_wav(&wrong_rate).is_err());
}

#[test]
fn duration_limit_stops_capture_and_never_grows_beyond_sixty_seconds() {
    let mut buffer = AudioBuffer::new(16000, 1).unwrap();
    assert!(buffer.push(&vec![0.5; MAX_SAMPLES + 20]).unwrap());
    assert_eq!(buffer.audio().samples().len(), MAX_SAMPLES);
    assert!(buffer.push(&[1.0]).unwrap());
    assert_eq!(buffer.audio().samples().len(), MAX_SAMPLES);
    assert!(PcmAudio::new(vec![0; MAX_SAMPLES + 1]).is_err());
}

#[test]
fn wav_rejects_unparsed_trailing_bytes_and_missing_padding() {
    let mut trailing = PcmAudio::new(vec![0]).unwrap().wav();
    trailing.extend_from_slice(b"x");
    let length = (trailing.len() - 8) as u32;
    trailing[4..8].copy_from_slice(&length.to_le_bytes());
    assert!(PcmAudio::read_wav(&trailing).is_err());
    let mut odd = PcmAudio::default().wav();
    odd.extend_from_slice(b"JUNK");
    odd.extend_from_slice(&1_u32.to_le_bytes());
    odd.push(0);
    let length = (odd.len() - 8) as u32;
    odd[4..8].copy_from_slice(&length.to_le_bytes());
    assert!(PcmAudio::read_wav(&odd).is_err());
}
