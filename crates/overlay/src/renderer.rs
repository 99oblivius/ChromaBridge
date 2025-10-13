use anyhow::Result;
use crate::{OverlayState, capture::DesktopDuplication};
use std::sync::Arc;
use std::time::Instant;
use parking_lot::RwLock;

#[allow(dead_code)]
struct OverlayApp {
    state: Arc<RwLock<OverlayState>>,
    duplication: DesktopDuplication,
    processed_texture: Option<egui::TextureHandle>,
    frame_count: u32,
    fps_update_time: Instant,
    window_initialized: bool,
}

#[allow(dead_code)]
impl OverlayApp {
    fn new(state: Arc<RwLock<OverlayState>>) -> Result<Self> {
        let monitor_index = state.read().monitor_index;
        let duplication = DesktopDuplication::new(monitor_index)?;

        Ok(Self {
            state,
            duplication,
            processed_texture: None,
            frame_count: 0,
            fps_update_time: Instant::now(),
            window_initialized: false,
        })
    }

    fn process_frame(&mut self, ctx: &egui::Context) -> Result<()> {
        // Capture frame from desktop
        if let Some(pixels) = self.duplication.capture_frame()? {
            let (width, height) = self.duplication.dimensions();

            // Convert BGRA to RGBA
            let mut processed = Vec::with_capacity((width * height) as usize);
            for chunk in pixels.chunks_exact(4) {
                let b = chunk[0];
                let g = chunk[1];
                let r = chunk[2];
                let a = chunk[3];
                processed.push(egui::Color32::from_rgba_unmultiplied(r, g, b, a));
            }

            // Update texture
            let color_image = egui::ColorImage {
                size: [width as usize, height as usize],
                pixels: processed,
            };

            if let Some(ref mut texture) = self.processed_texture {
                texture.set(color_image, egui::TextureOptions::NEAREST);
            } else {
                self.processed_texture = Some(ctx.load_texture(
                    "processed_frame",
                    color_image,
                    egui::TextureOptions::NEAREST,
                ));
            }

            self.frame_count += 1;
        }

        Ok(())
    }
}

impl egui_overlay::EguiOverlay for OverlayApp {
    fn gui_run(
        &mut self,
        ctx: &egui::Context,
        _three_d: &mut egui_overlay::egui_render_three_d::ThreeDBackend,
        glfw_backend: &mut egui_overlay::egui_window_glfw_passthrough::GlfwBackend,
    ) {
        // Always enable click passthrough
        glfw_backend.set_passthrough(true);

        // Initialize window position/size ONCE at startup
        if !self.window_initialized {
            let monitor_index = self.state.read().monitor_index;
            if let Ok((pos, size)) = crate::monitor::get_monitor_rect(monitor_index) {
                glfw_backend.window.set_pos(pos.0, pos.1);
                glfw_backend.set_window_size([size.0 as f32, size.1 as f32]);

                // CRITICAL: Exclude window from screen capture (like Xbox Game Bar)
                #[cfg(windows)]
                if let Some(hwnd) = crate::window_flags::get_current_window_hwnd() {
                    if let Err(e) = crate::window_flags::exclude_window_from_capture(hwnd) {
                        eprintln!("Warning: Failed to exclude window from capture: {}", e);
                    } else {
                        println!("✓ Window excluded from screen capture (WDA_EXCLUDEFROMCAPTURE)");
                    }
                }

                self.window_initialized = true;
            }
        }

        let frame_start = Instant::now();

        // Capture and process frame
        // Window is excluded from capture via WDA_EXCLUDEFROMCAPTURE flag
        if let Err(e) = self.process_frame(ctx) {
            eprintln!("Error processing frame: {}", e);
        }

        // Update FPS counter
        if self.fps_update_time.elapsed().as_secs_f32() >= 0.5 {
            let elapsed = self.fps_update_time.elapsed().as_secs_f32();
            let mut state = self.state.write();
            state.fps = self.frame_count as f32 / elapsed;
            state.frame_time_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
            self.frame_count = 0;
            self.fps_update_time = Instant::now();
        }

        // Render the captured and processed frame
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                if let Some(ref texture) = self.processed_texture {
                    let screen_rect = ui.ctx().screen_rect();
                    // Render fullscreen
                    ui.painter().image(
                        texture.id(),
                        screen_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            });

        // Request next frame
        ctx.request_repaint();
    }
}

#[allow(dead_code)]
pub fn run_overlay(state: Arc<RwLock<OverlayState>>) -> Result<()> {
    let app = OverlayApp::new(state)?;
    egui_overlay::start(app);
    Ok(())
}
