//! Cross-platform Bluetooth Low Energy (BLE) transport powered by `btleplug`.
//!
//! Supports macOS (CoreBluetooth) and Linux (BlueZ).
//! Features:
//! - Direct fast connect via cached CoreBluetooth UUID or MAC address
//! - ATT Write Request with ACK response flow control
//! - Hardware buffer-full (`FA 5A FB`) automatic pause handling

use crate::traits::{PrinterTransport, TransportError};
use btleplug::api::{
    Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Manager, Peripheral};
use futures::StreamExt;
use fzs_protocol::status::{parse_printer_response, PrinterStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};
use tokio::time::sleep;
use tracing::{debug, info, warn};

pub const SERVICE_UUID_FFF0_PREFIX: &str = "0000fff0";
pub const SERVICE_UUID_FFE0_PREFIX: &str = "0000ffe0";
pub const CHAR_WRITE_PREFIX_FFF1: &str = "0000fff1";
pub const CHAR_WRITE_PREFIX_FF01: &str = "0000ff01";
pub const CHAR_NOTIFY_PREFIX_FFF2: &str = "0000fff2";
pub const CHAR_NOTIFY_PREFIX_FF02: &str = "0000ff02";

#[derive(Debug, Clone)]
pub struct DiscoveredPrinter {
    pub name: String,
    pub identifier: String, // UUID on macOS, MAC on Linux
    pub rssi: Option<i16>,
}

pub struct BleTransport {
    peripheral: Peripheral,
    write_char: Characteristic,
    #[allow(dead_code)]
    notify_char: Option<Characteristic>,
    last_status: Arc<Mutex<Option<PrinterStatus>>>,
    completion_rx: watch::Receiver<Option<PrinterStatus>>,
    buffer_paused: Arc<AtomicBool>,
    is_connected: Arc<AtomicBool>,
}

impl BleTransport {
    /// Scan for candidate thermal printers in the area.
    pub async fn scan_printers(
        timeout_secs: u64,
    ) -> Result<Vec<DiscoveredPrinter>, TransportError> {
        let manager = Manager::new()
            .await
            .map_err(|e| TransportError::Bluetooth(e.to_string()))?;
        let adapters = manager
            .adapters()
            .await
            .map_err(|e| TransportError::Bluetooth(e.to_string()))?;

        if adapters.is_empty() {
            return Err(TransportError::Bluetooth(
                "No Bluetooth adapters found".into(),
            ));
        }

        let adapter = &adapters[0];
        adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(|e| TransportError::Bluetooth(e.to_string()))?;

        info!(
            "Scanning for nearby BLE thermal printers for {}s...",
            timeout_secs
        );
        sleep(Duration::from_secs(timeout_secs)).await;

        let peripherals = adapter
            .peripherals()
            .await
            .map_err(|e| TransportError::Bluetooth(e.to_string()))?;

        let mut discovered = Vec::new();
        for p in peripherals {
            if let Ok(Some(props)) = p.properties().await {
                let name = props.local_name.unwrap_or_default();
                let name_lower = name.to_lowercase();
                let is_candidate = name_lower.starts_with("fz")
                    || name_lower.contains("printer")
                    || name_lower.contains("teacher")
                    || props.services.iter().any(|u| {
                        let s = u.to_string().to_lowercase();
                        s.starts_with(SERVICE_UUID_FFF0_PREFIX)
                            || s.starts_with(SERVICE_UUID_FFE0_PREFIX)
                    });

                if is_candidate || !name.is_empty() {
                    discovered.push(DiscoveredPrinter {
                        name: if name.is_empty() {
                            "Unknown Device".into()
                        } else {
                            name
                        },
                        identifier: p.id().to_string(),
                        rssi: props.rssi,
                    });
                }
            }
        }

        let _ = adapter.stop_scan().await;
        Ok(discovered)
    }

