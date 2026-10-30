//! CLI tool for FZS Thermal Printer management, printing, and CUPS integration.

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use fzs_cups::{
    get_printer_status, install_cups_printer, uninstall_cups_printer, DEFAULT_PORT,
    DEFAULT_PRINTER_NAME,
};
#[cfg(target_os = "macos")]
use fzs_cups::{install_launch_agent, uninstall_launch_agent};
use fzs_daemon::CupsPrintDaemon;
use fzs_halftone::{
    generate_test_pattern, prepare_image, DitheringMethod as HalftoneMethod,
    FitMode as HalftoneFit, DEFAULT_A4_WIDTH,
};
use fzs_protocol::lzo::compress_lzo1x;
use fzs_protocol::packet::{
    build_config_packet, build_finish_packet, build_heating_density_packet, build_init_packet,
    build_paper_spec_packet, build_paper_steps_packet, build_paper_type_packet,
    build_query_version_packet, build_raster_slice_packet,
};
use fzs_protocol::qr::QrCodeInfo;
use fzs_protocol::status::{PaperSpec, PaperType, PrinterStatus};
use fzs_transport::ble::BleTransport;
use fzs_transport::config::{bind_printer, load_config, unbind_printer};
use fzs_transport::traits::PrinterTransport;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "fzs-printer")]
#[command(about = "High-performance offline driver for FZS & TeacherSaid thermal printers", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Bind a printer via QR code string or interactive Bluetooth scan
    Bind {
        /// QR string (e.g. "C8:F0:9E:C3:10:0A&FZJJD340654&FZS-C3100A") or MAC/UUID
        qr_or_address: Option<String>,
        /// Interactively scan for nearby Bluetooth printers and select one to bind
        #[arg(long, short)]
        scan: bool,
    },
    /// Clear currently bound printer configuration
    Unbind,
    /// Display information about the currently bound printer
    Info,
    /// Scan for nearby Bluetooth Low Energy thermal printers
    Scan {
        /// Scan duration in seconds (default 5)
        #[arg(long, short, default_value_t = 5)]
        timeout: u64,
    },
    /// Print an image or document file
    Print(PrintArgs),
    /// Print diagnostic hardware alignment and gray ramp test page
    Test(TestArgs),
    /// Run the background TCP AppSocket (Port 9100) print daemon
    Daemon {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    /// Manage macOS / Linux system CUPS printer integration
    Cups {
        #[command(subcommand)]
        action: CupsAction,
    },
    /// Query the printer hardware firmware version string
    Version,
}

#[derive(Args)]
struct PrintArgs {
    /// Path to image or document file
    file: PathBuf,
    /// Page layout fitting mode
    #[arg(long, value_enum, default_value_t = CliFitMode::Page)]
    fit: CliFitMode,
    /// Dithering / halftoning algorithm
    #[arg(long, value_enum, default_value_t = CliDitherMethod::FloydSteinberg)]
    dither: CliDitherMethod,
    /// Binarization threshold for threshold dithering (0-255)
    #[arg(long, default_value_t = 180)]
    threshold: u8,
    /// Heating density (150-220, default 180)
    #[arg(long, default_value_t = 180)]
    density: u16,
    /// Paper feed type
    #[arg(long, value_enum, default_value_t = CliPaperType::Continuous)]
    paper: CliPaperType,
    /// Override post-print feed steps (default derived from model)
    #[arg(long)]
    steps: Option<u16>,
    /// Auto-rotate landscape images to portrait
    #[arg(long)]
    auto_rotate: bool,
    /// Invert black and white
    #[arg(long)]
    invert: bool,
}

#[derive(Args)]
struct TestArgs {
    /// Heating density (150-220, default 180)
    #[arg(long, default_value_t = 180)]
    density: u16,
}

