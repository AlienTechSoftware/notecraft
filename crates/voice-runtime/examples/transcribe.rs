//! A standalone library consumer. Supply your own whisper-cli, model and PCM16 WAV.
use craft_voice_runtime::{
    Runtime, RuntimeEvent, WhisperConfig,
    audio::{MAX_SAMPLES, PcmAudio},
};
use std::{io::Read, path::PathBuf, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: transcribe WHISPER_CLI MODEL WAV PRIVATE_TEMP_ROOT".into());
    }
    let file = std::fs::File::open(&args[2])?;
    let mut bytes = Vec::new();
    file.take((MAX_SAMPLES * 2 + 65537) as u64).read_to_end(&mut bytes)?;
    let audio = PcmAudio::read_wav(&bytes)?;
    let config = WhisperConfig::new(PathBuf::from(&args[0]), PathBuf::from(&args[1]), PathBuf::from(&args[3]));
    let mut runtime = Runtime::from_audio(1, Box::new(config), audio)?;
    loop {
        for event in runtime.poll() {
            match event {
                RuntimeEvent::Transcript(t) if t.is_final => {
                    println!("{}", t.text);
                    return Ok(());
                }
                RuntimeEvent::Error { message, .. } => return Err(message.into()),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
