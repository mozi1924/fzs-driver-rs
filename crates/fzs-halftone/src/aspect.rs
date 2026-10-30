//! Motor step geometry and page aspect ratio compensation.

use serde::{Deserialize, Serialize};

/// Physical motor stepping ratio (3292 physical lines / 2380 logical lines).
///
/// Matches official FZS Android app `BitmapUtil.java`:
/// `fFloatValue = pheight / height = 3292.0 / 2380.0 = 1.383193277`
pub const MOTOR_ASPECT_RATIO: f64 = 3292.0 / 2380.0;

/// Standard thermal print width in dots (208mm @ 203.2 DPI).
pub const DEFAULT_A4_WIDTH: u32 = 1664;

/// Standard physical scan lines for full A4 length (297mm @ 281.5 DPI).
pub const DEFAULT_A4_PHYSICAL_HEIGHT: u32 = 3292;

/// Standard logical square-pixel height before motor compensation (297mm @ 203.2 DPI).
pub const DEFAULT_A4_LOGICAL_HEIGHT: u32 = 2380;

/// Image fitting modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FitMode {
    /// Scale proportionally to fit standard A4 canvas (1664x3292), centered with white margins.
    #[default]
    Page,
    /// Scale to fill full 1664-dot width while keeping aspect ratio (for continuous roll).
    Width,
    /// 1:1 pixel mapping with motor compensation, centered on 1664-dot canvas.
    Original,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motor_aspect_ratio_constant() {
        assert!((MOTOR_ASPECT_RATIO - 1.3831932773109244).abs() < 1e-12);
    }
}
