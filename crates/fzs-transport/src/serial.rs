//! Bluetooth Classic SPP and USB serial port transport.

use crate::traits::{PrinterTransport, TransportError};
use fzs_protocol::status::{parse_printer_response, PrinterStatus};
use serialport::SerialPort;
use std::io::{Read, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::info;

pub struct SerialTransport {
    port: Mutex<Box<dyn SerialPort>>,
    port_name: String,
    last_status: Option<PrinterStatus>,
}

impl SerialTransport {
    /// List available serial / SPP device paths.
    pub fn list_ports() -> Vec<String> {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.port_name)
            .collect()
    }

    /// Auto-detect candidate printer serial port.
    pub fn find_candidate_port() -> Option<String> {
        for p in serialport::available_ports().unwrap_or_default() {
            let name = p.port_name.to_lowercase();
            if name.contains("fz")
                || name.contains("printer")
                || name.contains("rfcomm")
                || name.contains("teacher")
            {
                return Some(p.port_name);
            }
        }
        None
    }

    /// Connect to a serial port at 115200 baud.
    pub fn connect(port_name: Option<&str>, baudrate: u32) -> Result<Self, TransportError> {
        let target = port_name
            .map(|s| s.to_string())
            .or_else(Self::find_candidate_port)
            .ok_or_else(|| {
                TransportError::DeviceNotFound("No candidate serial port found".into())
            })?;

        info!("Opening serial port {} at {} baud...", target, baudrate);

        let port = serialport::new(&target, baudrate)
            .timeout(Duration::from_millis(1000))
            .open()
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            port: Mutex::new(port),
            port_name: target,
            last_status: None,
        })
    }
}

impl PrinterTransport for SerialTransport {
    async fn write(
        &mut self,
        data: &[u8],
        delay_ms: u64,
        _with_response: bool,
    ) -> Result<(), TransportError> {
        {
            let mut port = self
                .port
                .lock()
                .map_err(|_| TransportError::WriteFailed("Serial port mutex poisoned".into()))?;

            port.write_all(data)
                .map_err(|e| TransportError::WriteFailed(e.to_string()))?;
            port.flush()
                .map_err(|e| TransportError::WriteFailed(e.to_string()))?;

            // Read any immediate feedback bytes
            let mut buf = [0u8; 32];
            if let Ok(n) = port.read(&mut buf) {
                if n > 0 {
                    let status = parse_printer_response(&buf[..n]);
                    if status != PrinterStatus::Unknown {
                        self.last_status = Some(status);
                    }
                }
            }
        }

        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        Ok(())
    }

    async fn wait_for_completion(
        &mut self,
        timeout: Duration,
    ) -> Result<PrinterStatus, TransportError> {
        let start = Instant::now();
        let mut buf = [0u8; 64];

        while start.elapsed() < timeout {
            if let Ok(mut port) = self.port.lock() {
                if let Ok(n) = port.read(&mut buf) {
                    if n > 0 {
                        let status = parse_printer_response(&buf[..n]);
                        if status != PrinterStatus::Unknown {
                            self.last_status = Some(status);
                            return Ok(status);
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Ok(self.last_status.unwrap_or(PrinterStatus::Unknown))
    }

    async fn disconnect(&mut self) -> Result<(), TransportError> {
        info!("Closed serial port {}.", self.port_name);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }
}
