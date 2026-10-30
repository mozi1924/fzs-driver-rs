//! Image halftoning, aspect ratio compensation, and bit packaging for thermal printing.

pub mod aspect;
pub mod dither;
pub mod packer;
pub mod test_pattern;

pub use aspect::{
    FitMode, DEFAULT_A4_LOGICAL_HEIGHT, DEFAULT_A4_PHYSICAL_HEIGHT, DEFAULT_A4_WIDTH,
    MOTOR_ASPECT_RATIO,
};
pub use dither::{
    binarize, binarize_bayer, binarize_floyd_steinberg, binarize_threshold, DitheringMethod,
    BAYER_4X4,
};
pub use packer::{
    group_rows_into_chunks, pack_pixels_to_rows, prepare_image, BYTES_PER_ROW, CHUNK_SIZE,
};
pub use test_pattern::generate_test_pattern;
