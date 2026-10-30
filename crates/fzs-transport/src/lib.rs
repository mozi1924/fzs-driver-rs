//! Cross-platform transports (BLE, Serial, TCP) and configuration persistence.

pub mod ble;
pub mod config;
pub mod serial;
pub mod tcp;
pub mod traits;

pub use ble::{BleTransport, DiscoveredPrinter};
pub use config::{
    bind_printer, get_config_path, load_config, save_config, unbind_printer, AppConfig,
    ConfigError, DaemonConfig, PrinterBindingConfig,
};
pub use serial::SerialTransport;
pub use tcp::TcpTransport;
pub use traits::{PrinterTransport, TransportError};
