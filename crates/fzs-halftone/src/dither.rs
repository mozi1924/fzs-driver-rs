//! Halftoning and binarization algorithms (Smart Hybrid, Floyd-Steinberg, Bayer 4x4, Threshold).

use image::GrayImage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DitheringMethod {
    /// Smart Hybrid: Edge-aware binarization with sharp text thresholding
    /// and continuous-tone Floyd-Steinberg error diffusion for photos.
    #[default]
    SmartHybrid,
    /// Pure Floyd-Steinberg error diffusion (ideal for continuous photos).
    FloydSteinberg,
    /// Bayer 4x4 ordered dithering (matches legacy thermal pattern).
    Bayer,
    /// Fixed threshold binarization (high contrast, text-only).
    Threshold,
}

/// 4x4 Bayer dithering matrix matching official implementation.
pub const BAYER_4X4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Binarize a grayscale image using Bayer 4x4 ordered dithering.
///
/// Returns a 1D boolean array of length `width * height`,
/// where `true` = black (heated dot), `false` = white (paper background).
pub fn binarize_bayer(img: &GrayImage) -> Vec<bool> {
    let (width, height) = img.dimensions();
    let mut out = vec![false; (width * height) as usize];

    for y in 0..height {
        let row_offset = (y * width) as usize;
        let y_idx = (y & 3) as usize;
        for x in 0..width {
            let lum = img.get_pixel(x, y)[0];
            let x_idx = (x & 3) as usize;
            let threshold = (BAYER_4X4[x_idx][y_idx] as u16 * 16) + 8;
            out[row_offset + x as usize] = (lum as u16) < threshold;
        }
    }
    out
}

/// Binarize a grayscale image using a fixed threshold (default 180).
///
/// Returns a 1D boolean array where `true` = black (heated dot), `false` = white.
pub fn binarize_threshold(img: &GrayImage, threshold: u8) -> Vec<bool> {
    let (width, height) = img.dimensions();
    let mut out = vec![false; (width * height) as usize];

    for y in 0..height {
        let row_offset = (y * width) as usize;
        for x in 0..width {
            let lum = img.get_pixel(x, y)[0];
            out[row_offset + x as usize] = lum < threshold;
        }
    }
    out
}

/// Binarize a grayscale image using Floyd-Steinberg error diffusion dithering.
///
/// Returns a 1D boolean array where `true` = black (heated dot), `false` = white.
pub fn binarize_floyd_steinberg(img: &GrayImage) -> Vec<bool> {
    let (width, height) = img.dimensions();
    let w = width as usize;
    let h = height as usize;
    let mut out = vec![false; w * h];

    // Maintain working float buffer for error diffusion
    let mut buffer: Vec<f32> = img.pixels().map(|p| p[0] as f32).collect();

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let old_val = buffer[idx];
            // In thermal printing: dark pixels (old_val < 128.0) are printed black (true)
            let is_black = old_val < 128.0;
            out[idx] = is_black;

            let new_val = if is_black { 0.0 } else { 255.0 };
            let err = old_val - new_val;

            if x + 1 < w {
                buffer[idx + 1] += err * (7.0 / 16.0);
            }
            if y + 1 < h {
                if x > 0 {
                    buffer[(y + 1) * w + (x - 1)] += err * (3.0 / 16.0);
                }
                buffer[(y + 1) * w + x] += err * (5.0 / 16.0);
                if x + 1 < w {
                    buffer[(y + 1) * w + (x + 1)] += err * (1.0 / 16.0);
                }
            }
        }
    }
    out
}

/// Binarize a grayscale image using Edge-Aware Smart Hybrid dithering.
///
/// Uses local 3x3 morphological contrast detection:
/// - High-contrast edges (sharp text glyphs, barcodes, vector lines, borders):
///   Directly thresholded (pure black or paper white) and suppresses error diffusion,
///   completely eliminating "mosquito noise" and stray dots around text.
/// - Low-contrast continuous tones (photographs, soft gradients, shading):
///   Diffused with Floyd-Steinberg error diffusion kernel for high photographic fidelity.
pub fn binarize_smart_hybrid(img: &GrayImage) -> Vec<bool> {
    binarize_smart_hybrid_with_params(img, 48, 160)
}

