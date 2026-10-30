//! CUPS queue installation, configuration, and service manager.

use crate::ppd::ensure_ppd_file;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::info;

pub const DEFAULT_PRINTER_NAME: &str = "FZS_Thermal_Printer";
pub const DEFAULT_PORT: u16 = 9100;
pub const LAUNCH_AGENT_LABEL: &str = "com.fzs.thermal.printer";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrinterQueueStatus {
    pub name: String,
    pub installed: bool,
    pub ready: bool,
    pub raw_status: String,
}

/// Register and enable the thermal printer in CUPS.
pub fn install_cups_printer(
    printer_name: &str,
    daemon_host: &str,
    daemon_port: u16,
    custom_ppd: Option<&Path>,
) -> Result<(), String> {
    let ppd_path = match custom_ppd {
        Some(p) => p.to_path_buf(),
        None => ensure_ppd_file(None).map_err(|e| format!("Failed to create PPD file: {}", e))?,
    };

    let uri = format!("socket://{}:{}", daemon_host, daemon_port);
    let desc = "FZS-C3100A Thermal Printer";
    let location = "Bluetooth Desktop Printer";

    info!("Registering CUPS printer '{}' -> {}...", printer_name, uri);

    let output = Command::new("lpadmin")
        .args([
            "-p",
            printer_name,
            "-E",
            "-v",
            &uri,
            "-P",
            &ppd_path.to_string_lossy(),
            "-D",
            desc,
            "-L",
            location,
            "-o",
            "PageSize=A4",
            "-o",
            "Resolution=203x281dpi",
        ])
        .output()
        .map_err(|e| format!("Failed to execute lpadmin: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("lpadmin failed: {}", err));
    }

    // Enable and accept jobs
    let _ = Command::new("cupsenable").arg(printer_name).output();
    let _ = Command::new("cupsaccept").arg(printer_name).output();

    info!(
        "CUPS printer '{}' successfully installed and enabled!",
        printer_name
    );
    Ok(())
}

/// Remove printer queue from CUPS.
pub fn uninstall_cups_printer(printer_name: &str) -> Result<(), String> {
    info!("Removing CUPS printer '{}'...", printer_name);
    let output = Command::new("lpadmin")
        .args(["-x", printer_name])
        .output()
        .map_err(|e| format!("Failed to execute lpadmin: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("lpadmin -x failed: {}", err));
    }

    info!("CUPS printer '{}' removed.", printer_name);
    Ok(())
}

/// Query CUPS queue status via `lpstat`.
pub fn get_printer_status(printer_name: &str) -> PrinterQueueStatus {
    match Command::new("lpstat").args(["-p", printer_name]).output() {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let is_installed = output.status.success();
            let is_idle = text.contains("is idle");
            let is_enabled = text.contains("enabled since");
            PrinterQueueStatus {
                name: printer_name.to_string(),
                installed: is_installed,
                ready: is_idle && is_enabled,
                raw_status: text,
            }
        }
        Err(e) => PrinterQueueStatus {
            name: printer_name.to_string(),
            installed: false,
            ready: false,
            raw_status: e.to_string(),
        },
    }
}

/// Get macOS LaunchAgent plist path (`~/Library/LaunchAgents/com.fzs.thermal.printer.plist`).
pub fn get_launch_agent_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", LAUNCH_AGENT_LABEL))
    })
}

/// Install macOS user LaunchAgent for automatic background startup on login.
#[cfg(target_os = "macos")]
pub fn install_launch_agent(exe_path: &Path, port: u16) -> Result<(), String> {
    let plist_path =
        get_launch_agent_path().ok_or_else(|| "Could not determine home directory".to_string())?;

    if let Some(parent) = plist_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let log_dir = dirs::home_dir()
        .map(|h| h.join("Library").join("Logs"))
        .unwrap_or_else(std::env::temp_dir);
    let _ = std::fs::create_dir_all(&log_dir);

    let stdout_log = log_dir.join("fzs-printer-daemon.log");
    let stderr_log = log_dir.join("fzs-printer-daemon.err.log");

    let plist_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>daemon</string>
        <string>--port</string>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}</string>
    <key>StandardErrorPath</key>
    <string>{}</string>
</dict>
</plist>
"#,
        LAUNCH_AGENT_LABEL,
        exe_path.to_string_lossy(),
        port,
        stdout_log.to_string_lossy(),
        stderr_log.to_string_lossy()
    );

    std::fs::write(&plist_path, plist_xml).map_err(|e| e.to_string())?;

    // Reload with launchctl
    let _ = Command::new("launchctl")
        .args(["unload", &plist_path.to_string_lossy()])
        .output();
    let res = Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .output()
        .map_err(|e| e.to_string())?;

    if !res.status.success() {
        return Err(String::from_utf8_lossy(&res.stderr).to_string());
    }

    info!("LaunchAgent registered and active at {:?}", plist_path);
    Ok(())
}

/// Uninstall macOS user LaunchAgent.
#[cfg(target_os = "macos")]
pub fn uninstall_launch_agent() -> Result<(), String> {
    if let Some(plist_path) = get_launch_agent_path() {
        if plist_path.exists() {
            let _ = Command::new("launchctl")
                .args(["unload", &plist_path.to_string_lossy()])
                .output();
            let _ = std::fs::remove_file(&plist_path);
            info!("LaunchAgent removed.");
        }
    }
    Ok(())
}
