//! Abstract transport traits for printer communication.

use fzs_protocol::status::PrinterStatus;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Transmission failed: {0}")]
    WriteFailed(String),
    #[error("Timeout waiting for printer response")]
    Timeout,
    #[error("Device disconnected")]
    Disconnected,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Bluetooth error: {0}")]
    Bluetooth(String),
}

/// Abstract asynchronous printer transport.
#[allow(async_fn_in_trait)]
pub trait PrinterTransport: Send + Sync {
    /// Transmit raw byte payload with optional ACK response and delay.
    async fn write(
        &mut self,
        data: &[u8],
        delay_ms: u64,
        with_response: bool,
    ) -> Result<(), TransportError>;

    /// Wait for printer to complete job and return final status.
    async fn wait_for_completion(
        &mut self,
        timeout: Duration,
    ) -> Result<PrinterStatus, TransportError>;

    /// Disconnect from the printer.
    async fn disconnect(&mut self) -> Result<(), TransportError>;

    /// Check if connection is currently active.
    fn is_connected(&self) -> bool;
}
