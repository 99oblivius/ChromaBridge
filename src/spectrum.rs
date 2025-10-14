use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrumNode {
    pub position: f32,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hue: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saturation: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f32>,
}

impl SpectrumNode {
    pub fn to_rgb(&self) -> Result<(f32, f32, f32)> {
        let hex = self.color.trim_start_matches('#');

        if hex.len() != 6 {
            anyhow::bail!("Invalid hex color format: {}", self.color);
        }

        let r = u8::from_str_radix(&hex[0..2], 16).context("Failed to parse red component")?;
        let g = u8::from_str_radix(&hex[2..4], 16).context("Failed to parse green component")?;
        let b = u8::from_str_radix(&hex[4..6], 16).context("Failed to parse blue component")?;

        Ok((r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0))
    }

    pub fn to_hsv(&self) -> Result<(f32, f32, f32)> {
        use crate::hue_mapper::HueMapper;

        let h = if let Some(hue) = self.hue {
            hue
        } else {
            let rgb = self.to_rgb()?;
            let (r_u8, g_u8, b_u8) = (
                (rgb.0 * 255.0) as u8,
                (rgb.1 * 255.0) as u8,
                (rgb.2 * 255.0) as u8,
            );
            let (h, _, _) = HueMapper::rgb_to_hsv(r_u8, g_u8, b_u8);
            h
        };

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spectrum {
    pub nodes: Vec<SpectrumNode>,
}

impl Spectrum {
    pub fn validate(&self) -> Result<()> {
        if self.nodes.is_empty() {
            anyhow::bail!("Spectrum must have at least one node");
        }

        let mut last_pos = -0.1;
        for node in &self.nodes {
            if node.position < 0.0 || node.position > 1.0 {
                anyhow::bail!("Node position {} out of range [0.0, 1.0]", node.position);
            }
            if node.position < last_pos {
                anyhow::bail!("Nodes must be sorted by position");
            }
            last_pos = node.position;

            node.to_rgb()?;

            if let Some(hue) = node.hue {
                if !(0.0..360.0).contains(&hue) {
                    anyhow::bail!("Node hue {} out of range [0.0, 360.0)", hue);
                }
            }
            if let Some(sat) = node.saturation {
                if !(0.0..=1.0).contains(&sat) {
                    anyhow::bail!("Node saturation {} out of range [0.0, 1.0]", sat);
                }
            }
            if let Some(val) = node.value {
                if !(0.0..=1.0).contains(&val) {
                    anyhow::bail!("Node value {} out of range [0.0, 1.0]", val);
                }
            }
        }

        Ok(())
    }

    pub fn map_hue_to_rgb(&self, input_hue: f32) -> Result<(f32, f32, f32)> {
        use crate::hue_mapper::HueMapper;

        let position = (input_hue % 360.0) / 360.0;

        if self.nodes.is_empty() {
            anyhow::bail!("Cannot map hue: spectrum has no nodes");
        }

        if self.nodes.len() == 1 {
            let (h, s, v) = self.nodes[0].to_hsv()?;
            let (r_u8, g_u8, b_u8) = HueMapper::hsv_to_rgb(h, s, v);
            return Ok((r_u8 as f32 / 255.0, g_u8 as f32 / 255.0, b_u8 as f32 / 255.0));
        }

        for i in 0..self.nodes.len() - 1 {
            let node1 = &self.nodes[i];
            let node2 = &self.nodes[i + 1];

            if position >= node1.position && position <= node2.position {
                let t = if node2.position > node1.position {
                    (position - node1.position) / (node2.position - node1.position)
                } else {
                    0.0
                };

                let (h1, s1, v1) = node1.to_hsv()?;
                let (h2, s2, v2) = node2.to_hsv()?;

                let (r1_u8, g1_u8, b1_u8) = HueMapper::hsv_to_rgb(h1, s1, v1);
                let (r2_u8, g2_u8, b2_u8) = HueMapper::hsv_to_rgb(h2, s2, v2);

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

        if position >= self.nodes.last().unwrap().position {
            let (h, s, v) = self.nodes.last().unwrap().to_hsv()?;
            let (r_u8, g_u8, b_u8) = HueMapper::hsv_to_rgb(h, s, v);
            return Ok((r_u8 as f32 / 255.0, g_u8 as f32 / 255.0, b_u8 as f32 / 255.0));
        }

        let (h, s, v) = self.nodes.first().unwrap().to_hsv()?;
        let (r_u8, g_u8, b_u8) = HueMapper::hsv_to_rgb(h, s, v);
        Ok((r_u8 as f32 / 255.0, g_u8 as f32 / 255.0, b_u8 as f32 / 255.0))
    }

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
        let content = fs::read_to_string(path.as_ref()).context("Failed to read spectrum file")?;

        let spectrum_file: SpectrumFile = serde_json::from_str(&content)
            .context("Failed to parse spectrum file")?;

        if spectrum_file.spectra.is_empty() {
            anyhow::bail!("Spectrum file must contain at least one spectrum");
        }

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
