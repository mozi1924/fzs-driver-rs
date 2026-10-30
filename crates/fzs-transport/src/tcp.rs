//! TCP socket transport for WiFi-enabled network thermal printers.

use crate::traits::{PrinterTransport, TransportError};
use fzs_protocol::status::{parse_printer_response, PrinterStatus};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::info;

pub struct TcpTransport {
    stream: TcpStream,
    host: String,
    port: u16,
    last_status: Option<PrinterStatus>,
}

impl TcpTransport {
    pub async fn connect(host: &str, port: u16) -> Result<Self, TransportError> {
        let addr = format!("{}:{}", host, port);
        info!("Connecting to network printer at {}...", addr);

        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            stream,
            host: host.into(),
            port,
            last_status: None,
        })
    }
}

impl PrinterTransport for TcpTransport {
    async fn write(
        &mut self,
        data: &[u8],
        delay_ms: u64,
        _with_response: bool,
    ) -> Result<(), TransportError> {
        self.stream
            .write_all(data)
            .await
            .map_err(|e| TransportError::WriteFailed(e.to_string()))?;
        self.stream
            .flush()
            .await
            .map_err(|e| TransportError::WriteFailed(e.to_string()))?;

        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        Ok(())
    }

    async fn wait_for_completion(
        &mut self,
        timeout: Duration,
    ) -> Result<PrinterStatus, TransportError> {
        let mut buf = [0u8; 32];
        match tokio::time::timeout(timeout, self.stream.read(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => {
                let status = parse_printer_response(&buf[..n]);
                self.last_status = Some(status);
                Ok(status)
            }
            _ => Ok(self.last_status.unwrap_or(PrinterStatus::Unknown)),
        }
    }

    async fn disconnect(&mut self) -> Result<(), TransportError> {
        let _ = self.stream.shutdown().await;
        info!("Closed TCP printer socket ({}:{}).", self.host, self.port);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }
}
