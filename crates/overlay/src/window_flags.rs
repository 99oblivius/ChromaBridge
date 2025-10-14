// Windows API helpers to exclude window from screen capture
use anyhow::Result;

#[cfg(windows)]
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE},
};

/// Set window to be excluded from screen capture (Desktop Duplication, etc.)
/// This is what Xbox Game Bar uses to avoid appearing in recordings
#[cfg(windows)]
#[allow(dead_code)]
pub fn exclude_window_from_capture(hwnd: HWND) -> Result<()> {
    unsafe {
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
            .map_err(|e| anyhow::anyhow!("Failed to set window display affinity: {:?}", e))?;
    }
    Ok(())
}

/// Get HWND for the current process's main window
#[cfg(windows)]
#[allow(dead_code)]
pub fn get_current_window_hwnd() -> Option<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    use std::sync::Mutex;

    unsafe {
        let _current_pid = std::process::id();
        let result = Mutex::new(None);

        let _ = EnumWindows(
            Some(enum_windows_proc),
            windows::Win32::Foundation::LPARAM(&result as *const _ as isize),
        );

        result.into_inner().ok()?
    }
}

#[cfg(windows)]
#[allow(dead_code)]
unsafe extern "system" fn enum_windows_proc(
    hwnd: HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::core::BOOL {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    let result = &*(lparam.0 as *const std::sync::Mutex<Option<HWND>>);
    let current_pid = std::process::id();

    let mut window_pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut window_pid));

    if window_pid == current_pid {
        let mut result = result.lock().unwrap();
        *result = Some(hwnd);
        return false.into(); // Stop enumeration
    }

    true.into() // Continue enumeration
}

#[cfg(not(windows))]
pub fn exclude_window_from_capture(_hwnd: ()) -> Result<()> {
    Ok(())
}
