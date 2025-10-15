use crate::StateManager;
use chromabridge::{log_info, log_error, log_warn, SpectrumPair, NoiseTexture, HueMapper};
use anyhow::Result;
use std::sync::Arc;
use std::thread;
use parking_lot::{Mutex, RwLock};

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
            Gdi::*,
        },
        UI::WindowsAndMessaging::*,
        System::{Com::*, Threading::*},
    },
};

pub struct OverlayState {
    pub spectrum_pair: SpectrumPair,
    pub noise_texture: Option<NoiseTexture>,
    pub hue_mapper: HueMapper,
    pub vsync_enabled: bool,
    pub target_fps: Option<f32>,
}

pub struct OverlayManager {
    app_state: Arc<StateManager>,
    running: Arc<Mutex<bool>>,
    overlay_thread: Mutex<Option<thread::JoinHandle<()>>>,
    last_monitor: Mutex<Option<usize>>,
    frame_stats: Arc<Mutex<Option<(f32, f32)>>>, // (fps, frame_time_ms)
}

impl OverlayManager {
    pub fn new(state: Arc<StateManager>) -> Self {
        Self {
            app_state: state,
            running: Arc::new(Mutex::new(false)),
            overlay_thread: Mutex::new(None),
            last_monitor: Mutex::new(None),
            frame_stats: Arc::new(Mutex::new(None)),
        }
    }

    pub fn is_running(&self) -> bool {
        *self.running.lock()
    }

    pub fn get_frame_stats(&self) -> Option<(f32, f32)> {
        *self.frame_stats.lock()
    }

    pub fn toggle(&self) {
        let running = self.is_running();
        if running {
            self.stop();
        } else {
            self.start();
        }
    }

    pub fn start(&self) {
        let mut running = self.running.lock();
        if *running {
            return;
        }

        let (spectrum_name, noise_name, strength, monitor_index, vsync_enabled, target_fps) = self.app_state.read(|s| {
            (
                s.spectrum_name.clone(),
                s.noise_texture.clone(),
                s.strength,
                s.last_monitor.unwrap_or(0),
                s.vsync_enabled,
                s.target_fps,
            )
        });

        let spectrum_name = match spectrum_name {
            Some(name) => name,
            None => {
                log_error!("No spectrum selected");
                return;
            }
        };

        let spectrum_path = self.app_state.get_spectrum_path(&spectrum_name);
        let spectrum_pair = match SpectrumPair::load_from_file(spectrum_path) {
            Ok(sp) => {
                log_info!("Loaded spectrum: {}", spectrum_name);
                sp
            }
            Err(e) => {
                log_error!("Failed to load spectrum '{}': {}", spectrum_name, e);
                return;
            }
        };

        let noise_texture = if let Some(ref name) = noise_name {
            let noise_path = self.app_state.get_noise_path(name);
            match NoiseTexture::load_from_file(noise_path) {
                Ok(nt) => {
                    log_info!("Loaded noise texture: {}", name);
                    Some(nt)
                }
                Err(e) => {
                    log_error!("Failed to load noise texture '{}': {}", name, e);
                    None
                }
            }
        } else {
            None
        };

        let hue_mapper = HueMapper::new(strength);

        let running_flag = Arc::clone(&self.running);
        let frame_stats = Arc::clone(&self.frame_stats);
        *running = true;
        *self.last_monitor.lock() = Some(monitor_index);

        let handle = thread::spawn(move || {
            log_info!("Overlay thread started (Monitor {})", monitor_index);

            #[cfg(windows)]
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

                let monitor_info = match get_monitor_info(monitor_index) {
                    Ok(info) => info,
                    Err(e) => {
                        log_error!("Failed to get monitor info: {}", e);
                        *running_flag.lock() = false;
                        return;
                    }
                };

                let overlay_state = OverlayState {
                    spectrum_pair,
                    noise_texture,
                    hue_mapper,
                    vsync_enabled,
                    target_fps,
                };

                let overlay_state = Arc::new(RwLock::new(overlay_state));

                let result = (|| -> Result<()> {
                    let mut overlay = DCompOverlay::new(overlay_state, monitor_info, monitor_index, vsync_enabled, target_fps)?;
                    overlay.run_message_loop(&running_flag, &frame_stats)
                })();

                if let Err(e) = result {
                    log_error!("Overlay error: {}", e);
                }

                *running_flag.lock() = false;
                log_info!("Overlay thread ended");
            }

            #[cfg(not(windows))]
            {
                log_error!("Overlay is only supported on Windows");
                *running_flag.lock() = false;
            }
        });

