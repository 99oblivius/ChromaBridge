// Windows.Graphics.Capture API - Modern capture with proper window exclusion
use anyhow::Result;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
use windows::{
    core::*,
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::{Direct3D11::IDirect3DSurface, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Foundation::{HWND, LPARAM, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::*,
            Dxgi::Common::*,
        },
        System::WinRT::{
            CreateDirect3D11DeviceFromDXGIDevice,
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
        UI::WindowsAndMessaging::{EnumDisplayMonitors, GetMonitorInfoW, HMONITOR, MONITORINFOEXW},
    },
};

pub struct WindowsGraphicsCapture {
    #[cfg(windows)]
    d3d_device: ID3D11Device,
    #[cfg(windows)]
    d3d_context: ID3D11DeviceContext,
    #[cfg(windows)]
    session: Option<GraphicsCaptureSession>,
    #[cfg(windows)]
    frame_pool: Option<Direct3D11CaptureFramePool>,
    #[cfg(windows)]
    last_frame: Arc<Mutex<Option<Vec<u8>>>>,

    width: u32,
    height: u32,
}

impl WindowsGraphicsCapture {
    #[cfg(windows)]
    pub fn new(monitor_index: usize) -> Result<Self> {
        unsafe {
            // Initialize COM for this thread
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            );

            // Create D3D11 device
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;

            let feature_levels = [
                D3D_FEATURE_LEVEL_11_1,
                D3D_FEATURE_LEVEL_11_0,
                D3D_FEATURE_LEVEL_10_1,
                D3D_FEATURE_LEVEL_10_0,
            ];

            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                windows::Win32::Foundation::HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(|e| anyhow::anyhow!("Failed to create D3D11 device: {:?}", e))?;

            let device = device.unwrap();
            let context = context.unwrap();

            // Get monitor HMONITOR
            let hmonitor = Self::get_monitor_handle(monitor_index)?;

            // Create GraphicsCaptureItem for the monitor
            let interop: IGraphicsCaptureItemInterop = windows::core::factory::<
                GraphicsCaptureItem,
                IGraphicsCaptureItemInterop,
            >()?;

            let item: GraphicsCaptureItem = interop.CreateForMonitor(hmonitor)?;

            // Get size
            let size = item.Size()?;
            let width = size.Width as u32;
            let height = size.Height as u32;

            // Create Direct3D11 device wrapper for WinRT
            let dxgi_device = device.cast::<windows::Win32::Graphics::Dxgi::IDXGIDevice>()?;
            let d3d_device = CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device)?;

            // Create frame pool
            let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                &d3d_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                1, // number of buffers
                size,
            )?;

            // Create capture session
            let session = frame_pool.CreateCaptureSession(&item)?;

            // Storage for frames
            let last_frame = Arc::new(Mutex::new(None));
            let last_frame_clone = last_frame.clone();

            // Set up frame arrived handler
            frame_pool.FrameArrived(&TypedEventHandler::new(
                move |pool: &Option<Direct3D11CaptureFramePool>, _| {
                    if let Some(pool) = pool {
                        if let Ok(frame) = pool.TryGetNextFrame() {
                            // Extract surface
                            if let Ok(surface) = frame.Surface() {
                                // TODO: Copy surface data to last_frame
                                // This is complex - need to get texture from surface
                            }
                        }
                    }
                    Ok(())
                },
            ))?;

            // Start capture
            session.StartCapture()?;

            Ok(Self {
                d3d_device: device,
                d3d_context: context,
                session: Some(session),
                frame_pool: Some(frame_pool),
                last_frame,
                width,
                height,
            })
        }
    }

    #[cfg(windows)]
    fn get_monitor_handle(monitor_index: usize) -> Result<HMONITOR> {
        use std::sync::Mutex;

        let monitors = Mutex::new(Vec::new());

        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(Self::monitor_enum_proc),
                LPARAM(&monitors as *const _ as isize),
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
        _hdc: windows::Win32::Graphics::Gdi::HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> windows::core::BOOL {
        use std::sync::Mutex;

        let monitors = &*(lparam.0 as *const Mutex<Vec<HMONITOR>>);
        let mut monitors = monitors.lock().unwrap();
        monitors.push(hmonitor);

        true.into()
    }

    pub fn capture_frame(&mut self) -> Result<Option<Vec<u8>>> {
        #[cfg(windows)]
        {
            // Return last captured frame
            let frame = self.last_frame.lock().unwrap();
            Ok(frame.clone())
        }

        #[cfg(not(windows))]
        Ok(None)
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[cfg(not(windows))]
    pub fn new(_monitor_index: usize) -> Result<Self> {
        anyhow::bail!("Windows Graphics Capture only supported on Windows")
    }
}

#[cfg(windows)]
impl Drop for WindowsGraphicsCapture {
    fn drop(&mut self) {
        // Stop capture session
        if let Some(session) = &self.session {
            let _ = session.Close();
        }
        if let Some(pool) = &self.frame_pool {
            let _ = pool.Close();
        }
    }
}
