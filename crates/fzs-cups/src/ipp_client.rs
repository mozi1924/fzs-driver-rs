//! TCP-based IPP (Internet Printing Protocol) client for communicating with CUPS on port 631.

use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::debug;

#[derive(Error, Debug)]
pub enum IppError {
    #[error("Failed to connect to CUPS server at {0}: {1}")]
    ConnectError(String, String),
    #[error("IPP IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CUPS responded with HTTP error: {0}")]
    HttpError(String),
}

/// Lightweight TCP client to interact with local CUPS scheduler over IPP.
pub struct IppClient {
    pub host: String,
    pub port: u16,
}

impl Default for IppClient {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 631,
        }
    }
}

impl IppClient {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// Check if CUPS scheduler is active and listening on TCP.
    pub async fn is_cups_online(&self) -> bool {
        let addr = format!("{}:{}", self.host, self.port);
        matches!(
            tokio::time::timeout(Duration::from_millis(500), TcpStream::connect(&addr)).await,
            Ok(Ok(_))
        )
    }

    /// Send a raw IPP `Get-Printer-Attributes` query over TCP and return true if printer exists.
    pub async fn printer_exists(&self, printer_name: &str) -> Result<bool, IppError> {
        let addr = format!("{}:{}", self.host, self.port);
        let mut stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| IppError::ConnectError(addr.clone(), e.to_string()))?;

        let printer_uri = format!(
            "ipp://{}:{}/printers/{}",
            self.host, self.port, printer_name
        );

        // Build binary IPP request: Get-Printer-Attributes (0x000B in CUPS or 0x0004 in IPP)
        let mut ipp_req = Vec::new();
        // Version 2.0: 0x02, 0x00
        ipp_req.extend_from_slice(&[0x02, 0x00]);
        // Operation: Get-Printer-Attributes (0x000B in CUPS)
        ipp_req.extend_from_slice(&[0x00, 0x0B]);
        // Request ID: 1
        ipp_req.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);

        // Operation attributes tag: 0x01
        ipp_req.push(0x01);

        // 1. attributes-charset = "utf-8"
        write_ipp_attr(&mut ipp_req, 0x47, "attributes-charset", "utf-8");
        // 2. attributes-natural-language = "en"
        write_ipp_attr(&mut ipp_req, 0x48, "attributes-natural-language", "en");
        // 3. printer-uri
        write_ipp_attr(&mut ipp_req, 0x45, "printer-uri", &printer_uri);

        // End of attributes: 0x03
        ipp_req.push(0x03);

        // Wrap in HTTP/1.1 POST
        let http_head = format!(
            "POST /printers/{} HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             User-Agent: fzs-cups-rust/1.0\r\n\
             Content-Type: application/ipp\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n",
            printer_name,
            self.host,
            self.port,
            ipp_req.len()
        );

        stream.write_all(http_head.as_bytes()).await?;
        stream.write_all(&ipp_req).await?;
        stream.flush().await?;

        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).await?;

        let resp_str = String::from_utf8_lossy(&resp);
        if resp_str.contains("200 OK") {
            Ok(true)
        } else if resp_str.contains("404 Not Found") {
            Ok(false)
        } else {
            debug!("CUPS response: {}", resp_str);
            Ok(resp_str.contains("successful-ok"))
        }
    }
}

fn write_ipp_attr(buf: &mut Vec<u8>, tag: u8, name: &str, value: &str) {
    buf.push(tag);
    buf.extend_from_slice(&(name.len() as u16).to_be_bytes());
    buf.extend_from_slice(name.as_bytes());
    buf.extend_from_slice(&(value.len() as u16).to_be_bytes());
    buf.extend_from_slice(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cups_online_check() {
        let client = IppClient::default();
        let _ = client.is_cups_online().await;
    }
}
