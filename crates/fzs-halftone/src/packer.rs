//! Pixel bit-packing and 416-byte chunk slicing for thermal hardware.

use crate::aspect::{FitMode, DEFAULT_A4_WIDTH, MOTOR_ASPECT_RATIO};
use crate::dither::{binarize, DitheringMethod};
use image::{imageops, DynamicImage, GrayImage, Rgb, RgbImage};

pub const BYTES_PER_ROW: usize = (DEFAULT_A4_WIDTH / 8) as usize; // 208 bytes
pub const CHUNK_SIZE: usize = BYTES_PER_ROW * 2; // 416 bytes

/// Pack a 1D boolean array (true=black, false=white) of dimension `(width, height)`
/// into 208-byte scanlines, MSB-first.
pub fn pack_pixels_to_rows(pixels: &[bool], width: u32, height: u32) -> Vec<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let bytes_per_line = w.div_ceil(8);
    let mut rows = Vec::with_capacity(h);

    for y in 0..h {
        let row_start = y * w;
        let mut row_bytes = vec![0u8; bytes_per_line];

        for x in 0..w {
            if pixels[row_start + x] {
                let byte_idx = x / 8;
                let bit_idx = 7 - (x % 8);
                row_bytes[byte_idx] |= 1 << bit_idx;
            }
        }
        rows.push(row_bytes);
    }
    rows
}

/// Slice row buffers into standard 416-byte raw hardware chunks (2 scanlines per chunk).
///
/// If odd number of rows, pads the final slice with 208 bytes of zeros.
pub fn group_rows_into_chunks(rows: &[Vec<u8>], target_width: u32) -> Vec<Vec<u8>> {
    let target_row_bytes = (target_width as usize) / 8;
    let mut chunks = Vec::new();

    let mut i = 0;
    while i < rows.len() {
        let mut chunk = Vec::with_capacity(CHUNK_SIZE);

        // First row
        let r1 = &rows[i];
        if r1.len() >= target_row_bytes {
            chunk.extend_from_slice(&r1[..target_row_bytes]);
        } else {
            chunk.extend_from_slice(r1);
            chunk.resize(target_row_bytes, 0);
        }

        // Second row
        if i + 1 < rows.len() {
            let r2 = &rows[i + 1];
            if r2.len() >= target_row_bytes {
                chunk.extend_from_slice(&r2[..target_row_bytes]);
            } else {
                chunk.extend_from_slice(r2);
                chunk.resize(CHUNK_SIZE, 0);
            }
        } else {
            // Odd row count: pad second line with zeros
            chunk.resize(CHUNK_SIZE, 0);
        }

        // Final sanity check: ensure exactly 416 bytes
        chunk.resize(CHUNK_SIZE, 0);
        chunks.push(chunk);
        i += 2;
    }

    chunks
}

