//! Core protocol crate for FZS / TeacherSaid thermal paper printers.
//! Reverse engineered from Android and iOS official printer applications.

pub mod crc16;
pub mod lzo;
pub mod packet;
pub mod profile;
pub mod qr;
pub mod status;

pub use crc16::crc16_modbus;
pub use lzo::{compress_lzo1x, decompress_lzo1x, LzoError};
pub use packet::{
    build_cancel_packet, build_config_packet, build_finish_packet, build_heating_density_packet,
    build_init_packet, build_paper_spec_packet, build_paper_steps_packet, build_paper_type_packet,
    build_query_version_packet, build_raster_slice_packet, CHUNK_RAW_SIZE, DEFAULT_DENSITY,
    DEFAULT_PAPER_STEPS, DEFAULT_WIDTH,
};
pub use profile::{HardwareProfile, PrinterSeries};
pub use qr::QrCodeInfo;
pub use status::{
    parse_printer_response, parse_version_response, PaperSpec, PaperType, PrinterStatus,
};
