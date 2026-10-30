//! Halftoning and binarization algorithms (Floyd-Steinberg, Bayer 4x4, Threshold).

use image::GrayImage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DitheringMethod {
    #[default]
    FloydSteinberg,
    Bayer,
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

/// Binarize image according to chosen DitheringMethod.
pub fn binarize(img: &GrayImage, method: DitheringMethod, threshold: u8) -> Vec<bool> {
    match method {
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
}