        *self.overlay_thread.lock() = Some(handle);
        self.app_state.update(|s| {
            s.overlay_enabled = true;
            s.last_overlay_enabled = true;
        });
        log_info!("Overlay started (Monitor {}, Spectrum: {})", monitor_index, spectrum_name);
    }

    pub fn stop(&self) {
        let mut running = self.running.lock();
        if !*running {
            return;
        }

        *running = false;
        drop(running);

        if let Some(handle) = self.overlay_thread.lock().take() {
            let _ = handle.join();
        }

        // Clear frame stats
        *self.frame_stats.lock() = None;

        let monitor_idx = self.last_monitor.lock().take();
        self.app_state.update(|s| {
            s.overlay_enabled = false;
            s.last_overlay_enabled = false;
        });

        if let Some(idx) = monitor_idx {
            log_info!("Overlay stopped (Monitor {})", idx);
        } else {
            log_info!("Overlay stopped");
        }
    }
}

impl Drop for OverlayManager {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
#[derive(Clone)]
struct MonitorInfo {
    pos: (i32, i32),
    size: (i32, i32),
    refresh_rate: u32,
}

#[cfg(windows)]
unsafe fn get_monitor_info(monitor_index: usize) -> Result<MonitorInfo> {
    use std::sync::Mutex;

    let monitors = Mutex::new(Vec::<MonitorInfo>::new());

    let _ = EnumDisplayMonitors(
        None,
        None,
        Some(monitor_enum_proc),
        LPARAM(&monitors as *const _ as isize),
    );

    let monitors = monitors.into_inner().unwrap();

    if monitor_index >= monitors.len() {
        anyhow::bail!("Monitor index {} out of range (found {} monitors)", monitor_index, monitors.len());
    }

    Ok(monitors[monitor_index].clone())
}

#[cfg(windows)]
unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    use std::sync::Mutex;

    let monitors = &*(lparam.0 as *const Mutex<Vec<MonitorInfo>>);

    let mut info: MONITORINFOEXW = std::mem::zeroed();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _).as_bool() {
        let rect = info.monitorInfo.rcMonitor;
        let pos = (rect.left, rect.top);
        let size = (rect.right - rect.left, rect.bottom - rect.top);

        let refresh_rate = {
            let mut dev_mode: DEVMODEW = std::mem::zeroed();
            dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;

            if EnumDisplaySettingsW(
                PCWSTR(info.szDevice.as_ptr()),
                ENUM_CURRENT_SETTINGS,
                &mut dev_mode,
            ).as_bool() {
                dev_mode.dmDisplayFrequency
            } else {
                60
            }
        };

        monitors.lock().unwrap().push(MonitorInfo {
            pos,
            size,
            refresh_rate,
        });
    }

    true.into()
}

#[cfg(windows)]
struct DesktopDuplicator {
    output_duplication: IDXGIOutputDuplication,
    _d3d_device: ID3D11Device,
    _d3d_context: ID3D11DeviceContext,
}

