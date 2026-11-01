//! Self-contained 100% pure-Rust LZO1X lossless compression and decompression.
//!
//! Powered by `lzokay`, providing `#![no_std]` capable, safe pure-Rust LZO1X
//! codec without any C dependencies or build tools. Works universally on native
//! and WebAssembly (`wasm32-unknown-unknown`) platforms.

use lzokay::compress::{compress_with_dict, Dict};
use lzokay::decompress::decompress;
use std::cell::RefCell;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LzoError {
    #[error("LZO compression failed: {0}")]
    CompressFailed(String),
    #[error("LZO decompression failed: {0}")]
    DecompressFailed(String),
    #[error("Decompressed size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: usize, actual: usize },
}

thread_local! {
    static THREAD_DICT: RefCell<Box<Dict>> = RefCell::new(Dict::new());
}

/// Compress input bytes using the LZO1X-1 algorithm in pure Rust.
pub fn compress_lzo1x(data: &[u8]) -> Result<Vec<u8>, LzoError> {
    THREAD_DICT.with(|cell| {
        let mut dict = cell.borrow_mut();
        compress_with_dict(data, &mut dict)
            .map_err(|e| LzoError::CompressFailed(e.to_string()))
    })
}

/// Decompress LZO1X data with the expected uncompressed output size.
pub fn decompress_lzo1x(data: &[u8], expected_len: usize) -> Result<Vec<u8>, LzoError> {
    let mut out = vec![0u8; expected_len];
    let actual = decompress(data, &mut out)
        .map_err(|e| LzoError::DecompressFailed(e.to_string()))?;
    if actual != expected_len {
        return Err(LzoError::SizeMismatch {
            expected: expected_len,
            actual,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_and_decompress() {
        let original = vec![0xABu8; 416];
        let compressed = compress_lzo1x(&original).expect("compress");
        assert!(!compressed.is_empty());
        let decompressed = decompress_lzo1x(&compressed, original.len()).expect("decompress");
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_zeros_compression() {
        let zeros = vec![0u8; 416];
        let compressed = compress_lzo1x(&zeros).expect("compress");
        let hex_out = compressed
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        // Pure-Rust LZO1X optimal encoding for 416 zeros
        assert_eq!(hex_out, "120020007f0000110000");

        let decompressed = decompress_lzo1x(&compressed, zeros.len()).expect("decompress");
        assert_eq!(decompressed, zeros);
    }

    #[test]
    fn test_decompress_python_minilzo_reference() {
        // Python reference C-miniLZO output for 416 zeros
        let py_ref_bytes = [
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x6c, 0x10, 0x00, 0x0c, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11,
            0x00, 0x00,
        ];
        let decompressed = decompress_lzo1x(&py_ref_bytes, 416).expect("decompress python ref");
        assert_eq!(decompressed, vec![0u8; 416]);
    }
}
