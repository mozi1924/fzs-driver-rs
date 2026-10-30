//! Pure Rust decoder for CUPS Raster and PWG Raster streams.
//!
//! Fully self-contained: zero runtime dependency on `libcups.dylib`.
//! Supports:
//! - CUPS Raster v1/v2/v3 (`RaSt`, `3SaR`)
//! - PWG Raster (`RaS2`, `2SaR`)
//! - 8-bit Grayscale (`cupsColorSpace 0`, `cupsBitsPerPixel 8`)
//! - 1-bit Monochrome (`cupsBitsPerPixel 1`)
//! - PackBits RLE line decompression (PWG standard)

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use fzs_halftone::{
    binarize_floyd_steinberg, group_rows_into_chunks, pack_pixels_to_rows, BYTES_PER_ROW,
    DEFAULT_A4_WIDTH,
};
use image::GrayImage;
use std::io::{Cursor, Read};
use thiserror::Error;
use tracing::{debug, info};

#[derive(Error, Debug)]
pub enum RasterError {
    #[error("Invalid raster magic bytes: {0:?}")]
    InvalidMagic([u8; 4]),
    #[error("Unexpected EOF while reading raster stream")]
    UnexpectedEof,
    #[error("Raster decode error: {0}")]
    DecodeError(String),
}

/// Parsed page header from CUPS / PWG raster stream.
#[derive(Debug, Clone)]
pub struct RasterPageHeader {
    pub width: u32,
    pub height: u32,
    pub bits_per_color: u32,
    pub bits_per_pixel: u32,
    pub bytes_per_line: u32,
    pub color_space: u32,
    pub is_big_endian: bool,
    pub is_pwg: bool,
}

impl RasterPageHeader {
    /// Read header (1796 bytes) from reader.
    pub fn read_from<R: Read>(
        reader: &mut R,
        is_pwg: bool,
        is_big_endian: bool,
    ) -> Result<Option<Self>, RasterError> {
        let mut buf = [0u8; 1796];
        match reader.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(RasterError::DecodeError(e.to_string())),
        }

        // Auto-detect endianness by checking cupsWidth (offset 372..376)
        // If interpreted as Little-Endian, valid width is < 100,000 (e.g. 1664 or 1678).
        // If it's big-endian, Little-Endian gives > 100,000.
        let raw_le_width = LittleEndian::read_u32(&buf[372..376]);
        let use_big_endian = raw_le_width > 100_000;

        let (width, height, bits_per_color, bits_per_pixel, bytes_per_line, color_space) =
            if use_big_endian {
                (
                    BigEndian::read_u32(&buf[372..376]),
                    BigEndian::read_u32(&buf[376..380]),
                    BigEndian::read_u32(&buf[384..388]),
                    BigEndian::read_u32(&buf[388..392]),
                    BigEndian::read_u32(&buf[392..396]),
                    BigEndian::read_u32(&buf[400..404]),
                )
            } else {
                (
                    raw_le_width,
                    LittleEndian::read_u32(&buf[376..380]),
                    LittleEndian::read_u32(&buf[384..388]),
                    LittleEndian::read_u32(&buf[388..392]),
                    LittleEndian::read_u32(&buf[392..396]),
                    LittleEndian::read_u32(&buf[400..404]),
                )
            };

        if width == 0 || height == 0 || bytes_per_line == 0 {
            return Ok(None);
        }

        Ok(Some(Self {
            width,
            height,
            bits_per_color,
            bits_per_pixel,
            bytes_per_line,
            color_space,
            is_big_endian,
            is_pwg,
        }))
    }
}

/// Decode a PackBits compressed line in PWG raster.
fn decode_packbits_line(
    reader: &mut impl Read,
    expected_len: usize,
) -> Result<Vec<u8>, RasterError> {
    let mut out = Vec::with_capacity(expected_len);
    let mut buf = [0u8; 1];

    while out.len() < expected_len {
        if reader.read_exact(&mut buf).is_err() {
            break;
        }
        let count = buf[0] as i8;
        if count >= 0 {
            // Literal run: copy (count + 1) bytes
            let num = (count as usize) + 1;
            let mut lit = vec![0u8; num];
            reader
                .read_exact(&mut lit)
                .map_err(|_| RasterError::UnexpectedEof)?;
            out.extend_from_slice(&lit);
        } else if count != -128 {
            // Repeat run: repeat next byte (-count + 1) times
            let num = ((-count) as usize) + 1;
            let mut val = [0u8; 1];
            reader
                .read_exact(&mut val)
                .map_err(|_| RasterError::UnexpectedEof)?;
            out.resize(out.len() + num, val[0]);
        }
    }

    if out.len() < expected_len {
        out.resize(expected_len, 0);
    } else if out.len() > expected_len {
        out.truncate(expected_len);
    }
    Ok(out)
}