#[cfg(windows)]
impl DesktopDuplicator {
    unsafe fn new(d3d_device: ID3D11Device, d3d_context: ID3D11DeviceContext, monitor_index: usize) -> Result<Self> {
        let dxgi_device: IDXGIDevice = d3d_device.cast()?;
        let dxgi_adapter = dxgi_device.GetAdapter()?;

        let output: IDXGIOutput = dxgi_adapter.EnumOutputs(monitor_index as u32)?;
        let output1: IDXGIOutput1 = output.cast()?;

        let output_duplication = output1.DuplicateOutput(&d3d_device)?;

        log_info!("Desktop duplication initialized for monitor {}", monitor_index);

        Ok(Self {
            output_duplication,
            _d3d_device: d3d_device,
            _d3d_context: d3d_context,
        })
    }

    unsafe fn acquire_next_frame(&mut self, timeout_ms: u32) -> Result<Option<ID3D11Texture2D>> {
        let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = std::mem::zeroed();
        let mut desktop_resource: Option<IDXGIResource> = None;

        match self.output_duplication.AcquireNextFrame(timeout_ms, &mut frame_info, &mut desktop_resource) {
            Ok(_) => {
                if let Some(resource) = desktop_resource {
                    let texture: ID3D11Texture2D = resource.cast()?;
                    Ok(Some(texture))
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                // DXGI_ERROR_WAIT_TIMEOUT means no new frame
                if e.code() == DXGI_ERROR_WAIT_TIMEOUT {
                    return Ok(None);
                }
                // DXGI_ERROR_ACCESS_LOST means we need to recreate the duplicator
                Err(anyhow::anyhow!("Failed to acquire frame: {:?}", e))
            }
        }
    }

    unsafe fn release_frame(&mut self) -> Result<()> {
        self.output_duplication.ReleaseFrame()?;
        Ok(())
    }
}

#[cfg(windows)]
struct DCompOverlay {
    _hwnd: HWND,
    d3d_device: ID3D11Device,
    d3d_context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain1,
    _dcomp_device: IDCompositionDevice,
    _dcomp_target: IDCompositionTarget,
    _dcomp_visual: IDCompositionVisual,

    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    input_layout: ID3D11InputLayout,
    vertex_buffer: ID3D11Buffer,
    sampler_state: ID3D11SamplerState,
    spectrum_sampler: ID3D11SamplerState,
    blend_state: ID3D11BlendState,

    spectrum1_srv: ID3D11ShaderResourceView,
    spectrum2_srv: Option<ID3D11ShaderResourceView>,
    noise_srv: Option<ID3D11ShaderResourceView>,
    constant_buffer: ID3D11Buffer,

    capture_texture: Option<ID3D11Texture2D>,
    capture_srv: Option<ID3D11ShaderResourceView>,

    desktop_duplication: Option<DesktopDuplicator>,

