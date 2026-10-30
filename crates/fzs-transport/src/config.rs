//! Persistent printer configuration and binding manager.
//!
//! Stores device binding information, macOS CoreBluetooth UUID, model profile,
//! and user preferences in `~/.config/fzs-printer/config.toml`.

use fzs_protocol::qr::QrCodeInfo;
use fzs_protocol::status::{PaperSpec, PaperType};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Could not determine user config directory")]
    HomeNotFound,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML serialization/deserialization error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterBindingConfig {
    pub mac: Option<String>,
    pub serial: Option<String>,
    pub model: String,
    pub macos_uuid: Option<String>,
    pub feed_steps: u16,
    pub density: u16,
    pub paper_type: PaperType,
    pub paper_spec: PaperSpec,
}

impl Default for PrinterBindingConfig {
    fn default() -> Self {
        Self {
            mac: None,
            serial: None,
            model: "FZS-C3100A".into(),
            macos_uuid: None,
            feed_steps: 284,
            density: 180,
            paper_type: PaperType::Continuous,
            paper_spec: PaperSpec::Spec1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub host: String,
    pub port: u16,
    pub auto_reconnect: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 9100,
            auto_reconnect: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub printer: PrinterBindingConfig,
    pub daemon: DaemonConfig,
}

/// Get path to `~/.config/fzs-printer/config.toml`.
pub fn get_config_path() -> Result<PathBuf, ConfigError> {
    let base = dirs::config_dir().ok_or(ConfigError::HomeNotFound)?;
    Ok(base.join("fzs-printer").join("config.toml"))
}

/// Load configuration from disk, or return default if file doesn't exist.
pub fn load_config() -> Result<AppConfig, ConfigError> {
    let path = get_config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let content = fs::read_to_string(&path)?;
    let cfg: AppConfig = toml::from_str(&content)?;
    Ok(cfg)
}

/// Save configuration to disk.
pub fn save_config(cfg: &AppConfig) -> Result<PathBuf, ConfigError> {
    let path = get_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(cfg)?;
    fs::write(&path, s)?;
    info!("Configuration saved to {:?}", path);
    Ok(path)
}

/// Bind a printer from a QR code string or address.
pub fn bind_printer(qr_or_str: &str, macos_uuid: Option<&str>) -> Result<AppConfig, ConfigError> {
    let qr = QrCodeInfo::parse(qr_or_str);
    let profile = qr.infer_profile();

    let mut cfg = load_config().unwrap_or_default();
    cfg.printer.mac = qr.mac;
    cfg.printer.serial = qr.serial;
    cfg.printer.model = profile.model_name;
    cfg.printer.feed_steps = profile.default_feed_steps;

    if let Some(uuid) = macos_uuid {
        cfg.printer.macos_uuid = Some(uuid.to_string());
    }

    save_config(&cfg)?;
    Ok(cfg)
}

/// Unbind any currently bound printer.
pub fn unbind_printer() -> Result<AppConfig, ConfigError> {
    let mut cfg = load_config().unwrap_or_default();
    cfg.printer = PrinterBindingConfig::default();
    save_config(&cfg)?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_from_qr() {
        let qr = "C8:F0:9E:C3:10:0A&FZJJD340654&FZS-C3100A";
        let info = QrCodeInfo::parse(qr);
        let profile = info.infer_profile();

        assert_eq!(profile.default_feed_steps, 284);
        assert_eq!(profile.model_name, "FZS-C3100A");
    }
}
