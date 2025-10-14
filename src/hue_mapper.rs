pub struct HueMapper {
    pub strength: f32,
}

impl HueMapper {
    pub fn new(strength: f32) -> Self {
        Self {
            strength: strength.clamp(0.0, 1.0),
        }
    }

    pub fn set_strength(&mut self, strength: f32) {
        self.strength = strength.clamp(0.0, 1.0);
    }

    pub fn get_strength(&self) -> f32 {
        self.strength
    }

    pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let v = max;
        let s = if max == 0.0 { 0.0 } else { delta / max };

        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };

        let h = if h < 0.0 { h + 360.0 } else { h };

        (h, s, v)
    }

    pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
        let h = h % 360.0;
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;

        let (r, g, b) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        let r = ((r + m) * 255.0).round() as u8;
        let g = ((g + m) * 255.0).round() as u8;
        let b = ((b + m) * 255.0).round() as u8;

        (r, g, b)
    }
}
