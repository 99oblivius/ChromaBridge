use anyhow::{Context, Result};
use image::ImageReader;
use std::path::Path;

pub struct NoiseTexture {
    width: u32,
    height: u32,
    data: Vec<bool>,
}

impl NoiseTexture {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let img = ImageReader::open(path.as_ref())
            .context("Failed to open noise texture file")?
            .decode()
            .context("Failed to decode noise texture")?;

        let gray = img.to_luma8();
        let (width, height) = gray.dimensions();

        let data: Vec<bool> = gray.pixels().map(|p| p.0[0] > 128).collect();

        Ok(Self {
            width,
            height,
            data,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn sample(&self, display_x: u32, display_y: u32, display_width: u32, display_height: u32) -> bool {
        let texture_aspect = self.width as f32 / self.height as f32;
        let display_aspect = display_width as f32 / display_height as f32;

        let (scale_x, scale_y, offset_x, offset_y) = if texture_aspect > display_aspect {
            let scale = self.width as f32 / display_width as f32;
            let scaled_height = (self.height as f32 / scale) as u32;
            let offset_y = (display_height.saturating_sub(scaled_height)) / 2;
            (scale, scale, 0, offset_y)
        } else {
            let scale = self.height as f32 / display_height as f32;
            let scaled_width = (self.width as f32 / scale) as u32;
            let offset_x = (display_width.saturating_sub(scaled_width)) / 2;
            (scale, scale, offset_x, 0)
        };

        if display_x < offset_x || display_y < offset_y {
            return false;
        }

        let adjusted_x = display_x - offset_x;
        let adjusted_y = display_y - offset_y;

        let tex_x = ((adjusted_x as f32 * scale_x) as u32).min(self.width - 1);
        let tex_y = ((adjusted_y as f32 * scale_y) as u32).min(self.height - 1);

        let idx = (tex_y * self.width + tex_x) as usize;
        self.data.get(idx).copied().unwrap_or(false)
    }
}