    /// Connect to a printer, prioritizing cached UUID/MAC for ultra-fast connection.
    pub async fn connect(
        target_uuid: Option<&str>,
        target_mac: Option<&str>,
        target_model: Option<&str>,
    ) -> Result<Self, TransportError> {
        let manager = Manager::new()
            .await
            .map_err(|e| TransportError::Bluetooth(e.to_string()))?;
        let adapters = manager
            .adapters()
            .await
            .map_err(|e| TransportError::Bluetooth(e.to_string()))?;

        if adapters.is_empty() {
            return Err(TransportError::Bluetooth(
                "No Bluetooth adapters found".into(),
            ));
        }

        let adapter = &adapters[0];

        // 1. Try fast direct match from existing adapter peripheral list (no scan delay!)
        let peripherals = adapter.peripherals().await.unwrap_or_default();
        let mut target_peripheral: Option<Peripheral> = None;

        for p in peripherals {
            let id = p.id().to_string();
            if let Some(uuid) = target_uuid {
                if id.eq_ignore_ascii_case(uuid) {
                    target_peripheral = Some(p);
                    break;
                }
            }
            if let Ok(Some(props)) = p.properties().await {
                if let Some(mac) = target_mac {
                    let addr_str = props.address.to_string();
                    if addr_str.eq_ignore_ascii_case(mac) {
                        target_peripheral = Some(p);
                        break;
                    }
                }
                if let Some(model) = target_model {
                    if let Some(ref name) = props.local_name {
                        if name.eq_ignore_ascii_case(model) || name.starts_with(model) {
                            target_peripheral = Some(p);
                            break;
                        }
                    }
                }
            }
        }

        // 2. If not found in cache, trigger quick 4-second active scan
        if target_peripheral.is_none() {
            info!("Direct BLE cache miss. Initiating quick 4s active scan...");
            let _ = adapter.start_scan(ScanFilter::default()).await;
            sleep(Duration::from_secs(4)).await;

            let scanned = adapter.peripherals().await.unwrap_or_default();
            for p in scanned {
                let id = p.id().to_string();
                if let Some(uuid) = target_uuid {
                    if id.eq_ignore_ascii_case(uuid) {
                        target_peripheral = Some(p);
                        break;
                    }
                }
                if let Ok(Some(props)) = p.properties().await {
                    if let Some(mac) = target_mac {
                        let addr_str = props.address.to_string();
                        if addr_str.eq_ignore_ascii_case(mac) {
                            target_peripheral = Some(p);
                            break;
                        }
                    }
                    if let Some(model) = target_model {
                        if let Some(ref name) = props.local_name {
                            if name.eq_ignore_ascii_case(model) || name.starts_with(model) {
                                target_peripheral = Some(p);
                                break;
                            }
                        }
                    }
                    // Auto-fallback: any printer
                    if target_peripheral.is_none() {
                        let name_lower = props.local_name.unwrap_or_default().to_lowercase();
                        if name_lower.starts_with("fz") || name_lower.contains("printer") {
                            target_peripheral = Some(p);
                            break;
                        }
                    }
                }
            }
            let _ = adapter.stop_scan().await;
        }

        let peripheral = target_peripheral.ok_or_else(|| {
            TransportError::DeviceNotFound(format!(
                "Could not find thermal printer (target_uuid={:?}, target_mac={:?}, model={:?})",
                target_uuid, target_mac, target_model
            ))
        })?;

        info!("Connecting to BLE peripheral {:?}...", peripheral.id());
        peripheral
            .connect()
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        peripheral
            .discover_services()
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        // Locate write and notify characteristics
        let chars = peripheral.characteristics();
        let mut write_char = None;
        let mut notify_char = None;

        for c in &chars {
            let uuid_s = c.uuid.to_string().to_lowercase();
            // Write char
            let is_known_write = uuid_s.starts_with(CHAR_WRITE_PREFIX_FFF1)
                || uuid_s.starts_with(CHAR_WRITE_PREFIX_FF01);
            let has_write_prop = c.properties.contains(CharPropFlags::WRITE)
                || c.properties.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE);

            if is_known_write || (write_char.is_none() && has_write_prop) {
                write_char = Some(c.clone());
            }

            // Notify char
            let is_known_notify = uuid_s.starts_with(CHAR_NOTIFY_PREFIX_FFF2)
                || uuid_s.starts_with(CHAR_NOTIFY_PREFIX_FF02);
            let has_notify_prop = c.properties.contains(CharPropFlags::NOTIFY)
                || c.properties.contains(CharPropFlags::INDICATE);

            if is_known_notify || (notify_char.is_none() && has_notify_prop) {
                notify_char = Some(c.clone());
            }
        }