    width: u32,
    height: u32,
    vsync_enabled: bool,
    frame_latency_waitable: HANDLE,
    target_fps: Option<f32>,
}

#[cfg(windows)]
impl DCompOverlay {
    unsafe fn new(state: Arc<RwLock<OverlayState>>, monitor_info: MonitorInfo, monitor_index: usize, vsync_enabled: bool, target_fps: Option<f32>) -> Result<Self> {
        let (pos, size) = (monitor_info.pos, monitor_info.size);
        let width = size.0 as u32;
        let height = size.1 as u32;

        let hwnd = Self::create_overlay_window(pos, size)?;
        let (d3d_device, d3d_context) = Self::create_d3d_device()?;
        let swap_chain = Self::create_swap_chain(&d3d_device, width, height)?;

        // Get waitable handle and set max frame latency for proper frame pacing
        let swap_chain2: IDXGISwapChain2 = swap_chain.cast()?;
        swap_chain2.SetMaximumFrameLatency(1)?;
        let frame_latency_waitable = swap_chain2.GetFrameLatencyWaitableObject();
        log_info!("Frame latency waitable object initialized");

        let dcomp_device: IDCompositionDevice = DCompositionCreateDevice(None)?;
        let dcomp_target = dcomp_device.CreateTargetForHwnd(hwnd, true)?;
        let dcomp_visual = dcomp_device.CreateVisual()?;
        dcomp_visual.SetContent(&swap_chain)?;
        dcomp_target.SetRoot(&dcomp_visual)?;
        dcomp_device.Commit()?;

        log_info!("DirectComposition overlay initialized ({}x{} @ {},{}, {}Hz)",
                 width, height, pos.0, pos.1, monitor_info.refresh_rate);

        let (vertex_shader, pixel_shader, input_layout, vertex_buffer) = Self::init_rendering_pipeline(&d3d_device)?;
        let (sampler_state, spectrum_sampler, blend_state) = Self::create_render_states(&d3d_device)?;

        let (spectrum1_srv, spectrum2_srv, noise_srv, constant_buffer) = Self::init_spectrum_textures(&d3d_device, &state)?;

        // Initialize desktop duplication
        let desktop_duplication = match DesktopDuplicator::new(d3d_device.clone(), d3d_context.clone(), monitor_index) {
            Ok(dd) => Some(dd),
            Err(e) => {
                log_warn!("Failed to initialize desktop duplication: {}. Falling back to test pattern.", e);
                None
            }
        };

        Ok(Self {
            _hwnd: hwnd,
            d3d_device,
            d3d_context,
            swap_chain,
            _dcomp_device: dcomp_device,
            _dcomp_target: dcomp_target,
            _dcomp_visual: dcomp_visual,
            vertex_shader,
            pixel_shader,
            input_layout,
            vertex_buffer,
            sampler_state,
            spectrum_sampler,
            blend_state,
            spectrum1_srv,
            spectrum2_srv,
            noise_srv,
            constant_buffer,
            capture_texture: None,
            capture_srv: None,
            desktop_duplication,
            width,
            height,
            vsync_enabled,
            frame_latency_waitable,
            target_fps,
        })
    }

    unsafe fn create_overlay_window(pos: (i32, i32), size: (i32, i32)) -> Result<HWND> {
        let class_name = w!("ChromaBridgeOverlay");
        let hinstance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;

        let wc = WNDCLASSW {
            lpfnWndProc: Some(Self::window_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class_name,
            w!("ChromaBridge Overlay"),
            WS_POPUP,
            pos.0, pos.1, size.0, size.1,
            None, None,
            Some(HINSTANCE(hinstance.0)),
            None,
        )?;

        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TRANSPARENT.0 as i32);

        if let Err(e) = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) {
            log_warn!("Failed to exclude window from capture: {:?}", e);
        } else {
            log_info!("Window excluded from Desktop Duplication");
        }

        let _ = ShowWindow(hwnd, SW_SHOW);

        Ok(hwnd)
    }

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

    unsafe fn create_swap_chain(device: &ID3D11Device, width: u32, height: u32) -> Result<IDXGISwapChain1> {
        let dxgi_device = device.cast::<IDXGIDevice>()?;
        let dxgi_adapter = dxgi_device.GetAdapter()?;
        let dxgi_factory: IDXGIFactory2 = dxgi_adapter.GetParent()?;

        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            Flags: DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
            ..Default::default()
        };

        let swap_chain = dxgi_factory.CreateSwapChainForComposition(device, &swap_chain_desc, None)?;

