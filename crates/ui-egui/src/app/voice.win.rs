//! Windows microphone privacy settings, opened only by the user's explicit action.

pub(super) fn open_microphone_settings() -> std::io::Result<()> {
    std::process::Command::new("explorer.exe").arg("ms-settings:privacy-microphone").spawn().map(|_| ())
}