        let write_char = write_char.ok_or_else(|| {
            TransportError::Bluetooth("No writable GATT characteristic found on device".into())
        })?;

        info!(
            "BLE connected! Write Char: {}, Notify Char: {:?}",
            write_char.uuid,
            notify_char.as_ref().map(|c| c.uuid)
        );

        let last_status = Arc::new(Mutex::new(None));
        let (completion_tx, completion_rx) = watch::channel(None);
        let buffer_paused = Arc::new(AtomicBool::new(false));
        let is_connected = Arc::new(AtomicBool::new(true));

        // Start notification listener loop if notify char is present
        if let Some(ref nc) = notify_char {
            let _ = peripheral.subscribe(nc).await;
            if let Ok(mut stream) = peripheral.notifications().await {
                let status_ref = Arc::clone(&last_status);
                let pause_ref = Arc::clone(&buffer_paused);
                let is_conn_ref = Arc::clone(&is_connected);

                tokio::spawn(async move {
                    while let Some(notif) = stream.next().await {
                        let status = parse_printer_response(&notif.value);
                        debug!(
                            "BLE Notification received: {:?} (raw={:?})",
                            status, notif.value
                        );

                        {
                            let mut lock = status_ref.lock().await;
                            *lock = Some(status);
                        }

                        match status {
                            PrinterStatus::BufferFull => {
                                warn!("Printer buffer full! Flow-control pausing 6s...");
                                pause_ref.store(true, Ordering::SeqCst);
                                let p_clone = Arc::clone(&pause_ref);
                                tokio::spawn(async move {
                                    sleep(Duration::from_secs(6)).await;
                                    p_clone.store(false, Ordering::SeqCst);
                                    info!("Printer buffer resumed.");
                                });
                            }
                            PrinterStatus::Success
                            | PrinterStatus::OutOfPaper
                            | PrinterStatus::PausedOrHalted => {
                                let _ = completion_tx.send(Some(status));
                            }
                            _ => {}
                        }
                    }
                    is_conn_ref.store(false, Ordering::SeqCst);
                });
            }
        }

        Ok(Self {
            peripheral,
            write_char,
            notify_char,
            last_status,
            completion_rx,
            buffer_paused,
            is_connected,
        })
    }
}

impl PrinterTransport for BleTransport {
    async fn write(
        &mut self,
        data: &[u8],
        delay_ms: u64,
        with_response: bool,
    ) -> Result<(), TransportError> {
        // Wait if buffer full
        while self.buffer_paused.load(Ordering::SeqCst) {
            sleep(Duration::from_millis(200)).await;
        }

        let has_resp = self.write_char.properties.contains(CharPropFlags::WRITE);
        let write_type = if with_response && has_resp {
            WriteType::WithResponse
        } else {
            WriteType::WithoutResponse
        };

        self.peripheral
            .write(&self.write_char, data, write_type)
            .await
            .map_err(|e| TransportError::WriteFailed(e.to_string()))?;

        if delay_ms > 0 {
            sleep(Duration::from_millis(delay_ms)).await;
        }

        Ok(())
    }

    async fn wait_for_completion(
        &mut self,
        timeout: Duration,
    ) -> Result<PrinterStatus, TransportError> {
        match tokio::time::timeout(timeout, async {
            while self.completion_rx.changed().await.is_ok() {
                if let Some(status) = *self.completion_rx.borrow() {
                    return Ok(status);
                }
            }
            Ok(PrinterStatus::Unknown)
        })
        .await
        {
            Ok(res) => res,
            Err(_) => {
                let lock = self.last_status.lock().await;
                Ok(lock.unwrap_or(PrinterStatus::Unknown))
            }
        }
    }

    async fn disconnect(&mut self) -> Result<(), TransportError> {
        self.is_connected.store(false, Ordering::SeqCst);
        let _ = self.peripheral.disconnect().await;
        info!("Disconnected from BLE printer.");
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }
}