/// Binarize using Smart Hybrid algorithm with custom contrast threshold and edge luminance threshold.
pub fn binarize_smart_hybrid_with_params(
    img: &GrayImage,
    contrast_threshold: u8,
    edge_lum_threshold: u8,
) -> Vec<bool> {
    let (width, height) = img.dimensions();
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return Vec::new();
    }

    let raw = img.as_raw();
    let mut is_edge = vec![false; w * h];

    // Pass 1: Compute 3x3 local contrast
    for y in 0..h {
        let y_min = y.saturating_sub(1);
        let y_max = (y + 1).min(h - 1);
        let row_offset = y * w;

        for x in 0..w {
            let x_min = x.saturating_sub(1);
            let x_max = (x + 1).min(w - 1);

            let mut min_val = 255u8;
            let mut max_val = 0u8;

            for ny in y_min..=y_max {
                let n_row = ny * w;
                for nx in x_min..=x_max {
                    let val = raw[n_row + nx];
                    if val < min_val {
                        min_val = val;
                    }
                    if val > max_val {
                        max_val = val;
                    }
                }
            }

            if (max_val - min_val) >= contrast_threshold {
                is_edge[row_offset + x] = true;
            }
        }
    }

    // Pass 2: Edge-suppressed error diffusion
    let mut out = vec![false; w * h];
    let mut buffer: Vec<f32> = raw.iter().map(|&p| p as f32).collect();

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if is_edge[idx] {
                // High contrast edge (text, stroke, border):
                // Direct thresholding on original pixel without error dispersion
                let orig = raw[idx];
                let is_black = orig < edge_lum_threshold;
                out[idx] = is_black;
                buffer[idx] = if is_black { 0.0 } else { 255.0 };
            } else {
                // Continuous tone region: Floyd-Steinberg error diffusion
                let old_val = buffer[idx];
                let is_black = old_val < 128.0;
                out[idx] = is_black;

                let new_val = if is_black { 0.0 } else { 255.0 };
                let err = old_val - new_val;

                if x + 1 < w {
                    buffer[idx + 1] += err * (7.0 / 16.0);
                }
                if y + 1 < h {
                    if x > 0 {
                        buffer[(y + 1) * w + (x - 1)] += err * (3.0 / 16.0);
                    }
                    buffer[(y + 1) * w + x] += err * (5.0 / 16.0);
                    if x + 1 < w {
                        buffer[(y + 1) * w + (x + 1)] += err * (1.0 / 16.0);
                    }
                }
            }
        }
    }

    out
}

/// Binarize image according to chosen DitheringMethod.
pub fn binarize(img: &GrayImage, method: DitheringMethod, threshold: u8) -> Vec<bool> {
    match method {
        DitheringMethod::SmartHybrid => binarize_smart_hybrid(img),
        DitheringMethod::FloydSteinberg => binarize_floyd_steinberg(img),
        DitheringMethod::Bayer => binarize_bayer(img),
        DitheringMethod::Threshold => binarize_threshold(img, threshold),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bayer_matrix_values() {
        let mut img = GrayImage::new(4, 4);
        // Fill with constant 128
        for p in img.pixels_mut() {
            *p = image::Luma([128]);
        }
        let bayer = binarize_bayer(&img);
        assert_eq!(bayer.len(), 16);
        // Check exact values for 128:
        // threshold = (BAYER_4X4[x][y] * 16) + 8
        for y in 0..4 {
            for x in 0..4 {
                let thresh = (BAYER_4X4[x][y] as u16 * 16) + 8;
                let expected = 128 < thresh;
                assert_eq!(bayer[y * 4 + x], expected);
            }
        }
    }

    #[test]
    fn test_threshold_binarize() {
        let mut img = GrayImage::new(2, 2);
        img.put_pixel(0, 0, image::Luma([50]));
        img.put_pixel(1, 0, image::Luma([200]));
        img.put_pixel(0, 1, image::Luma([180]));
        img.put_pixel(1, 1, image::Luma([179]));

        let res = binarize_threshold(&img, 180);
        assert_eq!(res, vec![true, false, false, true]);
    }

    #[test]
    fn test_smart_hybrid_sharp_text_no_dispersion() {
        // Create an 8x8 white image with a 2x2 black text block in the center
        let mut img = GrayImage::from_pixel(8, 8, image::Luma([255]));
        img.put_pixel(3, 3, image::Luma([0]));
        img.put_pixel(4, 3, image::Luma([0]));
        img.put_pixel(3, 4, image::Luma([0]));
        img.put_pixel(4, 4, image::Luma([0]));

        let hybrid = binarize_smart_hybrid(&img);
        // Black block must be exactly black (true)
        assert!(hybrid[3 * 8 + 3]);
        assert!(hybrid[3 * 8 + 4]);
        assert!(hybrid[4 * 8 + 3]);
        assert!(hybrid[4 * 8 + 4]);

        // White paper immediately surrounding it should be clean white (false),
        // with NO error diffusion leaking into surrounding pixels!
        assert!(!hybrid[2 * 8 + 2]);
        assert!(!hybrid[2 * 8 + 3]);
        assert!(!hybrid[2 * 8 + 4]);
        assert!(!hybrid[2 * 8 + 5]);
        assert!(!hybrid[5 * 8 + 2]);
        assert!(!hybrid[5 * 8 + 3]);
        assert!(!hybrid[5 * 8 + 4]);
        assert!(!hybrid[5 * 8 + 5]);
    }

    #[test]
    fn test_smart_hybrid_smooth_gradient_dithers() {
        // 16x16 flat gray 128 (continuous tone): contrast is 0 everywhere,
        // so it should dither identically to Floyd-Steinberg.
        let img = GrayImage::from_pixel(16, 16, image::Luma([128]));
        let hybrid = binarize_smart_hybrid(&img);
        let fs = binarize_floyd_steinberg(&img);
        assert_eq!(hybrid, fs);
    }
}
