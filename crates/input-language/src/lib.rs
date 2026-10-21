//! Read the active Windows keyboard layout to show the input direction at the caret.

#[cfg(windows)]
use std::sync::Arc;

/// Whether the active Windows keyboard layout belongs to a right-to-left language.
/// Other platforms do not expose a keyboard layout through NoteCraft's window backend yet.
#[cfg(windows)]
pub fn is_rtl() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;

    // SAFETY: GetKeyboardLayout with thread id zero reads the calling UI thread's current
    // keyboard layout and has no pointer arguments or side effects.
    let hkl = unsafe { GetKeyboardLayout(0) } as usize;
    is_rtl_language_id(hkl as u16)
}

/// Watches a native window for Windows input-language changes and calls `on_change` when
/// the operating system notifies that its active input locale changed.
#[cfg(windows)]
pub fn watch_changes(hwnd: isize, on_change: impl Fn() + Send + Sync + 'static) -> Option<Watcher> {
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;

    let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
    let callback: Box<ChangeCallback> = Box::new(Arc::new(on_change));
    let callback = Box::into_raw(callback);
    // SAFETY: The subclass callback receives this stable allocation as reference data. The
    // Watcher removes the subclass before freeing it, or intentionally leaves it allocated
    // if Windows cannot confirm removal.
    let installed = unsafe { SetWindowSubclass(hwnd, Some(input_language_changed), SUBCLASS_ID, callback as usize) };
    if installed == 0 {
        // SAFETY: SetWindowSubclass failed, so Windows retained no reference to this allocation.
        unsafe { drop(Box::from_raw(callback)) };
        return None;
    }
    Some(Watcher { hwnd, callback })
}

#[cfg(not(windows))]
pub fn watch_changes(_hwnd: isize, _on_change: impl Fn() + Send + Sync + 'static) -> Option<Watcher> {
    None
}

#[cfg(windows)]
type ChangeCallback = Arc<dyn Fn() + Send + Sync>;

#[cfg(windows)]
const SUBCLASS_ID: usize = 0x4e43_494e_5055_5401;

/// Owns the native window subclass and its callback data.
#[cfg(windows)]
pub struct Watcher {
    hwnd: windows_sys::Win32::Foundation::HWND,
    callback: *mut ChangeCallback,
}

#[cfg(not(windows))]
pub struct Watcher;

#[cfg(windows)]
impl Drop for Watcher {
    fn drop(&mut self) {
        use windows_sys::Win32::UI::Shell::RemoveWindowSubclass;

        // SAFETY: This handle was subclassed by `watch_changes` on the current UI thread.
        // If removal succeeds, Windows will make no further callback with this reference data.
        let removed = unsafe { RemoveWindowSubclass(self.hwnd, Some(input_language_changed), SUBCLASS_ID) };
        if removed != 0 {
            // SAFETY: Successful removal guarantees the callback data is no longer in use.
            unsafe { drop(Box::from_raw(self.callback)) };
        }
        // If removal fails, leak the small callback allocation rather than risk a later
        // native callback dereferencing freed memory.
    }
}

#[cfg(windows)]
unsafe extern "system" fn input_language_changed(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
    _subclass_id: usize,
    reference: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_INPUTLANGCHANGE;

    if message == WM_INPUTLANGCHANGE {
        // SAFETY: `reference` points to the callback allocation owned by the Watcher. Windows
        // invokes the subclass only while it is installed, and Drop frees it only after the
        // subclass has been removed successfully.
        let callback = unsafe { &*(reference as *const ChangeCallback) };
        callback();
    }
    // SAFETY: This forwards the original message and parameters to the next window procedure,
    // preserving the normal Windows input-language handling.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn is_rtl_language_id(lang_id: u16) -> bool {
    // Windows LANGID primary language identifiers for scripts written right to left.
    matches!(lang_id & 0x03ff, 0x01 | 0x0d | 0x20 | 0x29 | 0x3d | 0x59 | 0x5a | 0x60 | 0x63 | 0x65 | 0x80 | 0x8c | 0x92)
}

/// Other platforms currently use the text's own direction to place the caret.
#[cfg(not(windows))]
pub fn is_rtl() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_language_ids_identify_rtl_input_layouts() {
        for lang_id in [0x0401, 0x040d, 0x0420, 0x0429, 0x043d, 0x0459, 0x0460, 0x0463, 0x0465, 0x0480, 0x048c, 0x0492] {
            assert!(super::is_rtl_language_id(lang_id), "{lang_id:#06x} is an RTL language");
        }
        for lang_id in [0x0409, 0x0407, 0x0411] {
            assert!(!super::is_rtl_language_id(lang_id), "{lang_id:#06x} is not RTL");
        }
    }
}
