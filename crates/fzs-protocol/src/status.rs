//! Printer status and paper settings.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrinterStatus {
    Success,
    OutOfPaper,
    BufferFull,
    PausedOrHalted,
    UpdateSuccess,
    UpdateFail,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PaperType {
    #[default]
    Continuous = 1,
    Folded = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PaperSpec {
    #[default]
    Spec1 = 1,
    Spec2 = 2,
}

/// Parse notification/response bytes from printer.
pub fn parse_printer_response(data: &[u8]) -> PrinterStatus {
    if data.starts_with(&[0xFA, 0x13, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFB]) {
        PrinterStatus::Success
    } else if data.starts_with(&[0xFA, 0x13, 0x02, 0x00, 0x00, 0x00, 0x00, 0xFB]) {
        PrinterStatus::OutOfPaper
    } else if data.starts_with(&[0xFA, 0x5A, 0xFB]) {
        PrinterStatus::BufferFull
    } else if data.starts_with(&[0xFA, 0x17, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFB]) {
        PrinterStatus::PausedOrHalted
    } else if data.starts_with(&[0xFA, 0x0C, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFB]) {
        PrinterStatus::UpdateSuccess
    } else if data.starts_with(&[0xFA, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFB]) {
        PrinterStatus::UpdateFail
    } else {
        PrinterStatus::Unknown
    }
}

/// Parse incoming FA 07 01 response packet from printer.
///
/// Example: `FA 07 01 04 00 31 2E 34 35 00 00 FB` -> `"1.45"`
pub fn parse_version_response(data: &[u8]) -> Option<String> {
    if data.len() >= 8 && data[0] == 0xFA && data[1] == 0x07 && data[2] == 0x01 {
        let length = u16::from_le_bytes([data[3], data[4]]) as usize;
        if data.len() >= 5 + length {
            let ver_bytes = &data[5..5 + length];
            let s = String::from_utf8_lossy(ver_bytes);
            return Some(
                s.trim_matches(|c: char| c == '\0' || c.is_whitespace())
                    .to_string(),
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_printer_response() {
        assert_eq!(
            parse_printer_response(&[0xFA, 0x13, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFB]),
            PrinterStatus::Success
        );
        assert_eq!(
            parse_printer_response(&[0xFA, 0x13, 0x02, 0x00, 0x00, 0x00, 0x00, 0xFB]),
            PrinterStatus::OutOfPaper
        );
        assert_eq!(
            parse_printer_response(&[0xFA, 0x5A, 0xFB]),
            PrinterStatus::BufferFull
        );
        assert_eq!(
            parse_printer_response(&[0xFA, 0x17, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFB]),
            PrinterStatus::PausedOrHalted
        );
    }

    #[test]
    fn test_parse_version() {
        // "1.45" = 31 2E 34 35
        let data = [
            0xFA, 0x07, 0x01, 0x04, 0x00, 0x31, 0x2E, 0x34, 0x35, 0x00, 0x00, 0xFB,
        ];
        assert_eq!(parse_version_response(&data), Some("1.45".to_string()));
    }
}
