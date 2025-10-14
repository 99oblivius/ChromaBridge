use anyhow::Result;

#[cfg(windows)]
use windows::{
    core::*,
    Win32::Foundation::HMODULE,
    Win32::Graphics::{
        Direct3D::*,
        Direct3D11::*,
        Dxgi::Common::*,
        Dxgi::*,
    },
    Win32::System::Com::*,
};

pub struct DesktopDuplication {
    #[cfg(windows)]
    device: ID3D11Device,
    #[cfg(windows)]
    context: ID3D11DeviceContext,
    #[cfg(windows)]
    duplication: IDXGIOutputDuplication,
    #[cfg(windows)]
    staging_texture: Option<ID3D11Texture2D>,

    width: u32,
    height: u32,
    #[allow(dead_code)]
    monitor_index: usize,
}

impl DesktopDuplication {
    #[cfg(windows)]
    pub fn new(monitor_index: usize) -> Result<Self> {
        unsafe {
            // Initialize COM
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            // Ignore error - may already be initialized

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
                HMODULE::default(),
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

            // Get DXGI device
            let dxgi_device: IDXGIDevice = device.cast()
                .map_err(|e| anyhow::anyhow!("Failed to cast to IDXGIDevice: {:?}", e))?;

            // Get adapter
            let adapter = dxgi_device.GetAdapter()
                .map_err(|e| anyhow::anyhow!("Failed to get adapter: {:?}", e))?;

            // Enumerate outputs to find the requested monitor
            let output = adapter
                .EnumOutputs(monitor_index as u32)
                .map_err(|_| anyhow::anyhow!("Monitor index out of bounds"))?;

            // Get output1 for desktop duplication
            let output1: IDXGIOutput1 = output.cast()
                .map_err(|e| anyhow::anyhow!("Failed to cast to IDXGIOutput1: {:?}", e))?;

            // Get output description
            let desc = output.GetDesc()
                .map_err(|e| anyhow::anyhow!("Failed to get output description: {:?}", e))?;
            let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as u32;
            let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as u32;

            // Create desktop duplication
            let duplication = output1
                .DuplicateOutput(&device)
                .map_err(|e| anyhow::anyhow!("Failed to create desktop duplication - this usually means another app is already using it: {:?}", e))?;

            Ok(Self {
                device,
                context,
                duplication,
                staging_texture: None,
                width,
                height,
                monitor_index,
            })
        }
    }

    #[cfg(windows)]
    pub fn capture_frame(&mut self) -> Result<Option<Vec<u8>>> {
        unsafe {
            // Release previous frame if any
            let _ = self.duplication.ReleaseFrame();

            let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = std::mem::zeroed();
            let mut desktop_resource: Option<IDXGIResource> = None;

            // Try to acquire next frame
            // Use small timeout to avoid blocking the render loop
            match self.duplication.AcquireNextFrame(
                5, // 5ms timeout - fast polling for high refresh rates
                &mut frame_info,
                &mut desktop_resource,
            ) {
                Ok(_) => {
                    let desktop_resource = desktop_resource.unwrap();

                    // Get texture from resource
                    let texture: ID3D11Texture2D = desktop_resource.cast()
                        .map_err(|e| anyhow::anyhow!("Failed to cast to ID3D11Texture2D: {:?}", e))?;

                    // Get texture description
                    let mut desc: D3D11_TEXTURE2D_DESC = std::mem::zeroed();
                    texture.GetDesc(&mut desc);

                    // Create staging texture if not exists or size changed
                    if self.staging_texture.is_none() || desc.Width != self.width || desc.Height != self.height {
                        let staging_desc = D3D11_TEXTURE2D_DESC {
                            Width: desc.Width,
                            Height: desc.Height,
                            MipLevels: 1,
                            ArraySize: 1,
                            Format: desc.Format,
                            SampleDesc: DXGI_SAMPLE_DESC {
                                Count: 1,
                                Quality: 0,
                            },
                            Usage: D3D11_USAGE_STAGING,
                            BindFlags: 0,
                            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                            MiscFlags: 0,
                        };

                        let mut staging: Option<ID3D11Texture2D> = None;
                        self.device.CreateTexture2D(&staging_desc, None, Some(&mut staging))
                            .map_err(|e| anyhow::anyhow!("Failed to create staging texture: {:?}", e))?;
                        self.staging_texture = staging;
                        self.width = desc.Width;
                        self.height = desc.Height;
                    }

                    // Copy to staging texture
                    self.context.CopyResource(
                        self.staging_texture.as_ref().unwrap(),
                        &texture,
                    );

                    // Map the staging texture
                    let mut mapped: D3D11_MAPPED_SUBRESOURCE = std::mem::zeroed();
                    self.context.Map(
                        self.staging_texture.as_ref().unwrap(),
                        0,
                        D3D11_MAP_READ,
                        0,
                        Some(&mut mapped),
                    )
                    .map_err(|e| anyhow::anyhow!("Failed to map staging texture: {:?}", e))?;

                    // Copy pixel data
                    let pitch = mapped.RowPitch as usize;
                    let height = self.height as usize;
                    let mut pixels = vec![0u8; self.width as usize * height * 4]; // BGRA

                    for y in 0..height {
                        let src_offset = y * pitch;
                        let dst_offset = y * (self.width as usize * 4);
                        let row_size = self.width as usize * 4;

                        std::ptr::copy_nonoverlapping(
                            (mapped.pData as *const u8).add(src_offset),
                            pixels.as_mut_ptr().add(dst_offset),
                            row_size,
                        );
                    }

                    // Unmap
                    self.context.Unmap(self.staging_texture.as_ref().unwrap(), 0);

                    Ok(Some(pixels))
                }
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                    // No new frame yet, this is normal
                    Ok(None)
                }
                Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => {
                    // Access lost, need to recreate
                    anyhow::bail!("Desktop duplication access lost - monitor may have been disconnected or changed")
                }
                Err(e) => Err(e.into()),
            }
        }
    }

    #[allow(dead_code)]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[allow(dead_code)]
    pub fn monitor_index(&self) -> usize {
        self.monitor_index
    }

    #[cfg(not(windows))]
    pub fn new(_monitor_index: usize) -> Result<Self> {
        anyhow::bail!("Desktop duplication only supported on Windows")
    }

    #[cfg(not(windows))]
    pub fn capture_frame(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

#[cfg(windows)]
impl Drop for DesktopDuplication {
    fn drop(&mut self) {
        unsafe {
            let _ = self.duplication.ReleaseFrame();
            CoUninitialize();
        }
    }
}