#[derive(Subcommand)]
enum CupsAction {
    /// Register printer in system CUPS
    Install {
        #[arg(long, default_value = DEFAULT_PRINTER_NAME)]
        name: String,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long)]
        launch_agent: bool,
    },
    /// Query printer queue status
    Status {
        #[arg(long, default_value = DEFAULT_PRINTER_NAME)]
        name: String,
    },
    /// Remove printer queue from system CUPS
    Uninstall {
        #[arg(long, default_value = DEFAULT_PRINTER_NAME)]
        name: String,
        #[arg(long)]
        launch_agent: bool,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliFitMode {
    Page,
    Width,
    Original,
}

impl From<CliFitMode> for HalftoneFit {
    fn from(m: CliFitMode) -> Self {
        match m {
            CliFitMode::Page => HalftoneFit::Page,
            CliFitMode::Width => HalftoneFit::Width,
            CliFitMode::Original => HalftoneFit::Original,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliDitherMethod {
    FloydSteinberg,
    Bayer,
    Threshold,
}

impl From<CliDitherMethod> for HalftoneMethod {
    fn from(d: CliDitherMethod) -> Self {
        match d {
            CliDitherMethod::FloydSteinberg => HalftoneMethod::FloydSteinberg,
            CliDitherMethod::Bayer => HalftoneMethod::Bayer,
            CliDitherMethod::Threshold => HalftoneMethod::Threshold,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliPaperType {
    Continuous,
    Folded,
}

impl From<CliPaperType> for PaperType {
    fn from(p: CliPaperType) -> Self {
        match p {
            CliPaperType::Continuous => PaperType::Continuous,
            CliPaperType::Folded => PaperType::Folded,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let cli = Cli::parse();

    match cli.command {
        Commands::Bind {
            qr_or_address,
            scan,
        } => {
            handle_bind(qr_or_address, scan).await?;
        }
        Commands::Unbind => {
            unbind_printer()?;
            println!("✓ Bound printer configuration removed.");
        }
        Commands::Info => {
            handle_info()?;
        }
        Commands::Scan { timeout } => {
            println!(
                "🔍 Scanning for Bluetooth LE thermal printers ({}s)...",
                timeout
            );
            let devices = BleTransport::scan_printers(timeout).await?;
            if devices.is_empty() {
                println!("No thermal printers detected. Ensure your printer is powered on.");
            } else {
                println!("Found {} candidate device(s):", devices.len());
                for (i, d) in devices.iter().enumerate() {
                    let rssi_str = d
                        .rssi
                        .map(|r| format!("{} dBm", r))
                        .unwrap_or_else(|| "N/A".into());
                    println!(
                        "  [{}] {} (ID: {}, RSSI: {})",
                        i + 1,
                        d.name,
                        d.identifier,
                        rssi_str
                    );
                }
            }
        }
        Commands::Print(args) => {
            handle_print(args).await?;
        }
        Commands::Test(args) => {
            handle_test(args).await?;
        }
        Commands::Daemon { host, port } => {
            let daemon = CupsPrintDaemon::new(&host, port, None, None, None);
            println!("🚀 Starting FZS Print Daemon on {}:{}...", host, port);
            daemon.run().await.map_err(|e| anyhow::anyhow!("{}", e))?;
        }
        Commands::Cups { action } => match action {
            CupsAction::Install {
                name,
                host,
                port,
                launch_agent,
            } => {
                println!("Installing CUPS printer '{}'...", name);
                install_cups_printer(&name, &host, port, None).map_err(|e| anyhow::anyhow!(e))?;
                println!("✓ CUPS printer '{}' installed successfully!", name);

                #[cfg(target_os = "macos")]
                if launch_agent {
                    let exe = std::env::current_exe()?;
                    install_launch_agent(&exe, port).map_err(|e| anyhow::anyhow!(e))?;
                    println!("✓ LaunchAgent registered for auto-startup.");
                }
            }
            CupsAction::Status { name } => {
                let st = get_printer_status(&name);
                println!("CUPS Printer '{}' Status:", name);
                println!("  Installed: {}", st.installed);
                println!("  Ready:     {}", st.ready);
                println!("  Details:   {}", st.raw_status);
            }
            CupsAction::Uninstall { name, launch_agent } => {
                uninstall_cups_printer(&name).map_err(|e| anyhow::anyhow!(e))?;
                println!("✓ CUPS printer '{}' removed.", name);

                #[cfg(target_os = "macos")]
                if launch_agent {
                    uninstall_launch_agent().map_err(|e| anyhow::anyhow!(e))?;
                    println!("✓ LaunchAgent removed.");
                }
            }
        },
        Commands::Version => {
            handle_version().await?;
        }
    }

    Ok(())
}

async fn handle_bind(qr_or_address: Option<String>, scan: bool) -> Result<()> {
    match (scan, qr_or_address) {
        (true, _) | (false, None) => {
            println!("🔍 Scanning for nearby printers for interactive binding...");
            let devices = BleTransport::scan_printers(5).await?;
            if devices.is_empty() {
                bail!("No Bluetooth printers found. Make sure printer is powered on.");
            }

            println!("\nDetected Devices:");
            for (i, d) in devices.iter().enumerate() {
                println!("  [{}] {} (Identifier: {})", i + 1, d.name, d.identifier);
            }

            print!("\nSelect a printer to bind [1-{}]: ", devices.len());
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let choice: usize = input.trim().parse().context("Invalid selection index")?;

            if choice < 1 || choice > devices.len() {
                bail!("Selection out of range");
            }

            let selected = &devices[choice - 1];
            println!("Binding to '{}' ({})", selected.name, selected.identifier);

            let cfg = bind_printer(&selected.name, Some(&selected.identifier))?;
            println!(
                "✓ Successfully bound to {}! (Feed Steps: {})",
                cfg.printer.model, cfg.printer.feed_steps
            );
        }
        (false, Some(input_str)) => {
            let qr = QrCodeInfo::parse(&input_str);
            let profile = qr.infer_profile();

            println!("Parsed Device Information:");
            println!("  MAC:        {:?}", qr.mac);
            println!("  Serial:     {:?}", qr.serial);
            println!("  Model:      {}", profile.model_name);
            println!("  Series:     {:?}", profile.series);
            println!("  Feed Steps: {}", profile.default_feed_steps);

            let cfg = bind_printer(&input_str, None)?;
            println!(
                "✓ Successfully bound to {} (Model: {})",
                input_str, cfg.printer.model
            );
        }
    }
    Ok(())
}

fn handle_info() -> Result<()> {
    let cfg = load_config()?;
    println!("FZS Printer Configuration:");
    println!("  Model:        {}", cfg.printer.model);
    println!("  Serial (SN):  {:?}", cfg.printer.serial);
    println!("  MAC Address:  {:?}", cfg.printer.mac);
    println!("  macOS UUID:   {:?}", cfg.printer.macos_uuid);
    println!("  Feed Steps:   {} steps", cfg.printer.feed_steps);
    println!("  Heating:      {} / 220", cfg.printer.density);
    println!("  Paper Type:   {:?}", cfg.printer.paper_type);
    println!("  Daemon Port:  {}", cfg.daemon.port);
    Ok(())
}

async fn handle_print(args: PrintArgs) -> Result<()> {
    if !args.file.exists() {
        bail!("File not found: {:?}", args.file);
    }

    info!("Loading document {:?}", args.file);
    let dyn_img = image::open(&args.file).context("Failed to open image file")?;

    let cfg = load_config().unwrap_or_default();
    let feed_steps = args.steps.unwrap_or(cfg.printer.feed_steps);

    let fit: HalftoneFit = args.fit.into();
    let dither: HalftoneMethod = args.dither.into();
    info!("Preparing image (fit={:?}, dither={:?})...", fit, dither);
    let (chunks, w, h) = prepare_image(
        &dyn_img,
        DEFAULT_A4_WIDTH,
        fit,
        3292,
        2380,
        true,
        dither,
        args.threshold,
        args.auto_rotate,
        args.invert,
    );

    println!(
        "Image rendered: {}x{} dots, {} slice(s) ({} KB)",
        w,
        h,
        chunks.len(),
        (chunks.len() * 416) / 1024
    );

    // Connect to printer
    let target_uuid = cfg.printer.macos_uuid.as_deref();
    let target_mac = cfg.printer.mac.as_deref();
    let target_model = Some(&cfg.printer.model[..]);

    println!("Connecting to printer via BLE...");
    let mut transport = BleTransport::connect(target_uuid, target_mac, target_model).await?;

    print_slices_stream(
        &mut transport,
        &chunks,
        args.density,
        args.paper.into(),
        feed_steps,
    )
    .await?;
    println!("✓ Print completed successfully!");
    Ok(())
}

async fn handle_test(args: TestArgs) -> Result<()> {
    let pattern = generate_test_pattern(1664, 800);
    let cfg = load_config().unwrap_or_default();
    let feed_steps = cfg.printer.feed_steps;

    let (chunks, _, _) = prepare_image(
        &pattern,
        DEFAULT_A4_WIDTH,
        HalftoneFit::Width,
        800,
        800,
        false,
        HalftoneMethod::FloydSteinberg,
        180,
        false,
        false,
    );

    println!("Diagnostic pattern prepared ({} slices).", chunks.len());

    let target_uuid = cfg.printer.macos_uuid.as_deref();
    let target_mac = cfg.printer.mac.as_deref();
    let target_model = Some(&cfg.printer.model[..]);

    println!("Connecting to printer via BLE...");
    let mut transport = BleTransport::connect(target_uuid, target_mac, target_model).await?;

    print_slices_stream(
        &mut transport,
        &chunks,
        args.density,
        PaperType::Continuous,
        feed_steps,
    )
    .await?;
    println!("✓ Test pattern printed!");
    Ok(())
}

async fn handle_version() -> Result<()> {
    let cfg = load_config().unwrap_or_default();
    let target_uuid = cfg.printer.macos_uuid.as_deref();
    let target_mac = cfg.printer.mac.as_deref();
    let target_model = Some(&cfg.printer.model[..]);

    println!("Connecting to printer...");
    let mut transport = BleTransport::connect(target_uuid, target_mac, target_model).await?;

    transport
        .write(&build_query_version_packet(), 20, false)
        .await?;
    println!("Sent version query packet (FA 07 00). Waiting for response...");
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("Done.");
    Ok(())
}

async fn print_slices_stream(
    transport: &mut BleTransport,
    chunks: &[Vec<u8>],
    density: u16,
    paper_type: PaperType,
    feed_steps: u16,
) -> Result<()> {
    transport.write(&build_init_packet(), 20, true).await?;
    transport.write(&build_config_packet(), 20, true).await?;
    transport
        .write(&build_paper_type_packet(paper_type), 20, true)
        .await?;
    transport
        .write(&build_paper_spec_packet(PaperSpec::Spec1), 20, true)
        .await?;
    transport
        .write(&build_heating_density_packet(density), 20, true)
        .await?;
    transport
        .write(&build_paper_steps_packet(feed_steps), 20, true)
        .await?;

    let total = chunks.len();
    for (i, chunk) in chunks.iter().enumerate() {
        let compressed = compress_lzo1x(chunk).map_err(|e| anyhow::anyhow!("LZO error: {}", e))?;
        let pkt = build_raster_slice_packet(&compressed);
        transport.write(&pkt, 12, true).await?;

        if (i + 1) % 50 == 0 || i + 1 == total {
            print!(
                "\rTransmitting slices: {}/{} ({:.0}%)",
                i + 1,
                total,
                ((i + 1) as f64 / total as f64) * 100.0
            );
            io::stdout().flush()?;
        }
    }
    println!();

    transport.write(&build_finish_packet(), 20, true).await?;
    let status = transport
        .wait_for_completion(Duration::from_secs(30))
        .await?;
    if status == PrinterStatus::OutOfPaper {
        bail!("Printer out of paper!");
    }
    Ok(())
}
