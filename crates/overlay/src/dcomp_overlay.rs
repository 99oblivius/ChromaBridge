// DirectComposition overlay - renders like Xbox Game Bar
// Excluded from Desktop Duplication by integrating at DWM level

use anyhow::Result;
use std::sync::Arc;
use parking_lot::RwLock;
use crate::{OverlayState, capture::DesktopDuplication};
use color_interlacer_core::{log_info, log_warn, log_error};

#[cfg(windows)]
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::{
            Direct3D::*,
            Direct3D11::*,
            Dxgi::Common::*,
            Dxgi::*,
            DirectComposition::*,
        },
        UI::WindowsAndMessaging::*,
    },
};

pub struct DCompOverlay {
    #[cfg(windows)]
    hwnd: HWND,
    #[cfg(windows)]
    d3d_device: ID3D11Device,
    #[cfg(windows)]
    d3d_context: ID3D11DeviceContext,
    #[cfg(windows)]
    swap_chain: IDXGISwapChain1,
    #[cfg(windows)]
    dcomp_device: IDCompositionDevice,
    #[cfg(windows)]
    #[allow(dead_code)]
    dcomp_target: IDCompositionTarget,
    #[cfg(windows)]
    #[allow(dead_code)]
    dcomp_visual: IDCompositionVisual,
    #[cfg(windows)]
    capture_texture: Option<ID3D11Texture2D>,
    #[cfg(windows)]
    capture_srv: Option<ID3D11ShaderResourceView>,

    // Rendering pipeline
    #[cfg(windows)]
    vertex_shader: Option<ID3D11VertexShader>,
    #[cfg(windows)]
    pixel_shader: Option<ID3D11PixelShader>,
    #[cfg(windows)]
    input_layout: Option<ID3D11InputLayout>,
    #[cfg(windows)]
    vertex_buffer: Option<ID3D11Buffer>,
    #[cfg(windows)]
    sampler_state: Option<ID3D11SamplerState>,
    #[cfg(windows)]
    spectrum_sampler: Option<ID3D11SamplerState>,
    #[cfg(windows)]
    blend_state: Option<ID3D11BlendState>,

    // Spectrum textures and constant buffer
    #[cfg(windows)]
    spectrum1_texture: Option<ID3D11Texture2D>,
    #[cfg(windows)]
    spectrum1_srv: Option<ID3D11ShaderResourceView>,
    #[cfg(windows)]
    spectrum2_texture: Option<ID3D11Texture2D>,
    #[cfg(windows)]
    spectrum2_srv: Option<ID3D11ShaderResourceView>,
    #[cfg(windows)]
    noise_srv: Option<ID3D11ShaderResourceView>,
    #[cfg(windows)]
    constant_buffer: Option<ID3D11Buffer>,

    // Direct2D for text rendering
    #[cfg(windows)]
    #[allow(dead_code)]
    d2d_factory: Option<windows::Win32::Graphics::Direct2D::ID2D1Factory>,
    #[cfg(windows)]
    #[allow(dead_code)]
    d2d_render_target: Option<windows::Win32::Graphics::Direct2D::ID2D1RenderTarget>,
    #[cfg(windows)]
    #[allow(dead_code)]
    d2d_text_format: Option<windows::Win32::Graphics::DirectWrite::IDWriteTextFormat>,
    #[cfg(windows)]
    #[allow(dead_code)]
    d2d_brush: Option<windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush>,

    width: u32,
    height: u32,
    monitor_refresh_rate: u32,
    state: Arc<RwLock<OverlayState>>,
    duplication: DesktopDuplication,
    frame_count: u32,
    fps_timer: std::time::Instant,
    frame_start_timer: std::time::Instant,
}

