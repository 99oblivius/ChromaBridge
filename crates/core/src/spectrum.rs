use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// A single node in the spectrum gradient
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrumNode {
    /// Position along the spectrum (0.0 to 1.0, maps to 0° to 360° input hue)
    pub position: f32,
    /// RGB color at this position (hex format like "#FFFF00")
    /// If hue/saturation/value are not specified, they will be extracted from this color
    pub color: String,
    /// Optional: Target hue in degrees (0-360). If specified, overrides hue from color
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hue: Option<f32>,
    /// Optional: Target saturation (0.0-1.0). If not specified, uses saturation from color or 1.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saturation: Option<f32>,
    /// Optional: Target value/brightness (0.0-1.0). If not specified, uses value from color or 1.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f32>,
}

impl SpectrumNode {
    /// Parse hex color to RGB (0.0-1.0 range)
    pub fn to_rgb(&self) -> Result<(f32, f32, f32)> {
        let hex = self.color.trim_start_matches('#');

        if hex.len() != 6 {
            anyhow::bail!("Invalid hex color format: {}", self.color);
        }

        let r = u8::from_str_radix(&hex[0..2], 16)
            .context("Failed to parse red component")?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .context("Failed to parse green component")?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .context("Failed to parse blue component")?;

        Ok((r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0))
    }

    /// Get HSV values for this node
    /// Returns (hue in degrees 0-360, saturation 0-1, value 0-1)
    pub fn to_hsv(&self) -> Result<(f32, f32, f32)> {
        use crate::hue_mapper::HueMapper;

        // If explicit HSV values are provided, use them
        let h = if let Some(hue) = self.hue {
            hue
        } else {
            // Extract hue from color
            let rgb = self.to_rgb()?;
            let (r_u8, g_u8, b_u8) = (
                (rgb.0 * 255.0) as u8,
                (rgb.1 * 255.0) as u8,
                (rgb.2 * 255.0) as u8,
            );
            let (h, _, _) = HueMapper::rgb_to_hsv(r_u8, g_u8, b_u8);
            h
        };

        // Use explicit saturation or extract from color, default to 1.0
        let s = if let Some(sat) = self.saturation {
            sat
        } else {
            let rgb = self.to_rgb()?;
            let (r_u8, g_u8, b_u8) = (
                (rgb.0 * 255.0) as u8,
                (rgb.1 * 255.0) as u8,
                (rgb.2 * 255.0) as u8,
            );
            let (_, s, _) = HueMapper::rgb_to_hsv(r_u8, g_u8, b_u8);
            s
        };

        // Use explicit value or extract from color, default to 1.0
        let v = if let Some(val) = self.value {
            val
        } else {
            let rgb = self.to_rgb()?;
            let (r_u8, g_u8, b_u8) = (
                (rgb.0 * 255.0) as u8,
                (rgb.1 * 255.0) as u8,
                (rgb.2 * 255.0) as u8,
            );
            let (_, _, v) = HueMapper::rgb_to_hsv(r_u8, g_u8, b_u8);
            v
        };

        Ok((h, s, v))
    }
}

/// A single spectrum gradient defined by a series of color nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spectrum {
    pub nodes: Vec<SpectrumNode>,
}

impl Spectrum {
    /// Validate that nodes are properly ordered
    pub fn validate(&self) -> Result<()> {
        if self.nodes.is_empty() {
            anyhow::bail!("Spectrum must have at least one node");
        }

        // Check that positions are sorted and in valid range
        let mut last_pos = -0.1;
        for node in &self.nodes {
            if node.position < 0.0 || node.position > 1.0 {
                anyhow::bail!("Node position {} out of range [0.0, 1.0]", node.position);
            }
            if node.position < last_pos {
                anyhow::bail!("Nodes must be sorted by position");
            }
            last_pos = node.position;

            // Validate color format
            node.to_rgb()?;

            // Validate optional HSV values
            if let Some(hue) = node.hue {
                if hue < 0.0 || hue >= 360.0 {
                    anyhow::bail!("Node hue {} out of range [0.0, 360.0)", hue);
                }
            }
            if let Some(sat) = node.saturation {
                if sat < 0.0 || sat > 1.0 {
                    anyhow::bail!("Node saturation {} out of range [0.0, 1.0]", sat);
                }
            }
            if let Some(val) = node.value {
                if val < 0.0 || val > 1.0 {
                    anyhow::bail!("Node value {} out of range [0.0, 1.0]", val);
                }
            }
        }

        Ok(())
    }

