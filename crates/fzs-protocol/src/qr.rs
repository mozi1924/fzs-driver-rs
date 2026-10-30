//! QR code parser and binder decoder for printer back labels.
//!
//! Example format:
//! `C8:F0:9E:C3:10:0A&FZJJD340654&FZS-C3100A`
//! `[MAC Address]   &  [Serial SN]  &  [Model Name]`

use crate::profile::HardwareProfile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QrCodeInfo {
    pub raw: String,
    pub mac: Option<String>,
    pub serial: Option<String>,
    pub model: Option<String>,
}

impl QrCodeInfo {
    /// Parse a QR code string or device address string.
    pub fn parse(input: &str) -> Self {
        let raw = input.trim().to_string();

        if raw.contains('&') {
            let parts: Vec<&str> = raw.split('&').map(|s| s.trim()).collect();
            let mut mac = None;
            let mut serial = None;
            let mut model = None;

            if !parts.is_empty() && is_valid_mac(parts[0]) {
                mac = Some(normalize_mac(parts[0]));
            }
            if parts.len() > 1 && !parts[1].is_empty() {
                serial = Some(parts[1].to_string());
            }
            if parts.len() > 2 && !parts[2].is_empty() {
                model = Some(parts[2].to_string());
            }

            Self {
                raw,
                mac,
                serial,
                model,
            }
        } else if is_valid_mac(&raw) {
            Self {
                mac: Some(normalize_mac(&raw)),
                raw,
                serial: None,
                model: None,
            }
        } else {
            // Treat as model or SN keyword
            Self {
                raw: raw.clone(),
                mac: None,
                serial: if raw.starts_with("FZ") {
                    Some(raw.clone())
                } else {
                    None
                },
                model: Some(raw),
            }
        }
    }

    /// Infer hardware profile and recommended feed steps from parsed QR info.
    pub fn infer_profile(&self) -> HardwareProfile {
        HardwareProfile::from_device_info(self.serial.as_deref(), self.model.as_deref())
    }
}

/// Check if string looks like a standard 6-byte MAC address.
pub fn is_valid_mac(s: &str) -> bool {
    let s = s.trim();
    if s.len() == 17 {
        let chars: Vec<char> = s.chars().collect();
        let sep = chars[2];
        if (sep == ':' || sep == '-')
            && chars[5] == sep
            && chars[8] == sep
            && chars[11] == sep
            && chars[14] == sep
        {
            return s
                .split(sep)
                .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }
    false
}

/// Normalize MAC to uppercase with colons (e.g. `C8:F0:9E:C3:10:0A`).
pub fn normalize_mac(s: &str) -> String {
    s.trim().replace('-', ":").to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::PrinterSeries;

    #[test]
    fn test_parse_official_qr_code() {
        let qr = "C8:F0:9E:C3:10:0A&FZJJD340654&FZS-C3100A";
        let info = QrCodeInfo::parse(qr);

        assert_eq!(info.mac, Some("C8:F0:9E:C3:10:0A".into()));
        assert_eq!(info.serial, Some("FZJJD340654".into()));
        assert_eq!(info.model, Some("FZS-C3100A".into()));

        let profile = info.infer_profile();
        assert_eq!(profile.series, PrinterSeries::Fzjj);
        assert_eq!(profile.default_feed_steps, 284);
        assert_eq!(profile.default_physical_lines, 3292);
        assert_eq!(profile.default_logical_lines, 2380);
    }

    #[test]
    fn test_parse_fzxx_qr_code() {
        let qr = "D0:11:22:33:44:55&FZXX123456&FZXXT-1";
        let info = QrCodeInfo::parse(qr);

        assert_eq!(info.mac, Some("D0:11:22:33:44:55".into()));
        assert_eq!(info.serial, Some("FZXX123456".into()));
        assert_eq!(info.model, Some("FZXXT-1".into()));

        let profile = info.infer_profile();
        assert_eq!(profile.series, PrinterSeries::Fzxx);
        assert_eq!(profile.default_feed_steps, 162);
        assert_eq!(profile.default_physical_lines, 2100);
    }

    #[test]
    fn test_parse_bare_mac() {
        let info = QrCodeInfo::parse("c8-f0-9e-c3-10-0a");
        assert_eq!(info.mac, Some("C8:F0:9E:C3:10:0A".into()));
    }
}