impl DCompOverlay {
    #[cfg(windows)]
    pub fn new(state: Arc<RwLock<OverlayState>>) -> Result<Self> {
        unsafe {
            // Initialize COM
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            );

            // Get monitor dimensions and refresh rate
            let monitor_index = state.read().monitor_index;
            let monitor_info = crate::monitor::get_monitor_info(monitor_index)?;
            let (pos, size) = (monitor_info.pos, monitor_info.size);

            // Create Desktop Duplication capture
            let duplication = DesktopDuplication::new(monitor_index)?;

            // Create invisible message-only window (NO standard window)
            let hwnd = Self::create_overlay_window(pos, size)?;

            // Create D3D11 device
            let (d3d_device, d3d_context) = Self::create_d3d_device()?;

            // Create swap chain for DirectComposition
            let swap_chain = Self::create_swap_chain(&d3d_device, size.0, size.1)?;

            // Create DirectComposition device and visual tree
            let dcomp_device: IDCompositionDevice =
                DCompositionCreateDevice(None)?;

            // Create composition target from HWND
            let dcomp_target = dcomp_device.CreateTargetForHwnd(hwnd, true)?;

            // Create visual and bind swap chain
            let dcomp_visual = dcomp_device.CreateVisual()?;
            dcomp_visual.SetContent(&swap_chain)?;

            // Set visual to target
            dcomp_target.SetRoot(&dcomp_visual)?;

            // Commit composition
            dcomp_device.Commit()?;

            log_info!("DirectComposition overlay initialized");
            log_info!("  Size: {}x{}", size.0, size.1);
            log_info!("  Position: ({}, {})", pos.0, pos.1);
            log_info!("  Refresh rate: {} Hz", monitor_info.refresh_rate);

            let mut overlay = Self {
                hwnd,
                d3d_device,
                d3d_context,
                swap_chain,
                dcomp_device,
                dcomp_target,
                dcomp_visual,
                capture_texture: None,
                capture_srv: None,
                vertex_shader: None,
                pixel_shader: None,
                input_layout: None,
                vertex_buffer: None,
                sampler_state: None,
                spectrum_sampler: None,
                blend_state: None,
                spectrum1_texture: None,
                spectrum1_srv: None,
                spectrum2_texture: None,
                spectrum2_srv: None,
                noise_srv: None,
                constant_buffer: None,
                d2d_factory: None,
                d2d_render_target: None,
                d2d_text_format: None,
                d2d_brush: None,
                width: size.0 as u32,
                height: size.1 as u32,
                monitor_refresh_rate: monitor_info.refresh_rate,
                state,
                duplication,
                frame_count: 0,
                fps_timer: std::time::Instant::now(),
                frame_start_timer: std::time::Instant::now(),
            };

            // Initialize rendering pipeline
            overlay.init_rendering_pipeline()?;

            // Initialize spectrum textures
            overlay.init_spectrum_textures()?;

            Ok(overlay)
        }
    }

    #[cfg(windows)]
    unsafe fn create_overlay_window(pos: (i32, i32), size: (i32, i32)) -> Result<HWND> {
        // Register window class
        let class_name = w!("ColorInterlacerOverlay");

        let hinstance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;

        let wc = WNDCLASSW {
            lpfnWndProc: Some(Self::window_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };

        RegisterClassW(&wc);

        // Create layered window for DirectComposition
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class_name,
            w!("Color Interlacer Overlay"),
            WS_POPUP,
            pos.0,
            pos.1,
            size.0,
            size.1,
            None,
            None,
            Some(HINSTANCE(hinstance.0)),
            None,
        )?;

        // Make window click-through
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TRANSPARENT.0 as i32);

        // CRITICAL: Exclude from screen capture (like Xbox Game Bar)
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE};
        if let Err(e) = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) {
            log_warn!("Failed to exclude window from capture: {:?}", e);
        } else {
            log_info!("Window excluded from Desktop Duplication (WDA_EXCLUDEFROMCAPTURE)");
        }

        // Show window
        let _ = ShowWindow(hwnd, SW_SHOW);

        Ok(hwnd)
    }

    #[cfg(windows)]
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    #[cfg(windows)]
    unsafe fn create_d3d_device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;

        let feature_levels = [
            D3D_FEATURE_LEVEL_11_1,
            D3D_FEATURE_LEVEL_11_0,
            D3D_FEATURE_LEVEL_10_1,
        ];

        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;

        Ok((device.unwrap(), context.unwrap()))
    }

    #[cfg(windows)]
    unsafe fn create_swap_chain(
        device: &ID3D11Device,
        width: i32,
        height: i32,
    ) -> Result<IDXGISwapChain1> {
        let dxgi_device = device.cast::<IDXGIDevice>()?;
        let dxgi_adapter = dxgi_device.GetAdapter()?;
        let dxgi_factory: IDXGIFactory2 = dxgi_adapter.GetParent()?;

        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width as u32,
            Height: height as u32,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            Flags: 0,
            ..Default::default()
        };

        let swap_chain = dxgi_factory.CreateSwapChainForComposition(
            device,
            &swap_chain_desc,
            None,
        )?;

        Ok(swap_chain)
    }

    #[cfg(windows)]
    unsafe fn init_rendering_pipeline(&mut self) -> Result<()> {
        // Embed the HLSL shader source
        const SHADER_SOURCE: &str = include_str!("shaders.hlsl");

        // Compile vertex shader
        let vs_blob = Self::compile_shader(SHADER_SOURCE, "VS_Main", "vs_5_0")?;
        let mut vertex_shader: Option<ID3D11VertexShader> = None;
        self.d3d_device.CreateVertexShader(
            std::slice::from_raw_parts(
                vs_blob.GetBufferPointer() as *const u8,
                vs_blob.GetBufferSize(),
            ),
            None,
            Some(&mut vertex_shader),
        )?;
        self.vertex_shader = vertex_shader;

        // Compile pixel shader
        let ps_blob = Self::compile_shader(SHADER_SOURCE, "PS_Main", "ps_5_0")?;
        let mut pixel_shader: Option<ID3D11PixelShader> = None;
        self.d3d_device.CreatePixelShader(
            std::slice::from_raw_parts(
                ps_blob.GetBufferPointer() as *const u8,
                ps_blob.GetBufferSize(),
            ),
            None,
            Some(&mut pixel_shader),
        )?;
        self.pixel_shader = pixel_shader;

        // Create input layout
        use windows::Win32::Graphics::Direct3D11::*;
        use windows::core::s;

        let input_elements = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: s!("TEXCOORD"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 8,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let mut input_layout: Option<ID3D11InputLayout> = None;
        self.d3d_device.CreateInputLayout(
            &input_elements,
            std::slice::from_raw_parts(
                vs_blob.GetBufferPointer() as *const u8,
                vs_blob.GetBufferSize(),
            ),
            Some(&mut input_layout),
        )?;
        self.input_layout = input_layout;

        // Create fullscreen quad vertex buffer
        #[repr(C)]
        struct Vertex {
            pos: [f32; 2],
            tex: [f32; 2],
        }

        let vertices = [
            // First triangle (top-left, top-right, bottom-left)
            Vertex { pos: [-1.0, 1.0], tex: [0.0, 0.0] },   // Top-left
            Vertex { pos: [1.0, 1.0], tex: [1.0, 0.0] },    // Top-right
            Vertex { pos: [-1.0, -1.0], tex: [0.0, 1.0] },  // Bottom-left

            // Second triangle (top-right, bottom-right, bottom-left)
            Vertex { pos: [1.0, 1.0], tex: [1.0, 0.0] },    // Top-right
            Vertex { pos: [1.0, -1.0], tex: [1.0, 1.0] },   // Bottom-right
            Vertex { pos: [-1.0, -1.0], tex: [0.0, 1.0] },  // Bottom-left
        ];

        let vertex_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: vertices.as_ptr() as *const _,
            SysMemPitch: 0,
            SysMemSlicePitch: 0,
        };

        let buffer_desc = D3D11_BUFFER_DESC {
            ByteWidth: std::mem::size_of_val(&vertices) as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
            StructureByteStride: 0,
        };

        let mut vertex_buffer: Option<ID3D11Buffer> = None;
        self.d3d_device.CreateBuffer(
            &buffer_desc,
            Some(&vertex_data),
            Some(&mut vertex_buffer),
        )?;
        self.vertex_buffer = vertex_buffer;

        // Create sampler state
        let sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MipLODBias: 0.0,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            BorderColor: [0.0, 0.0, 0.0, 0.0],
            MinLOD: 0.0,
            MaxLOD: f32::MAX,
        };

        let mut sampler_state: Option<ID3D11SamplerState> = None;
        self.d3d_device.CreateSamplerState(&sampler_desc, Some(&mut sampler_state))?;
        self.sampler_state = sampler_state;

        // Create spectrum sampler (for 1D spectrum texture lookup)
        let spectrum_sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MipLODBias: 0.0,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            BorderColor: [0.0, 0.0, 0.0, 0.0],
            MinLOD: 0.0,
            MaxLOD: f32::MAX,
        };

        let mut spectrum_sampler: Option<ID3D11SamplerState> = None;
        self.d3d_device.CreateSamplerState(&spectrum_sampler_desc, Some(&mut spectrum_sampler))?;
        self.spectrum_sampler = spectrum_sampler;

        // Create blend state for transparent overlay
        let blend_desc = D3D11_BLEND_DESC {
            AlphaToCoverageEnable: false.into(),
            IndependentBlendEnable: false.into(),
            RenderTarget: [
                D3D11_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: true.into(),
                    SrcBlend: D3D11_BLEND_ONE,
                    DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                    BlendOp: D3D11_BLEND_OP_ADD,
                    SrcBlendAlpha: D3D11_BLEND_ONE,
                    DestBlendAlpha: D3D11_BLEND_ZERO,
                    BlendOpAlpha: D3D11_BLEND_OP_ADD,
                    RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
        };

        let mut blend_state: Option<ID3D11BlendState> = None;
        self.d3d_device.CreateBlendState(&blend_desc, Some(&mut blend_state))?;
        self.blend_state = blend_state;

        log_info!("Rendering pipeline initialized");

        Ok(())
    }

    #[cfg(windows)]
    unsafe fn compile_shader(
        source: &str,
        entry_point: &str,
        target: &str,
    ) -> Result<windows::Win32::Graphics::Direct3D::ID3DBlob> {
        use windows::Win32::Graphics::Direct3D::Fxc::*;
        use windows::Win32::Graphics::Direct3D::ID3DBlob;
        use windows::core::PCSTR;

        let mut blob: Option<ID3DBlob> = None;
        let mut error_blob: Option<ID3DBlob> = None;

        let entry_cstr = std::ffi::CString::new(entry_point)?;
        let target_cstr = std::ffi::CString::new(target)?;

        let result = D3DCompile(
            source.as_ptr() as *const _,
            source.len(),
            None,
            None,
            None,
            PCSTR(entry_cstr.as_ptr() as *const u8),
            PCSTR(target_cstr.as_ptr() as *const u8),
            D3DCOMPILE_ENABLE_STRICTNESS,
            0,
            &mut blob,
            Some(&mut error_blob),
        );

        if result.is_err() {
            if let Some(error_blob) = error_blob {
                let error_msg = std::slice::from_raw_parts(
                    error_blob.GetBufferPointer() as *const u8,
                    error_blob.GetBufferSize(),
                );
                let error_str = String::from_utf8_lossy(error_msg);
                return Err(anyhow::anyhow!("Shader compilation failed: {}", error_str));
            }
            return Err(anyhow::anyhow!("Shader compilation failed"));
        }

        Ok(blob.unwrap())
    }

    #[cfg(windows)]
    unsafe fn init_spectrum_textures(&mut self) -> Result<()> {
        const SPECTRUM_RESOLUTION: usize = 360; // One RGB value per degree

        let state = self.state.read();

        // Generate spectrum1 lookup table
        let spectrum1_data = state.spectrum_pair.spectrum1.get_rgb_lookup_table(SPECTRUM_RESOLUTION)?;

        // Create spectrum1 texture (1D texture stored as 2D with height=1)
        let spectrum_desc = D3D11_TEXTURE2D_DESC {
            Width: SPECTRUM_RESOLUTION as u32,
            Height: 1,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R32G32B32_FLOAT, // RGB float format
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };

        let spectrum1_init_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: spectrum1_data.as_ptr() as *const _,
            SysMemPitch: (SPECTRUM_RESOLUTION * 3 * std::mem::size_of::<f32>()) as u32,
            SysMemSlicePitch: 0,
        };

        let mut spectrum1_texture: Option<ID3D11Texture2D> = None;
        self.d3d_device.CreateTexture2D(&spectrum_desc, Some(&spectrum1_init_data), Some(&mut spectrum1_texture))?;
        self.spectrum1_texture = spectrum1_texture.clone();

        // Create SRV for spectrum1
        let mut spectrum1_srv: Option<ID3D11ShaderResourceView> = None;
        self.d3d_device.CreateShaderResourceView(
            &spectrum1_texture.unwrap(),
            None,
            Some(&mut spectrum1_srv),
        )?;
        self.spectrum1_srv = spectrum1_srv;

        // If dual spectrum, create spectrum2
        if let Some(ref spectrum2) = state.spectrum_pair.spectrum2 {
            let spectrum2_data = spectrum2.get_rgb_lookup_table(SPECTRUM_RESOLUTION)?;

            let spectrum2_init_data = D3D11_SUBRESOURCE_DATA {
                pSysMem: spectrum2_data.as_ptr() as *const _,
                SysMemPitch: (SPECTRUM_RESOLUTION * 3 * std::mem::size_of::<f32>()) as u32,
                SysMemSlicePitch: 0,
            };

            let mut spectrum2_texture: Option<ID3D11Texture2D> = None;
            self.d3d_device.CreateTexture2D(&spectrum_desc, Some(&spectrum2_init_data), Some(&mut spectrum2_texture))?;
            self.spectrum2_texture = spectrum2_texture.clone();

            // Create SRV for spectrum2
            let mut spectrum2_srv: Option<ID3D11ShaderResourceView> = None;
            self.d3d_device.CreateShaderResourceView(
                &spectrum2_texture.unwrap(),
                None,
                Some(&mut spectrum2_srv),
            )?;
            self.spectrum2_srv = spectrum2_srv;
        }

        // Load noise texture if available
        if let Some(ref noise_texture) = state.noise_texture {
            // Convert noise texture to GPU format
            let noise_width = noise_texture.width();
            let noise_height = noise_texture.height();

            // Create noise texture data (R8 format - grayscale)
            let mut noise_data: Vec<u8> = Vec::with_capacity((noise_width * noise_height) as usize);
            for y in 0..noise_height {
                for x in 0..noise_width {
                    let value = if noise_texture.sample(x, y, noise_width, noise_height) {
                        255u8 // White
                    } else {
                        0u8   // Black
                    };
                    noise_data.push(value);
                }
            }

            let noise_desc = D3D11_TEXTURE2D_DESC {
                Width: noise_width,
                Height: noise_height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };

            let noise_init_data = D3D11_SUBRESOURCE_DATA {
                pSysMem: noise_data.as_ptr() as *const _,
                SysMemPitch: noise_width,
                SysMemSlicePitch: 0,
            };

            let mut noise_texture_d3d: Option<ID3D11Texture2D> = None;
            self.d3d_device.CreateTexture2D(&noise_desc, Some(&noise_init_data), Some(&mut noise_texture_d3d))?;

            // Create SRV for noise
            let mut noise_srv: Option<ID3D11ShaderResourceView> = None;
            self.d3d_device.CreateShaderResourceView(
                &noise_texture_d3d.unwrap(),
                None,
                Some(&mut noise_srv),
            )?;
            self.noise_srv = noise_srv;
        }

        // Create constant buffer for shader parameters
        #[repr(C)]
        struct SpectrumParams {
            strength: f32,
            use_dual_spectrum: i32,
            use_noise_texture: i32,
            padding: f32,
        }

        let params = SpectrumParams {
            strength: state.hue_mapper.strength,
            use_dual_spectrum: if state.spectrum_pair.has_dual_spectrum() { 1 } else { 0 },
            use_noise_texture: if state.noise_texture.is_some() { 1 } else { 0 },
            padding: 0.0,
        };

        let cb_desc = D3D11_BUFFER_DESC {
            ByteWidth: std::mem::size_of::<SpectrumParams>() as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            MiscFlags: 0,
            StructureByteStride: 0,
        };

        let cb_init_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: &params as *const _ as *const _,
            SysMemPitch: 0,
            SysMemSlicePitch: 0,
        };

        let mut constant_buffer: Option<ID3D11Buffer> = None;
        self.d3d_device.CreateBuffer(&cb_desc, Some(&cb_init_data), Some(&mut constant_buffer))?;
        self.constant_buffer = constant_buffer;

        log_info!("Spectrum textures initialized (dual: {}, noise: {})",
                 state.spectrum_pair.has_dual_spectrum(),
                 state.noise_texture.is_some());

        Ok(())
    }

    pub fn render_frame(&mut self) -> Result<()> {
        #[cfg(windows)]
        unsafe {
            // Start frame timing
            self.frame_start_timer = std::time::Instant::now();

            // Capture frame from Desktop Duplication
            let pixels = match self.duplication.capture_frame()? {
                Some(p) => p,
                None => return Ok(()), // No new frame
            };

            // Create or update capture texture
            if self.capture_texture.is_none() {
                let texture_desc = D3D11_TEXTURE2D_DESC {
                    Width: self.width,
                    Height: self.height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DYNAMIC,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                    MiscFlags: 0,
                };

                let mut texture: Option<ID3D11Texture2D> = None;
                self.d3d_device.CreateTexture2D(&texture_desc, None, Some(&mut texture))?;
                let texture = texture.unwrap();

                // Create shader resource view
                let mut srv: Option<ID3D11ShaderResourceView> = None;
                self.d3d_device.CreateShaderResourceView(&texture, None, Some(&mut srv))?;

                self.capture_texture = Some(texture);
                self.capture_srv = Some(srv.unwrap());
            }

            // Upload pixel data to texture
            if let Some(ref texture) = self.capture_texture {
                let mut mapped: D3D11_MAPPED_SUBRESOURCE = std::mem::zeroed();
                self.d3d_context.Map(
                    texture,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped),
                )?;

                // Copy pixel data
                let src_ptr = pixels.as_ptr();
                let dst_ptr = mapped.pData as *mut u8;
                let row_pitch = (self.width * 4) as usize;

                for y in 0..self.height as usize {
                    std::ptr::copy_nonoverlapping(
                        src_ptr.add(y * row_pitch),
                        dst_ptr.add(y * mapped.RowPitch as usize),
                        row_pitch,
                    );
                }

                self.d3d_context.Unmap(texture, 0);
            }

            // Get back buffer and create render target view
            let back_buffer: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
            let mut rtv: Option<ID3D11RenderTargetView> = None;
            self.d3d_device.CreateRenderTargetView(&back_buffer, None, Some(&mut rtv))?;
            let rtv = rtv.unwrap();

            // Clear back buffer to TRANSPARENT (important for DirectComposition)
            let clear_color = [0.0f32, 0.0, 0.0, 0.0]; // Fully transparent
            self.d3d_context.ClearRenderTargetView(&rtv, &clear_color);

            // Set up the rendering pipeline
            self.d3d_context.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);

            // Set viewport
            let viewport = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.width as f32,
                Height: self.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            self.d3d_context.RSSetViewports(Some(&[viewport]));

            // Set shaders
            self.d3d_context.VSSetShader(self.vertex_shader.as_ref(), None);
            self.d3d_context.PSSetShader(self.pixel_shader.as_ref(), None);

            // Set input layout
            self.d3d_context.IASetInputLayout(self.input_layout.as_ref());

            // Set primitive topology
            self.d3d_context.IASetPrimitiveTopology(windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

            // Set vertex buffer
            let stride = std::mem::size_of::<f32>() as u32 * 4; // 2 floats for pos, 2 for tex
            let offset = 0u32;
            if let Some(ref vb) = self.vertex_buffer {
                self.d3d_context.IASetVertexBuffers(
                    0,
                    1,
                    Some(&Some(vb.clone())),
                    Some(&stride),
                    Some(&offset),
                );
            }

            // Set textures: capture, spectrum1, spectrum2 (optional), noise (optional)
            let mut srvs: Vec<Option<ID3D11ShaderResourceView>> = vec![None; 4];

            // t0: Screen capture
            if let Some(ref srv) = self.capture_srv {
                srvs[0] = Some(srv.clone());
            }

            // t1: Spectrum1
            if let Some(ref srv) = self.spectrum1_srv {
                srvs[1] = Some(srv.clone());
            }

            // t2: Spectrum2 (if available)
            if let Some(ref srv) = self.spectrum2_srv {
                srvs[2] = Some(srv.clone());
            }

            // t3: Noise texture (if available)
            if let Some(ref srv) = self.noise_srv {
                srvs[3] = Some(srv.clone());
            }

            self.d3d_context.PSSetShaderResources(0, Some(&srvs));

            // Set samplers: s0 for screen, s1 for spectrum
            if let Some(ref sampler) = self.sampler_state {
                self.d3d_context.PSSetSamplers(0, Some(&[Some(sampler.clone())]));
            }
            if let Some(ref spectrum_sampler) = self.spectrum_sampler {
                self.d3d_context.PSSetSamplers(1, Some(&[Some(spectrum_sampler.clone())]));
            }

            // Set constant buffer (b0) - constant buffer was initialized on startup and doesn't change
            if let Some(ref cb) = self.constant_buffer {
                self.d3d_context.PSSetConstantBuffers(0, Some(&[Some(cb.clone())]));
            }

            // Set blend state for transparency
            if let Some(ref blend) = self.blend_state {
                let blend_factor = [1.0f32, 1.0, 1.0, 1.0];
                self.d3d_context.OMSetBlendState(Some(blend), Some(&blend_factor), 0xffffffff);
            }

            // Draw fullscreen quad (6 vertices = 2 triangles)
            self.d3d_context.Draw(6, 0);

            // Calculate actual frame computation time (before VSync wait)
            let frame_compute_time_ms = self.frame_start_timer.elapsed().as_secs_f32() * 1000.0;

            // Update FPS counter
            self.frame_count += 1;
            let elapsed = self.fps_timer.elapsed().as_secs_f32();
            if elapsed >= 0.5 {
                let fps = self.frame_count as f32 / elapsed;

                let mut state = self.state.write();
                state.fps = fps;
                state.frame_time_ms = frame_compute_time_ms;

                // TODO: Render on-screen debug overlay if debug_overlay is enabled
                // Show: Resolution, FPS, Frame Time, Spectrum, Noise, Strength

                self.frame_count = 0;
                self.fps_timer = std::time::Instant::now();
            }

            // Present with VSync (1) to sync to monitor refresh rate
            self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()?;

            // Commit DirectComposition changes
            self.dcomp_device.Commit()?;
        }

        Ok(())
    }


    pub fn run_message_loop(&mut self) -> Result<()> {
        #[cfg(windows)]
        unsafe {
            let mut msg = MSG::default();
            let mut last_error_log = std::time::Instant::now();
            let mut error_count = 0u32;

            loop {
                // Non-blocking message processing
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        if error_count > 0 {
                            log_warn!("Exiting with {} render errors encountered", error_count);
                        }
                        return Ok(());
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                // Render frame with Desktop Duplication capture
                if let Err(e) = self.render_frame() {
                    error_count += 1;

                    // Rate-limited error logging (max 1 per second)
                    if last_error_log.elapsed().as_secs() >= 1 {
                        log_error!("Render error (count: {}): {}", error_count, e);
                        last_error_log = std::time::Instant::now();
                    }
                }

                // No sleep - run as fast as possible to match display refresh rate
            }
        }

        #[cfg(not(windows))]
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn new(_state: Arc<RwLock<OverlayState>>) -> Result<Self> {
        anyhow::bail!("DirectComposition only supported on Windows")
    }

    #[cfg(not(windows))]
    pub fn run_message_loop(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for DCompOverlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
