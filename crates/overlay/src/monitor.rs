use anyhow::Result;

#[cfg(windows)]
use windows::{
    core::BOOL,
    Win32::Graphics::Gdi::{
        EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
        DEVMODEW, ENUM_CURRENT_SETTINGS,
    },
};

/// Monitor information including position, size, and refresh rate
#[derive(Debug, Clone)]
pub struct MonitorRect {
    pub pos: (i32, i32),
    pub size: (i32, i32),
    pub refresh_rate: u32,
}

/// Get the position and size of a specific monitor by index
#[cfg(windows)]
pub fn get_monitor_rect(monitor_index: usize) -> Result<((i32, i32), (i32, i32))> {
    let info = get_monitor_info(monitor_index)?;
    Ok((info.pos, info.size))
}

/// Get full monitor information including refresh rate
#[cfg(windows)]
pub fn get_monitor_info(monitor_index: usize) -> Result<MonitorRect> {
    use std::sync::Mutex;

    let monitors = Mutex::new(Vec::<MonitorRect>::new());

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
        Ok(monitors_list[monitor_index].clone())
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

    let monitors = &*(lparam.0 as *const Mutex<Vec<MonitorRect>>);

    let mut info: MONITORINFOEXW = std::mem::zeroed();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _).as_bool() {
        let rect = info.monitorInfo.rcMonitor;
        let pos = (rect.left, rect.top);
        let size = (rect.right - rect.left, rect.bottom - rect.top);

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
        monitors.push(MonitorRect {
            pos,
            size,
            refresh_rate,
        });
    }

    true.into()
}

#[cfg(not(windows))]
pub fn get_monitor_rect(_monitor_index: usize) -> Result<((i32, i32), (i32, i32))> {
    // Fallback for non-Windows platforms
    Ok(((0, 0), (1920, 1080)))
}

#[cfg(not(windows))]
pub fn get_monitor_info(_monitor_index: usize) -> Result<MonitorRect> {
    // Fallback for non-Windows platforms
    Ok(MonitorRect {
        pos: (0, 0),
        size: (1920, 1080),
        refresh_rate: 60,
    })
}
