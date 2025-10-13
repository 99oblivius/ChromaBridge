use anyhow::Result;

#[cfg(windows)]
use windows::{
    core::BOOL,
    Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
    },
};

/// Get the position and size of a specific monitor by index
#[cfg(windows)]
pub fn get_monitor_rect(monitor_index: usize) -> Result<((i32, i32), (i32, i32))> {
    use std::sync::Mutex;

    let monitors = Mutex::new(Vec::new());

    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum_proc),
            windows::Win32::Foundation::LPARAM(&monitors as *const _ as isize),
        );
    }

    let monitors_list = monitors.into_inner().unwrap();

    if monitor_index < monitors_list.len() {
        Ok(monitors_list[monitor_index])
    } else {
        anyhow::bail!("Monitor index {} out of bounds", monitor_index)
    }
}

#[cfg(windows)]
unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut windows::Win32::Foundation::RECT,
    lparam: windows::Win32::Foundation::LPARAM,
) -> BOOL {
    use std::sync::Mutex;

    let monitors = &*(lparam.0 as *const Mutex<Vec<((i32, i32), (i32, i32))>>);

    let mut info: MONITORINFOEXW = std::mem::zeroed();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _).as_bool() {
        let rect = info.monitorInfo.rcMonitor;
        let pos = (rect.left, rect.top);
        let size = (rect.right - rect.left, rect.bottom - rect.top);

        let mut monitors = monitors.lock().unwrap();
        monitors.push((pos, size));
    }

    true.into()
}

#[cfg(not(windows))]
pub fn get_monitor_rect(_monitor_index: usize) -> Result<((i32, i32), (i32, i32))> {
    // Fallback for non-Windows platforms
    Ok(((0, 0), (1920, 1080)))
}