/// Pure Rust decoder for CUPS / PWG raster streams into pages of 416-byte raw chunks.
pub fn parse_raster_stream(data: &[u8]) -> Result<Vec<Vec<Vec<u8>>>, RasterError> {
    if data.len() < 4 {
        return Err(RasterError::UnexpectedEof);
    }

    let magic = &data[0..4];
    let (is_pwg, is_big_endian) = match magic {
        b"RaS2" => (true, true),   // Standard PWG Raster (big-endian)
        b"2SaR" => (true, false),  // PWG Raster (little-endian)
        b"RaSt" => (false, false), // CUPS Raster (little-endian)
        b"3SaR" => (false, true),  // CUPS Raster (big-endian)
        b"RaS3" => (false, false), // CUPS Raster v3
        _ => {
            return Err(RasterError::InvalidMagic([
                magic[0], magic[1], magic[2], magic[3],
            ]))
        }
    };

    info!(
        "Raster stream identified: magic={:?}, is_pwg={}, big_endian={}",
        String::from_utf8_lossy(magic),
        is_pwg,
        is_big_endian
    );

    let mut cursor = Cursor::new(&data[4..]);
    let mut pages = Vec::new();

    while let Some(header) = RasterPageHeader::read_from(&mut cursor, is_pwg, is_big_endian)? {
        debug!(
            "Raster page decoded: {}x{} dots, bpp={}, bpl={}, cspace={}",
            header.width,
            header.height,
            header.bits_per_pixel,
            header.bytes_per_line,
            header.color_space
        );

        let mut raw_rows: Vec<Vec<u8>> = Vec::with_capacity(header.height as usize);

        if header.is_pwg {
            // PWG Raster: scanlines have a 1-byte repeat count prefix + PackBits encoding
            let mut y = 0;
            while y < header.height {
                let mut rep_buf = [0u8; 1];
                if cursor.read_exact(&mut rep_buf).is_err() {
                    break;
                }
                let repeat_count = rep_buf[0] as usize; // repeat N times after first line
                let line_data = decode_packbits_line(&mut cursor, header.bytes_per_line as usize)?;

                for _ in 0..=repeat_count {
                    if y < header.height {
                        raw_rows.push(line_data.clone());
                        y += 1;
                    }
                }
            }
        } else {
            // Uncompressed CUPS Raster
            for _ in 0..header.height {
                let mut line_buf = vec![0u8; header.bytes_per_line as usize];
                if cursor.read_exact(&mut line_buf).is_err() {
                    break;
                }
                raw_rows.push(line_buf);
            }
        }

        // Convert page rows into normalized 1664-dot (208 bytes) bit rows
        let normalized_rows = if header.bits_per_pixel == 8 {
            // 8-bit Grayscale: run Floyd-Steinberg dithering
            let mut gray_img = GrayImage::new(header.width, header.height);
            for (y, row) in raw_rows.iter().enumerate() {
                for (x, &lum) in row.iter().take(header.width as usize).enumerate() {
                    // In CUPS color_space 0 (Gray): 0 is black, 255 is white
                    gray_img.put_pixel(x as u32, y as u32, image::Luma([lum]));
                }
            }
            let bits = binarize_floyd_steinberg(&gray_img);
            pack_pixels_to_rows(&bits, header.width, header.height)
        } else {
            // 1-bit Monochrome: normalize row length to 208 bytes
            let mut norm = Vec::with_capacity(raw_rows.len());
            for row in raw_rows {
                let mut n_row = vec![0u8; BYTES_PER_ROW];
                let copy_len = row.len().min(BYTES_PER_ROW);
                n_row[..copy_len].copy_from_slice(&row[..copy_len]);
                norm.push(n_row);
            }
            norm
        };

        // If raster was rendered at square 203x203 DPI (height < 2600),
        // stretch to ~3292 lines to compensate for physical 281.5 DPI motor
        let mut final_rows = normalized_rows;
        if header.height < 2600 && !final_rows.is_empty() {
            let target_lines = ((header.height as f64) * (3292.0 / 2380.0)).round() as usize;
            info!(
                "Compensating 203x203 DPI raster for physical motor: {} -> {} lines",
                header.height, target_lines
            );
            let mut stretched = Vec::with_capacity(target_lines);
            for ty in 0..target_lines {
                let src_y = (ty * (header.height as usize)) / target_lines;
                let clamp_y = src_y.min(final_rows.len() - 1);
                stretched.push(final_rows[clamp_y].clone());
            }
            final_rows = stretched;
        }

        // Group rows into 416-byte raw chunks
        let chunks = group_rows_into_chunks(&final_rows, DEFAULT_A4_WIDTH);
        info!(
            "Page assembled: {} slices ({} bytes)",
            chunks.len(),
            chunks.len() * 416
        );
        pages.push(chunks);
    }

    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_magic() {
        let res = parse_raster_stream(b"BAD!");
        assert!(res.is_err());
    }
}
