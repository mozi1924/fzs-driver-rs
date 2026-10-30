//! Pure Rust CUPS/PWG raster decoder, PPD generation, and CUPS management.

pub mod ipp_client;
pub mod manager;
pub mod ppd;
pub mod raster;

pub use ipp_client::IppClient;
pub use manager::{
    get_printer_status, install_cups_printer, uninstall_cups_printer, PrinterQueueStatus,
    DEFAULT_PORT, DEFAULT_PRINTER_NAME,
};
#[cfg(target_os = "macos")]
pub use manager::{install_launch_agent, uninstall_launch_agent};
pub use ppd::{ensure_ppd_file, DEFAULT_PPD_CONTENT};
pub use raster::{
    parse_raster_pages, parse_raster_stream, RasterError, RasterPage, RasterPageHeader,
};
