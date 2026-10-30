//! Integration test testing macOS / Linux CUPS raster generation and pure Rust decoding.

use fzs_cups::ppd::ensure_ppd_file;
use fzs_cups::raster::parse_raster_stream;
use std::fs;
use std::process::Command;

#[test]
fn test_cupsfilter_and_pure_rust_raster_decoding() {
    let ppd_path = ensure_ppd_file(None).expect("create PPD");

    // Create a temporary text file to print
    let temp_txt = std::env::temp_dir().join("cups_test_input.txt");
    fs::write(
        &temp_txt,
        "CUPS Raster Integration Test\nTesting pure Rust decoder",
    )
    .unwrap();

    // Check if cupsfilter is installed
    if let Ok(output) = Command::new("cupsfilter")
        .args([
            "-p",
            &ppd_path.to_string_lossy(),
            "-m",
            "application/vnd.cups-raster",
            &temp_txt.to_string_lossy(),
        ])
        .output()
    {
        if output.status.success() {
            let raster_bytes = output.stdout;
            println!("cupsfilter stdout length: {}", raster_bytes.len());
            println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            if raster_bytes.len() >= 4 {
                println!("magic: {:?}", &raster_bytes[..4]);
            }

            // Decode with our pure Rust decoder
            let pages = parse_raster_stream(&raster_bytes).expect("parse raster stream");
            println!("Decoded {} page(s)", pages.len());
            if !pages.is_empty() {
                println!("Page 0 slices count: {}", pages[0].len());
            }
            assert_eq!(pages.len(), 1, "Expected 1 page");
            assert!(!pages[0].is_empty(), "Expected slices on page 1");

            // Every slice must be exactly 416 bytes!
            for (idx, slice) in pages[0].iter().enumerate() {
                assert_eq!(
                    slice.len(),
                    416,
                    "Slice {} has invalid size {}",
                    idx,
                    slice.len()
                );
            }
        }
    }

    let _ = fs::remove_file(temp_txt);
}