        Ok(swap_chain)
    }

    fn run_message_loop(&mut self, running_flag: &Arc<Mutex<bool>>, frame_stats: &Arc<Mutex<Option<(f32, f32)>>>) -> Result<()> {
        #[cfg(windows)]
        unsafe {
            let mut msg = MSG::default();
            let mut last_error_log = std::time::Instant::now();
            let mut error_count = 0u32;

            // Frame timing tracking (render_time, total_time)
            let mut frame_times: Vec<(f32, f32)> = Vec::with_capacity(60);
            let mut last_stats_update = std::time::Instant::now();

            // Track time since last frame for accurate FPS capping
            let mut last_frame_time = std::time::Instant::now();

            loop {
                if !*running_flag.lock() {
                    log_info!("Overlay stop requested");
                    break;
                }

                // When VSync is disabled, wait for frame latency waitable object
                // This provides proper frame pacing without the latency of VSync
                if !self.vsync_enabled {
                    WaitForSingleObjectEx(self.frame_latency_waitable, INFINITE, false);
                }

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

                // Track frame start time (for stats)
                let frame_start = std::time::Instant::now();

                if let Err(e) = self.prepare_frame() {
                    error_count += 1;

                    if last_error_log.elapsed().as_secs() >= 1 {
                        log_error!("Render error (count: {}): {}", error_count, e);
                        last_error_log = std::time::Instant::now();
                    }
                }

                // Measure rendering time before Present (excludes VSync wait)
                let render_time_ms = frame_start.elapsed().as_secs_f32() * 1000.0;

                // Now call Present which will block on VSync
                let _ = self.present_frame();

                // Apply FPS cap if enabled - use time since last frame to account for all overhead
                if let Some(target_fps) = self.target_fps {
                    let target_frame_duration = std::time::Duration::from_secs_f32(1.0 / target_fps);
                    let elapsed_since_last = last_frame_time.elapsed();

                    if elapsed_since_last < target_frame_duration {
                        let remaining = target_frame_duration - elapsed_since_last;
                        // spin_sleep uses hybrid sleep/spin with platform-specific tuning
                        spin_sleep::sleep(remaining);
                    }
                }

                // Capture timestamp IMMEDIATELY to avoid gaps
                let now = std::time::Instant::now();
                let total_frame_time_ms = now.duration_since(last_frame_time).as_secs_f32() * 1000.0;
                last_frame_time = now;
                frame_times.push((render_time_ms, total_frame_time_ms));

                // Update stats every 100ms
                if last_stats_update.elapsed().as_millis() >= 100 && !frame_times.is_empty() {
                    // Calculate averages
                    let (sum_render, sum_total): (f32, f32) = frame_times.iter()
                        .fold((0.0, 0.0), |(r, t), &(render, total)| (r + render, t + total));
                    let avg_render_time = sum_render / frame_times.len() as f32;
                    let avg_total_time = sum_total / frame_times.len() as f32;

                    // FPS from total frame time (including VSync)
                    let fps = if avg_total_time > 0.0 {
                        1000.0 / avg_total_time
                    } else {
                        0.0
                    };

                    // Update shared stats (fps from total, but show render time)
                    *frame_stats.lock() = Some((fps, avg_render_time));

                    // Keep only last 60 frames for rolling average
                    if frame_times.len() > 60 {
                        frame_times.drain(0..frame_times.len() - 60);
                    }

                    last_stats_update = std::time::Instant::now();
                }
            }

            Ok(())
        }

        #[cfg(not(windows))]
        Ok(())
    }

    #[cfg(windows)]
    unsafe fn prepare_frame(&mut self) -> Result<()> {
        // Try to acquire a new frame from desktop duplication
        if let Some(ref mut duplicator) = self.desktop_duplication {
            if let Some(acquired_texture) = duplicator.acquire_next_frame(0)? {
                // Copy the acquired frame to our capture texture
                if self.capture_texture.is_none() {
                    // Create a staging texture that can be used as a shader resource
                    let texture_desc = D3D11_TEXTURE2D_DESC {
                        Width: self.width,
                        Height: self.height,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                        Usage: D3D11_USAGE_DEFAULT,
                        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                        CPUAccessFlags: 0,
                        MiscFlags: 0,
                    };

                    let mut texture: Option<ID3D11Texture2D> = None;
                    self.d3d_device.CreateTexture2D(&texture_desc, None, Some(&mut texture))?;
                    let texture = texture.unwrap();

                    let mut srv: Option<ID3D11ShaderResourceView> = None;
                    self.d3d_device.CreateShaderResourceView(&texture, None, Some(&mut srv))?;

                    self.capture_texture = Some(texture);
                    self.capture_srv = Some(srv.unwrap());
                }

                // Copy the acquired frame to our texture
                if let Some(ref capture_texture) = self.capture_texture {
                    self.d3d_context.CopyResource(capture_texture, &acquired_texture);
                }

                // Release the acquired frame
                duplicator.release_frame()?;
            }
        } else if self.capture_texture.is_none() {
            // Fallback: Create test pattern if desktop duplication is not available
            let mut test_pixels = vec![0u8; (self.width * self.height * 4) as usize];
            for y in 0..self.height {
                for x in 0..self.width {
                    let idx = ((y * self.width + x) * 4) as usize;
                    let r = ((x as f32 / self.width as f32) * 255.0) as u8;
                    let g = ((y as f32 / self.height as f32) * 255.0) as u8;
                    let b = 128u8;
                    test_pixels[idx] = b;
                    test_pixels[idx + 1] = g;
                    test_pixels[idx + 2] = r;
                    test_pixels[idx + 3] = 255;
                }
            }

            let texture_desc = D3D11_TEXTURE2D_DESC {
                Width: self.width,
                Height: self.height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                MiscFlags: 0,
            };

            let texture_data = D3D11_SUBRESOURCE_DATA {
                pSysMem: test_pixels.as_ptr() as *const _,
                SysMemPitch: self.width * 4,
                SysMemSlicePitch: 0,
            };

            let mut texture: Option<ID3D11Texture2D> = None;
            self.d3d_device.CreateTexture2D(&texture_desc, Some(&texture_data), Some(&mut texture))?;
            let texture = texture.unwrap();

            let mut srv: Option<ID3D11ShaderResourceView> = None;
            self.d3d_device.CreateShaderResourceView(&texture, None, Some(&mut srv))?;

            self.capture_texture = Some(texture);
            self.capture_srv = Some(srv.unwrap());
        }

        let back_buffer: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
        let mut rtv: Option<ID3D11RenderTargetView> = None;
        self.d3d_device.CreateRenderTargetView(&back_buffer, None, Some(&mut rtv))?;
        let rtv = rtv.unwrap();

        let clear_color = [0.0f32, 0.0, 0.0, 0.0];
        self.d3d_context.ClearRenderTargetView(&rtv, &clear_color);

        self.d3d_context.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);

        let viewport = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: self.width as f32,
            Height: self.height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        self.d3d_context.RSSetViewports(Some(&[viewport]));

        self.d3d_context.VSSetShader(&self.vertex_shader, None);
        self.d3d_context.PSSetShader(&self.pixel_shader, None);
        self.d3d_context.IASetInputLayout(&self.input_layout);
        self.d3d_context.IASetPrimitiveTopology(windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

        let stride = std::mem::size_of::<f32>() as u32 * 4;
        let offset = 0u32;
        self.d3d_context.IASetVertexBuffers(0, 1, Some(&Some(self.vertex_buffer.clone())), Some(&stride), Some(&offset));

        let mut srvs: Vec<Option<ID3D11ShaderResourceView>> = vec![None; 4];
        if let Some(ref srv) = self.capture_srv {
            srvs[0] = Some(srv.clone());
        }
        srvs[1] = Some(self.spectrum1_srv.clone());
        if let Some(ref srv) = self.spectrum2_srv {
            srvs[2] = Some(srv.clone());
        }
        if let Some(ref srv) = self.noise_srv {
            srvs[3] = Some(srv.clone());
        }

        self.d3d_context.PSSetShaderResources(0, Some(&srvs));
        self.d3d_context.PSSetSamplers(0, Some(&[Some(self.sampler_state.clone())]));
        self.d3d_context.PSSetSamplers(1, Some(&[Some(self.spectrum_sampler.clone())]));
        self.d3d_context.PSSetConstantBuffers(0, Some(&[Some(self.constant_buffer.clone())]));

        let blend_factor = [1.0f32, 1.0, 1.0, 1.0];
        self.d3d_context.OMSetBlendState(Some(&self.blend_state), Some(&blend_factor), 0xffffffff);

        self.d3d_context.Draw(6, 0);

        Ok(())
    }

    #[cfg(windows)]
    unsafe fn present_frame(&mut self) -> Result<()> {
        // sync_interval: 0 = no vsync, 1 = vsync to refresh rate
        let sync_interval = if self.vsync_enabled { 1 } else { 0 };
        self.swap_chain.Present(sync_interval, DXGI_PRESENT(0)).ok()?;
        Ok(())
    }

    unsafe fn init_rendering_pipeline(device: &ID3D11Device) -> Result<(ID3D11VertexShader, ID3D11PixelShader, ID3D11InputLayout, ID3D11Buffer)> {
        const SHADER_SOURCE: &str = include_str!("shaders.hlsl");

        let vs_blob = Self::compile_shader(SHADER_SOURCE, "VS_Main", "vs_5_0")?;
        let mut vertex_shader: Option<ID3D11VertexShader> = None;
        device.CreateVertexShader(
            std::slice::from_raw_parts(
                vs_blob.GetBufferPointer() as *const u8,
                vs_blob.GetBufferSize(),
            ),
            None,
            Some(&mut vertex_shader),
        )?;

        let ps_blob = Self::compile_shader(SHADER_SOURCE, "PS_Main", "ps_5_0")?;
        let mut pixel_shader: Option<ID3D11PixelShader> = None;
        device.CreatePixelShader(
            std::slice::from_raw_parts(
                ps_blob.GetBufferPointer() as *const u8,
                ps_blob.GetBufferSize(),
            ),
            None,
            Some(&mut pixel_shader),
        )?;

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
        device.CreateInputLayout(
            &input_elements,
            std::slice::from_raw_parts(
                vs_blob.GetBufferPointer() as *const u8,
                vs_blob.GetBufferSize(),
            ),
            Some(&mut input_layout),
        )?;

        #[repr(C)]
        struct Vertex {
            pos: [f32; 2],
            tex: [f32; 2],
        }

        let vertices = [
            Vertex { pos: [-1.0, 1.0], tex: [0.0, 0.0] },
            Vertex { pos: [1.0, 1.0], tex: [1.0, 0.0] },
            Vertex { pos: [-1.0, -1.0], tex: [0.0, 1.0] },
            Vertex { pos: [1.0, 1.0], tex: [1.0, 0.0] },
            Vertex { pos: [1.0, -1.0], tex: [1.0, 1.0] },
            Vertex { pos: [-1.0, -1.0], tex: [0.0, 1.0] },
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
        device.CreateBuffer(&buffer_desc, Some(&vertex_data), Some(&mut vertex_buffer))?;

        Ok((vertex_shader.unwrap(), pixel_shader.unwrap(), input_layout.unwrap(), vertex_buffer.unwrap()))
    }

    unsafe fn compile_shader(source: &str, entry_point: &str, target: &str) -> Result<windows::Win32::Graphics::Direct3D::ID3DBlob> {
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

    unsafe fn create_render_states(device: &ID3D11Device) -> Result<(ID3D11SamplerState, ID3D11SamplerState, ID3D11BlendState)> {
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
        device.CreateSamplerState(&sampler_desc, Some(&mut sampler_state))?;

        let mut spectrum_sampler: Option<ID3D11SamplerState> = None;
        device.CreateSamplerState(&sampler_desc, Some(&mut spectrum_sampler))?;

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
        device.CreateBlendState(&blend_desc, Some(&mut blend_state))?;

        Ok((sampler_state.unwrap(), spectrum_sampler.unwrap(), blend_state.unwrap()))
    }

    unsafe fn init_spectrum_textures(device: &ID3D11Device, state: &Arc<RwLock<OverlayState>>) -> Result<(ID3D11ShaderResourceView, Option<ID3D11ShaderResourceView>, Option<ID3D11ShaderResourceView>, ID3D11Buffer)> {
        const SPECTRUM_RESOLUTION: usize = 360;

        let state_read = state.read();

        let spectrum1_data = state_read.spectrum_pair.spectrum1.get_rgb_lookup_table(SPECTRUM_RESOLUTION)?;

        let spectrum_desc = D3D11_TEXTURE2D_DESC {
            Width: SPECTRUM_RESOLUTION as u32,
            Height: 1,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R32G32B32_FLOAT,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
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
        device.CreateTexture2D(&spectrum_desc, Some(&spectrum1_init_data), Some(&mut spectrum1_texture))?;

        let mut spectrum1_srv: Option<ID3D11ShaderResourceView> = None;
        device.CreateShaderResourceView(&spectrum1_texture.unwrap(), None, Some(&mut spectrum1_srv))?;

        let spectrum2_srv = if let Some(ref spectrum2) = state_read.spectrum_pair.spectrum2 {
            let spectrum2_data = spectrum2.get_rgb_lookup_table(SPECTRUM_RESOLUTION)?;
            let spectrum2_init_data = D3D11_SUBRESOURCE_DATA {
                pSysMem: spectrum2_data.as_ptr() as *const _,
                SysMemPitch: (SPECTRUM_RESOLUTION * 3 * std::mem::size_of::<f32>()) as u32,
                SysMemSlicePitch: 0,
            };

            let mut spectrum2_texture: Option<ID3D11Texture2D> = None;
            device.CreateTexture2D(&spectrum_desc, Some(&spectrum2_init_data), Some(&mut spectrum2_texture))?;

            let mut srv: Option<ID3D11ShaderResourceView> = None;
            device.CreateShaderResourceView(&spectrum2_texture.unwrap(), None, Some(&mut srv))?;
            Some(srv.unwrap())
        } else {
            None
        };

        let noise_srv = if let Some(ref noise_texture) = state_read.noise_texture {
            let noise_width = noise_texture.width();
            let noise_height = noise_texture.height();

            let mut noise_data: Vec<u8> = Vec::with_capacity((noise_width * noise_height) as usize);
            for y in 0..noise_height {
                for x in 0..noise_width {
                    let value = if noise_texture.sample(x, y, noise_width, noise_height) {
                        255u8
                    } else {
                        0u8
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
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
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
            device.CreateTexture2D(&noise_desc, Some(&noise_init_data), Some(&mut noise_texture_d3d))?;

            let mut srv: Option<ID3D11ShaderResourceView> = None;
            device.CreateShaderResourceView(&noise_texture_d3d.unwrap(), None, Some(&mut srv))?;
            Some(srv.unwrap())
        } else {
            None
        };

        #[repr(C)]
        struct SpectrumParams {
            strength: f32,
            use_dual_spectrum: i32,
            use_noise_texture: i32,
            padding: f32,
        }

        let params = SpectrumParams {
            strength: state_read.hue_mapper.strength,
            use_dual_spectrum: if state_read.spectrum_pair.has_dual_spectrum() { 1 } else { 0 },
            use_noise_texture: if state_read.noise_texture.is_some() { 1 } else { 0 },
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
        device.CreateBuffer(&cb_desc, Some(&cb_init_data), Some(&mut constant_buffer))?;

        log_info!("Spectrum textures initialized (dual: {}, noise: {})",
                 state_read.spectrum_pair.has_dual_spectrum(),
                 state_read.noise_texture.is_some());

        Ok((spectrum1_srv.unwrap(), spectrum2_srv, noise_srv, constant_buffer.unwrap()))
    }
}
