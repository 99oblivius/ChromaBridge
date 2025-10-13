use anyhow::Result;

#[cfg(windows)]
use windows::{
    core::BOOL,
    Win32::Graphics::Gdi::{
        EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
        DEVMODEW, ENUM_CURRENT_SETTINGS,
    },
};

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub is_primary: bool,
    pub width: i32,
    pub height: i32,
    pub refresh_rate: u32,
}

#[cfg(windows)]
pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>> {
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

    let mut result = monitors.into_inner().unwrap();

    // Sort by primary first, then by index
    result.sort_by(|a: &MonitorInfo, b: &MonitorInfo| {
        b.is_primary.cmp(&a.is_primary).then(a.index.cmp(&b.index))
    });

    Ok(result)
}

#[cfg(windows)]
unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut windows::Win32::Foundation::RECT,
    lparam: windows::Win32::Foundation::LPARAM,
) -> BOOL {
    use std::sync::Mutex;
    let monitors = &*(lparam.0 as *const Mutex<Vec<MonitorInfo>>);

    let mut info: MONITORINFOEXW = std::mem::zeroed();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _).as_bool() {
        let rect = info.monitorInfo.rcMonitor;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let is_primary = (info.monitorInfo.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY

        let name = String::from_utf16_lossy(
            &info.szDevice
                .iter()
                .take_while(|&&c| c != 0)
                .copied()
                .collect::<Vec<_>>(),
        );

        // Get refresh rate for this monitor
        let refresh_rate = {
            let mut dev_mode: DEVMODEW = std::mem::zeroed();
            dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;

            if EnumDisplaySettingsW(
                windows::core::PCWSTR(info.szDevice.as_ptr()),
                ENUM_CURRENT_SETTINGS,
                &mut dev_mode,
            ).as_bool() {
                dev_mode.dmDisplayFrequency
            } else {
                60 // Default fallback
            }
        };

        let mut monitors = monitors.lock().unwrap();
        let index = monitors.len();

        monitors.push(MonitorInfo {
            index,
            name,
            is_primary,
            width,
            height,
            refresh_rate,
        });
    }

    true.into()
}

#[cfg(not(windows))]
pub fn enumerate_monitors() -> Result<Vec<MonitorInfo>> {
    // Fallback for non-Windows platforms
    Ok(vec![MonitorInfo {
        index: 0,
        name: "Primary Monitor".to_string(),
        is_primary: true,
        width: 1920,
        height: 1080,
        refresh_rate: 60,
    }])
}
