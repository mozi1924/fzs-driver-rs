//! Background TCP AppSocket / JetDirect print daemon for FZS thermal printer.

use fzs_cups::parse_raster_stream;
use fzs_halftone::{prepare_image, DitheringMethod, FitMode, DEFAULT_A4_WIDTH};
use fzs_protocol::lzo::compress_lzo1x;
use fzs_protocol::packet::{
    build_config_packet, build_finish_packet, build_heating_density_packet, build_init_packet,
    build_paper_spec_packet, build_paper_steps_packet, build_paper_type_packet,
    build_raster_slice_packet,
};
use fzs_protocol::status::PrinterStatus;
use fzs_transport::ble::BleTransport;
use fzs_transport::config::load_config;
use fzs_transport::traits::PrinterTransport;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

pub struct CupsPrintDaemon {
    pub host: String,
    pub port: u16,
    pub device_uuid: Option<String>,
    pub device_mac: Option<String>,
    pub device_model: Option<String>,
    printer_lock: Arc<Mutex<Option<BleTransport>>>,
}

impl CupsPrintDaemon {
    pub fn new(
        host: &str,
        port: u16,
        device_uuid: Option<&str>,
        device_mac: Option<&str>,
        device_model: Option<&str>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            device_uuid: device_uuid.map(Into::into),
            device_mac: device_mac.map(Into::into),
            device_model: device_model.map(Into::into),
            printer_lock: Arc::new(Mutex::new(None)),
        }
    }

    /// Run the TCP daemon serving print requests until cancelled.
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!(
            "FZS Thermal Printer Daemon active and listening on {}",
            addr
        );
        info!("CUPS backend URI: socket://{}", addr);

        let printer_lock = Arc::clone(&self.printer_lock);
        let uuid = self.device_uuid.clone();
        let mac = self.device_mac.clone();
        let model = self.device_model.clone();

        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    info!("Incoming print connection from {}", peer);
                    let lock_clone = Arc::clone(&printer_lock);
                    let uuid_c = uuid.clone();
                    let mac_c = mac.clone();
                    let model_c = model.clone();

                    tokio::spawn(async move {
                        if let Err(e) =
                            handle_client(stream, lock_clone, uuid_c, mac_c, model_c).await
                        {
                            error!("Error handling print job from {}: {}", peer, e);
                        }
                    });
                }
                Err(e) => {
                    error!("TCP listener accept error: {}", e);
                }
            }
        }
    }
}

/// Handle a single incoming print connection.
async fn handle_client(
    mut stream: TcpStream,
    printer_lock: Arc<Mutex<Option<BleTransport>>>,
    uuid_override: Option<String>,
    mac_override: Option<String>,
    model_override: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Read all payload data until connection closes
    let mut job_data = Vec::new();
    let mut buf = vec![0u8; 65536];

    while let Ok(n) = stream.read(&mut buf).await {
        if n == 0 {
            break;
        }
        job_data.extend_from_slice(&buf[..n]);
    }

    let job_size = job_data.len();
    info!(
        "Received print job payload: {} bytes ({:.1} KB)",
        job_size,
        job_size as f64 / 1024.0
    );

    if job_size == 0 {
        warn!("Empty print job received, ignoring.");
        return Ok(());
    }

    // 2. Decode pages: first try CUPS / PWG Raster parser
    let pages: Vec<Vec<Vec<u8>>> = if let Ok(raster_pages) = parse_raster_stream(&job_data) {
        info!(
            "Decoded {} page(s) from CUPS / PWG raster stream",
            raster_pages.len()
        );
        raster_pages
    } else if let Ok(dyn_img) = image::load_from_memory(&job_data) {
        // Standard image file (JPEG, PNG, etc.)
        info!(
            "Decoded standard image stream ({}x{})",
            dyn_img.width(),
            dyn_img.height()
        );
        let (chunks, _, _) = prepare_image(
            &dyn_img,
            DEFAULT_A4_WIDTH,
            FitMode::Page,
            3292,
            2380,
            true,
            DitheringMethod::FloydSteinberg,
            180,
            false,
            false,
        );
        vec![chunks]
    } else {
        return Err(format!(
            "Unrecognized print stream format (magic={:?})",
            &job_data[..job_data.len().min(8)]
        )
        .into());
    };

    if pages.is_empty() {
        warn!("No printable pages found in job.");
        return Ok(());
    }

    // 3. Load configuration preferences
    let cfg = load_config().unwrap_or_default();
    let target_uuid = uuid_override
        .as_deref()
        .or(cfg.printer.macos_uuid.as_deref());
    let target_mac = mac_override.as_deref().or(cfg.printer.mac.as_deref());
    let target_model = model_override.as_deref().or(Some(&cfg.printer.model[..]));

    let feed_steps = cfg.printer.feed_steps;
    let density = cfg.printer.density;
    let paper_type = cfg.printer.paper_type;
    let paper_spec = cfg.printer.paper_spec;

    // 4. Acquire printer connection
    let mut lock = printer_lock.lock().await;
    if lock.is_none() || !lock.as_ref().unwrap().is_connected() {
        info!("Connecting to thermal printer via BLE...");
        let transport = BleTransport::connect(target_uuid, target_mac, target_model).await?;
        *lock = Some(transport);
    }
    let printer = lock.as_mut().unwrap();

    // 5. Print each page
    let total_pages = pages.len();
    for (page_idx, chunks) in pages.iter().enumerate() {
        info!(
            "Printing page {}/{} ({} slices, density={}, feed_steps={})...",
            page_idx + 1,
            total_pages,
            chunks.len(),
            density,
            feed_steps
        );

        // a. Initialize
        printer.write(&build_init_packet(), 20, true).await?;

        // b. Configuration
        printer.write(&build_config_packet(), 20, true).await?;

        // c. Paper Type & Spec
        printer
            .write(&build_paper_type_packet(paper_type), 20, true)
            .await?;
        printer
            .write(&build_paper_spec_packet(paper_spec), 20, true)
            .await?;

        // d. Heating density
        printer
            .write(&build_heating_density_packet(density), 20, true)
            .await?;

        // e. Post-print feed steps
        printer
            .write(&build_paper_steps_packet(feed_steps), 20, true)
            .await?;

        // f. Transmit all slices with LZO1X compression
        for chunk in chunks {
            let compressed =
                compress_lzo1x(chunk).map_err(|e| format!("LZO compression error: {}", e))?;
            let pkt = build_raster_slice_packet(&compressed);
            printer.write(&pkt, 12, true).await?;
        }

        // g. Print finish / flush
        printer.write(&build_finish_packet(), 20, true).await?;

        // h. Wait for printer completion
        let status = printer.wait_for_completion(Duration::from_secs(30)).await?;
        info!(
            "Page {}/{} completed with status: {:?}",
            page_idx + 1,
            total_pages,
            status
        );

        if status == PrinterStatus::OutOfPaper || status == PrinterStatus::PausedOrHalted {
            error!("Printer reported error state: {:?}", status);
            break;
        }
    }

    Ok(())
}
