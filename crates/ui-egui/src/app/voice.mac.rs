//! macOS microphone privacy settings, opened only by the user's explicit action.

pub(super) fn open_microphone_settings() -> std::io::Result<()> {
    std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone").spawn().map(|_| ())
}