    /// Map input hue (0-360°) to RGB color from spectrum
    /// The spectrum defines what color each input hue should map to
    /// Interpolates directly in RGB space to avoid hue shifts between nodes
    pub fn map_hue_to_rgb(&self, input_hue: f32) -> Result<(f32, f32, f32)> {
        use crate::hue_mapper::HueMapper;

        // Normalize hue to 0.0-1.0 range
        let position = (input_hue % 360.0) / 360.0;

        if self.nodes.is_empty() {
            anyhow::bail!("Cannot map hue: spectrum has no nodes");
        }

        // Single node - convert HSV to RGB and return
        if self.nodes.len() == 1 {
            let (h, s, v) = self.nodes[0].to_hsv()?;
            let (r_u8, g_u8, b_u8) = HueMapper::hsv_to_rgb(h, s, v);
            return Ok((r_u8 as f32 / 255.0, g_u8 as f32 / 255.0, b_u8 as f32 / 255.0));
        }

        // Find the two nodes to interpolate between
        for i in 0..self.nodes.len() - 1 {
            let node1 = &self.nodes[i];
            let node2 = &self.nodes[i + 1];

            if position >= node1.position && position <= node2.position {
                // Calculate interpolation factor
                let t = if node2.position > node1.position {
                    (position - node1.position) / (node2.position - node1.position)
                } else {
                    0.0
                };

                // Get HSV for both nodes and convert to RGB
                let (h1, s1, v1) = node1.to_hsv()?;
                let (h2, s2, v2) = node2.to_hsv()?;

                let (r1_u8, g1_u8, b1_u8) = HueMapper::hsv_to_rgb(h1, s1, v1);
                let (r2_u8, g2_u8, b2_u8) = HueMapper::hsv_to_rgb(h2, s2, v2);

                // Linear interpolation in RGB space
                // This prevents hue shifts when interpolating between desaturated colors
                let r1 = r1_u8 as f32 / 255.0;
                let g1 = g1_u8 as f32 / 255.0;
                let b1 = b1_u8 as f32 / 255.0;

                let r2 = r2_u8 as f32 / 255.0;
                let g2 = g2_u8 as f32 / 255.0;
                let b2 = b2_u8 as f32 / 255.0;

                let r = r1 + t * (r2 - r1);
                let g = g1 + t * (g2 - g1);
                let b = b1 + t * (b2 - b1);

                return Ok((r, g, b));
            }
        }

        // If position is beyond last node, use last node
        if position >= self.nodes.last().unwrap().position {
            let (h, s, v) = self.nodes.last().unwrap().to_hsv()?;
            let (r_u8, g_u8, b_u8) = HueMapper::hsv_to_rgb(h, s, v);
            return Ok((r_u8 as f32 / 255.0, g_u8 as f32 / 255.0, b_u8 as f32 / 255.0));
        }

        // If position is before first node, use first node
        let (h, s, v) = self.nodes.first().unwrap().to_hsv()?;
        let (r_u8, g_u8, b_u8) = HueMapper::hsv_to_rgb(h, s, v);
        Ok((r_u8 as f32 / 255.0, g_u8 as f32 / 255.0, b_u8 as f32 / 255.0))
    }

    /// Get flattened RGB data for GPU upload (for texture/buffer)
    /// Returns a Vec of RGB values (each 0.0-1.0) with specified resolution
    pub fn get_rgb_lookup_table(&self, resolution: usize) -> Result<Vec<f32>> {
        let mut table = Vec::with_capacity(resolution * 3);

        for i in 0..resolution {
            let hue = (i as f32 / resolution as f32) * 360.0;
            let (r, g, b) = self.map_hue_to_rgb(hue)?;
            table.push(r);
            table.push(g);
            table.push(b);
        }

        Ok(table)
    }
}

/// Container for spectrum file (supports single or dual spectrum)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrumFile {
    pub spectra: Vec<Spectrum>,
}

#[derive(Debug, Clone)]
pub struct SpectrumPair {
    pub spectrum1: Spectrum,
    pub spectrum2: Option<Spectrum>,
}

impl SpectrumPair {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .context("Failed to read spectrum file")?;

        let spectrum_file: SpectrumFile = serde_json::from_str(&content)
            .context("Failed to parse spectrum file")?;

        if spectrum_file.spectra.is_empty() {
            anyhow::bail!("Spectrum file must contain at least one spectrum");
        }

        // Validate all spectra
        for spectrum in &spectrum_file.spectra {
            spectrum.validate()?;
        }

        match spectrum_file.spectra.len() {
            1 => Ok(Self {
                spectrum1: spectrum_file.spectra[0].clone(),
                spectrum2: None,
            }),
            _ => Ok(Self {
                spectrum1: spectrum_file.spectra[0].clone(),
                spectrum2: Some(spectrum_file.spectra[1].clone()),
            }),
        }
    }

    pub fn has_dual_spectrum(&self) -> bool {
        self.spectrum2.is_some()
    }
}
