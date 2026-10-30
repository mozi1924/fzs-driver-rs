//! Self-contained LZO1X-1 lossless compression and decompression.
//!
//! Uses statically linked MiniLZO to guarantee zero runtime system C library dependencies.

use minilzo_rs::LZO;
use std::cell::RefCell;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LzoError {
    #[error("LZO initialization failed: {0}")]
    InitFailed(String),
    #[error("LZO compression failed: {0}")]
    CompressFailed(String),
    #[error("LZO decompression failed: {0}")]
    DecompressFailed(String),
}

thread_local! {
    static THREAD_LZO: RefCell<Option<LZO>> = RefCell::new(LZO::init().ok());
}

/// Compress input bytes using the LZO1X-1 algorithm.
pub fn compress_lzo1x(data: &[u8]) -> Result<Vec<u8>, LzoError> {
    THREAD_LZO.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let lzo = borrow.as_mut().ok_or_else(|| {
            LzoError::InitFailed("Thread-local LZO engine failed to initialize".into())
        })?;
        lzo.compress(data)
            .map_err(|e| LzoError::CompressFailed(format!("{:?}", e)))
    })
}

/// Decompress LZO1X data with the expected uncompressed output size.
pub fn decompress_lzo1x(data: &[u8], expected_len: usize) -> Result<Vec<u8>, LzoError> {
    THREAD_LZO.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let lzo = borrow.as_mut().ok_or_else(|| {
            LzoError::InitFailed("Thread-local LZO engine failed to initialize".into())
        })?;
        lzo.decompress_safe(data, expected_len)
            .map_err(|e| LzoError::DecompressFailed(format!("{:?}", e)))
    })
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
    fn test_zeros_parity_with_python() {
        let zeros = vec![0u8; 416];
        let compressed = compress_lzo1x(&zeros).expect("compress");
        let hex_out = compressed
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        // Python reference: 02000000000020006c10000c000000000000000000000000000000110000
        assert_eq!(
            hex_out,
            "02000000000020006c10000c000000000000000000000000000000110000"
        );
    }
}
