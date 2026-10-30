//! Diagnostic test pattern and printer calibration chart generator.

use image::{DynamicImage, Rgb, RgbImage};

/// Generate a comprehensive diagnostic test chart.
pub fn generate_test_pattern(width: u32, height: u32) -> DynamicImage {
    let mut img = RgbImage::from_pixel(width, height, Rgb([255, 255, 255]));

    let black = Rgb([0, 0, 0]);
    let gray_line = Rgb([180, 180, 180]);

    // 1. Outer border (4px)
    let margin = 10;
    for t in 0..4 {
        for x in margin..(width - margin) {
            img.put_pixel(x, margin + t, black);
            img.put_pixel(x, height - margin - 1 - t, black);
        }
        for y in margin..(height - margin) {
            img.put_pixel(margin + t, y, black);
            img.put_pixel(width - margin - 1 - t, y, black);
        }
    }

    // 2. Top separator line
    for x in 30..(width - 30) {
        for t in 0..2 {
            img.put_pixel(x, 120 + t, black);
        }
    }

    // 3. Line resolution test bars (1px, 2px, 3px, 4px, 6px, 8px)
    let mut cur_y = 170;
    for &lw in &[1, 2, 3, 4, 6, 8] {
        for y in cur_y..(cur_y + lw) {
            if y < height {
                for x in 40..(width - 40) {
                    img.put_pixel(x, y, black);
                }
            }
        }
        cur_y += lw + 14;
    }

    // 4. Grayscale halftone gradient ramp (0% - 100%)
    cur_y += 30;
    let ramp_w = width.saturating_sub(80);
    let ramp_h = 60;
    if ramp_w > 0 && cur_y + ramp_h < height {
        for rx in 0..ramp_w {
            let val = (255.0 * (1.0 - (rx as f64 / ramp_w as f64))).round() as u8;
            let col = Rgb([val, val, val]);
            for ry in 0..ramp_h {
                img.put_pixel(40 + rx, cur_y + ry, col);
            }
        }
        cur_y += ramp_h + 30;
    }

    // 5. Alignment grid test (40px step)
    let grid_size = 40;
    let grid_h = 160;
    if cur_y + grid_h < height {
        // Vertical grid lines
        let mut gx = 40;
        while gx < width - 40 {
            for gy in cur_y..(cur_y + grid_h) {
                img.put_pixel(gx, gy, gray_line);
            }
            gx += grid_size;
        }
        // Horizontal grid lines
        let mut gy = cur_y;
        while gy <= cur_y + grid_h {
            for x in 40..(width - 40) {
                img.put_pixel(x, gy, gray_line);
            }
            gy += grid_size;
        }
    }

    DynamicImage::ImageRgb8(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_pattern() {
        let pattern = generate_test_pattern(1664, 400);
        assert_eq!(pattern.width(), 1664);
        assert_eq!(pattern.height(), 400);
    }
}