/// Full image preparation pipeline matching Python `prepare_image_for_printing`.
///
/// Handles RGB conversion, auto-rotation, aspect compensation, resizing (Lanczos3),
/// dithering, and packing into 416-byte chunks.
#[allow(clippy::too_many_arguments)]
pub fn prepare_image(
    img: &DynamicImage,
    target_width: u32,
    fit: FitMode,
    page_height: u32,
    logical_height: u32,
    compensate_aspect: bool,
    dither: DitheringMethod,
    threshold: u8,
    auto_rotate: bool,
    invert: bool,
) -> (Vec<Vec<u8>>, u32, u32) {
    let mut current = img.to_rgb8();

    // Auto-rotate landscape images if requested
    if auto_rotate && current.width() > current.height() {
        current = imageops::rotate270(&current);
    }

    let ratio = if compensate_aspect {
        MOTOR_ASPECT_RATIO
    } else {
        1.0
    };

    let processed_gray: GrayImage = match fit {
        FitMode::Page => {
            let l_height = if logical_height.is_multiple_of(2) {
                logical_height
            } else {
                logical_height + 1
            };

            let scale = (target_width as f64 / current.width() as f64)
                .min(l_height as f64 / current.height() as f64);
            let new_w = (current.width() as f64 * scale).round().max(1.0) as u32;
            let new_h = (current.height() as f64 * scale).round().max(1.0) as u32;

            let resized = imageops::resize(&current, new_w, new_h, imageops::FilterType::Lanczos3);

            // Create white canvas (1664 x 2380)
            let mut canvas = RgbImage::from_pixel(target_width, l_height, Rgb([255, 255, 255]));
            let offset_x = (target_width - new_w) / 2;
            let offset_y = (l_height - new_h) / 2;
            imageops::overlay(&mut canvas, &resized, offset_x as i64, offset_y as i64);

            // Stretch vertically if compensate_aspect
            let final_rgb = if compensate_aspect {
                let p_height = if page_height.is_multiple_of(2) {
                    page_height
                } else {
                    page_height + 1
                };
                imageops::resize(
                    &canvas,
                    target_width,
                    p_height,
                    imageops::FilterType::Lanczos3,
                )
            } else {
                canvas
            };

            DynamicImage::ImageRgb8(final_rgb).into_luma8()
        }
        FitMode::Width => {
            let scale = target_width as f64 / current.width() as f64;
            let mut target_h = (current.height() as f64 * scale * ratio).round() as u32;
            if !target_h.is_multiple_of(2) {
                target_h += 1;
            }
            let resized = imageops::resize(
                &current,
                target_width,
                target_h,
                imageops::FilterType::Lanczos3,
            );
            DynamicImage::ImageRgb8(resized).into_luma8()
        }
        FitMode::Original => {
            let (new_w, new_h) = if current.width() > target_width {
                let scale = target_width as f64 / current.width() as f64;
                let h = (current.height() as f64 * scale * ratio).round() as u32;
                (target_width, h)
            } else {
                let h = (current.height() as f64 * ratio).round() as u32;
                (current.width(), h)
            };

            let target_h = if new_h.is_multiple_of(2) {
                new_h
            } else {
                new_h + 1
            };
            let resized =
                imageops::resize(&current, new_w, target_h, imageops::FilterType::Lanczos3);

            let final_rgb = if new_w < target_width {
                let mut canvas = RgbImage::from_pixel(target_width, target_h, Rgb([255, 255, 255]));
                let offset_x = (target_width - new_w) / 2;
                imageops::overlay(&mut canvas, &resized, offset_x as i64, 0);
                canvas
            } else {
                resized
            };

            DynamicImage::ImageRgb8(final_rgb).into_luma8()
        }
    };

    let (final_w, final_h) = processed_gray.dimensions();
    let mut gray_img = processed_gray;

    if invert {
        imageops::invert(&mut gray_img);
    }

    // Binarize
    let bit_pixels = binarize(&gray_img, dither, threshold);

    // Pack into rows
    let rows = pack_pixels_to_rows(&bit_pixels, final_w, final_h);

    // Group into 416-byte chunks
    let chunks = group_rows_into_chunks(&rows, target_width);

    (chunks, final_w, final_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_pixels() {
        // 8 pixels: black, white, black, white, black, black, white, black
        // 1 0 1 0 1 1 0 1 = 0xAD
        let pixels = vec![true, false, true, false, true, true, false, true];
        let rows = pack_pixels_to_rows(&pixels, 8, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec![0xAD]);
    }

    #[test]
    fn test_chunk_grouping() {
        let row = vec![0xFFu8; 208];
        let rows = vec![row.clone(), row.clone(), row.clone()]; // 3 rows -> 2 chunks
        let chunks = group_rows_into_chunks(&rows, 1664);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 416);
        assert_eq!(chunks[1].len(), 416);
        // Chunk 0 has 2 rows of 0xFF
        assert_eq!(&chunks[0][..208], &row[..]);
        assert_eq!(&chunks[0][208..], &row[..]);
        // Chunk 1 has 1 row of 0xFF, padded with 208 zeros
        assert_eq!(&chunks[1][..208], &row[..]);
        assert_eq!(&chunks[1][208..], &[0u8; 208][..]);
    }
}
