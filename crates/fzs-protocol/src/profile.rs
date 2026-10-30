//! Hardware profiles and device classification rules.

use serde::{Deserialize, Serialize};

/// Known printer hardware series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrinterSeries {
    /// FZJJ / FZS parent/student series (e.g. FZS-C3100A, FZJJ)
    Fzjj,
    /// FZXX study machine series (e.g. FZXXT, FZXXH)
    Fzxx,
    /// Generic or custom model
    Generic,
}

/// Detailed hardware profile for a thermal printer model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub model_name: String,
    pub serial: Option<String>,
    pub series: PrinterSeries,
    pub default_feed_steps: u16,
    pub print_width_dots: usize,
    pub default_physical_lines: usize,
    pub default_logical_lines: usize,
    pub motor_ratio: f64,
}

impl HardwareProfile {
    /// Create a profile for the FZS-C3100A / FZJJ series.
    pub fn fzs_c3100a() -> Self {
        Self {
            model_name: "FZS-C3100A".into(),
            serial: None,
            series: PrinterSeries::Fzjj,
            default_feed_steps: 284,
            print_width_dots: 1664,
            default_physical_lines: 3292,
            default_logical_lines: 2380,
            motor_ratio: 3292.0 / 2380.0,
        }
    }

    /// Create a profile for the FZXX series.
    pub fn fzxx(model_name: Option<&str>) -> Self {
        Self {
            model_name: model_name.unwrap_or("FZXX").into(),
            serial: None,
            series: PrinterSeries::Fzxx,
            default_feed_steps: 162,
            print_width_dots: 1664,
            default_physical_lines: 2100,
            default_logical_lines: 2100,
            motor_ratio: 1.0,
        }
    }

    /// Automatically infer profile from serial number and model name.
    pub fn from_device_info(serial: Option<&str>, model: Option<&str>) -> Self {
        let sn_upper = serial.map(|s| s.trim().to_uppercase());
        let model_upper = model.map(|m| m.trim().to_uppercase());

        let is_fzxx = sn_upper.as_deref().is_some_and(|s| s.starts_with("FZXX"))
            || model_upper
                .as_deref()
                .is_some_and(|m| m.starts_with("FZXX"));

        if is_fzxx {
            let mut p = Self::fzxx(model);
            p.serial = serial.map(|s| s.to_string());
            p
        } else {
            // Default to FZJJ / FZS series (FZS-C3100A, FZJJD..., etc.)
            let mut p = Self::fzs_c3100a();
            if let Some(m) = model {
                p.model_name = m.to_string();
            }
            p.serial = serial.map(|s| s.to_string());
            p
        }
    }
}
