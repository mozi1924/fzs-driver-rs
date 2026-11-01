//! WebAssembly bindings for FZS Thermal Printer Driver.
//!
//! Provides in-browser image dithering, layout simulation, WYSIWYG preview generation,
//! and complete protocol packet streaming via Web Bluetooth and Web Serial.

use fzs_halftone::{
    aspect::{FitMode, DEFAULT_A4_LOGICAL_HEIGHT, DEFAULT_A4_PHYSICAL_HEIGHT, DEFAULT_A4_WIDTH, MOTOR_ASPECT_RATIO},
    dither::DitheringMethod,
    packer::{prepare_image, CHUNK_SIZE},
};
use fzs_protocol::{
    lzo::compress_lzo1x,
    packet::{
        build_config_packet, build_finish_packet, build_heating_density_packet, build_init_packet,
        build_paper_spec_packet, build_paper_steps_packet, build_paper_type_packet,
        build_raster_slice_packet,
    },
    qr::QrCodeInfo,
    status::{PaperSpec, PaperType},
};
use image::{DynamicImage, RgbaImage};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmPrintConfig {
    #[serde(default = "default_fit_mode")]
    pub fit_mode: String, // "page", "width", "original"
    #[serde(default = "default_dither")]
    pub dither: String, // "smart_hybrid", "floyd_steinberg", "bayer", "threshold"
    #[serde(default = "default_threshold")]
    pub threshold: u8,
    #[serde(default = "default_density")]
    pub density: u16,
    #[serde(default = "default_paper_type")]
    pub paper_type: String, // "continuous", "folded"
    #[serde(default = "default_paper_spec")]
    pub paper_spec: String, // "spec1", "spec2"
    pub feed_steps: Option<u16>,
    #[serde(default = "default_auto_rotate")]
    pub auto_rotate: bool,
    #[serde(default)]
    pub invert: bool,
    pub page_height: Option<u32>,
    pub logical_height: Option<u32>,
}

fn default_fit_mode() -> String {
    "page".into()
}
fn default_dither() -> String {
    "smart_hybrid".into()
}
fn default_threshold() -> u8 {
    180
}
fn default_density() -> u16 {
    180
}
fn default_paper_type() -> String {
    "continuous".into()
}
fn default_paper_spec() -> String {
    "spec1".into()
}
fn default_auto_rotate() -> bool {
    true
}

impl Default for WasmPrintConfig {
    fn default() -> Self {
        Self {
            fit_mode: default_fit_mode(),
            dither: default_dither(),
            threshold: default_threshold(),
            density: default_density(),
            paper_type: default_paper_type(),
            paper_spec: default_paper_spec(),
            feed_steps: None,
            auto_rotate: default_auto_rotate(),
            invert: false,
            page_height: None,
            logical_height: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PreviewResult {
    pub width: u32,
    pub height: u32,
    pub slice_count: usize,
    pub paper_length_mm: f32,
    pub preview_rgba: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct ParsedQrResult {
    pub mac: Option<String>,
    pub serial: Option<String>,
    pub model: Option<String>,
    pub series: String,
    pub feed_steps: u16,
}

fn parse_fit_mode(s: &str) -> FitMode {
    match s.to_lowercase().as_str() {
        "width" => FitMode::Width,
        "original" => FitMode::Original,
        _ => FitMode::Page,
    }
}

fn parse_dither_method(s: &str) -> DitheringMethod {
    match s.to_lowercase().as_str() {
        "floyd_steinberg" | "floydsteinberg" => DitheringMethod::FloydSteinberg,
        "bayer" => DitheringMethod::Bayer,
        "threshold" => DitheringMethod::Threshold,
        _ => DitheringMethod::SmartHybrid,
    }
}

fn parse_paper_type(s: &str) -> PaperType {
    match s.to_lowercase().as_str() {
        "folded" => PaperType::Folded,
        _ => PaperType::Continuous,
    }
}

fn parse_paper_spec(s: &str) -> PaperSpec {
    match s.to_lowercase().as_str() {
        "spec2" => PaperSpec::Spec2,
        _ => PaperSpec::Spec1,
    }
}

/// Parse a printer QR string (e.g. from camera scan) and return device profile.
#[wasm_bindgen]
pub fn parse_printer_qr(qr_text: &str) -> Result<JsValue, JsValue> {
    let qr = QrCodeInfo::parse(qr_text);
    let profile = qr.infer_profile();

    let res = ParsedQrResult {
        mac: qr.mac,
        serial: qr.serial,
        model: qr.model,
        series: format!("{:?}", profile.series),
        feed_steps: profile.default_feed_steps,
    };

    serde_wasm_bindgen::to_value(&res)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

/// Generate a 1-bit thermal dot matrix preview with physical millimeter estimates.
#[wasm_bindgen]
pub fn generate_preview_from_image_bytes(
    image_bytes: &[u8],
    config_val: JsValue,
) -> Result<JsValue, JsValue> {
    let config: WasmPrintConfig = serde_wasm_bindgen::from_value(config_val)
        .unwrap_or_default();

    let dyn_img = image::load_from_memory(image_bytes)
        .map_err(|e| JsValue::from_str(&format!("Failed to decode image: {}", e)))?;

    render_preview_internal(&dyn_img, &config)
}

/// Generate preview from raw RGBA buffer (e.g. from Canvas or PDF.js ImageData).
#[wasm_bindgen]
pub fn generate_preview_from_rgba(
    rgba_bytes: &[u8],
    width: u32,
    height: u32,
    config_val: JsValue,
) -> Result<JsValue, JsValue> {
    let config: WasmPrintConfig = serde_wasm_bindgen::from_value(config_val)
        .unwrap_or_default();

    let img_buffer = RgbaImage::from_raw(width, height, rgba_bytes.to_vec())
        .ok_or_else(|| JsValue::from_str("Invalid RGBA buffer dimensions"))?;
    let dyn_img = DynamicImage::ImageRgba8(img_buffer);

    render_preview_internal(&dyn_img, &config)
}

fn render_preview_internal(
    dyn_img: &DynamicImage,
    config: &WasmPrintConfig,
) -> Result<JsValue, JsValue> {
    let fit = parse_fit_mode(&config.fit_mode);
    let dither = parse_dither_method(&config.dither);
    let page_height = config.page_height.unwrap_or(DEFAULT_A4_PHYSICAL_HEIGHT);
    let logical_height = config.logical_height.unwrap_or(DEFAULT_A4_LOGICAL_HEIGHT);

    let (chunks, w, h) = prepare_image(
        dyn_img,
        DEFAULT_A4_WIDTH,
        fit,
        page_height,
        logical_height,
        true,
        dither,
        config.threshold,
        config.auto_rotate,
        config.invert,
    );

    // Reconstruct 1-bit thermal dot preview into RGBA canvas buffer
    let mut preview_rgba = vec![255u8; (w * h * 4) as usize];
    let bytes_per_row = (w / 8) as usize;

    for (chunk_idx, chunk) in chunks.iter().enumerate() {
        let r1_start = chunk_idx * 2;
        let r2_start = r1_start + 1;

        // Row 1
        if (r1_start as u32) < h {
            unpack_row_to_rgba(
                &chunk[0..bytes_per_row],
                w as usize,
                r1_start as usize,
                &mut preview_rgba,
            );
        }
        // Row 2
        if (r2_start as u32) < h && chunk.len() >= CHUNK_SIZE {
            unpack_row_to_rgba(
                &chunk[bytes_per_row..CHUNK_SIZE],
                w as usize,
                r2_start as usize,
                &mut preview_rgba,
            );
        }
    }

    // 203 DPI = 8 dots/mm. Considering motor aspect ratio (1.383), physical mm length is:
    let physical_dots_y = (h as f64) / MOTOR_ASPECT_RATIO;
    let paper_length_mm = (physical_dots_y / 8.0) as f32;

    let result = PreviewResult {
        width: w,
        height: h,
        slice_count: chunks.len(),
        paper_length_mm,
        preview_rgba,
    };

    serde_wasm_bindgen::to_value(&result)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
}

#[inline]
fn unpack_row_to_rgba(row_bytes: &[u8], width: usize, y: usize, rgba: &mut [u8]) {
    let row_offset = y * width * 4;
    for x in 0..width {
        let byte_idx = x / 8;
        let bit_idx = 7 - (x % 8);
        let is_black = if byte_idx < row_bytes.len() {
            (row_bytes[byte_idx] & (1 << bit_idx)) != 0
        } else {
            false
        };

        let pixel_offset = row_offset + x * 4;
        if is_black {
            // Black thermal dot
            rgba[pixel_offset] = 0x22;
            rgba[pixel_offset + 1] = 0x22;
            rgba[pixel_offset + 2] = 0x22;
            rgba[pixel_offset + 3] = 0xFF;
        } else {
            // White thermal paper background
            rgba[pixel_offset] = 0xFF;
            rgba[pixel_offset + 1] = 0xFF;
            rgba[pixel_offset + 2] = 0xFF;
            rgba[pixel_offset + 3] = 0xFF;
        }
    }
}

/// Encode full print job into an array of Uint8Array packets ready for Web Bluetooth / Web Serial transmission.
#[wasm_bindgen]
pub fn encode_print_job_from_rgba(
    rgba_bytes: &[u8],
    width: u32,
    height: u32,
    config_val: JsValue,
) -> Result<js_sys::Array, JsValue> {
    let config: WasmPrintConfig = serde_wasm_bindgen::from_value(config_val)
        .unwrap_or_default();

    let img_buffer = RgbaImage::from_raw(width, height, rgba_bytes.to_vec())
        .ok_or_else(|| JsValue::from_str("Invalid RGBA buffer dimensions"))?;
    let dyn_img = DynamicImage::ImageRgba8(img_buffer);

    encode_print_job_internal(&dyn_img, &config)
}

/// Encode full print job from encoded image file bytes.
#[wasm_bindgen]
pub fn encode_print_job_from_image_bytes(
    image_bytes: &[u8],
    config_val: JsValue,
) -> Result<js_sys::Array, JsValue> {
    let config: WasmPrintConfig = serde_wasm_bindgen::from_value(config_val)
        .unwrap_or_default();

    let dyn_img = image::load_from_memory(image_bytes)
        .map_err(|e| JsValue::from_str(&format!("Failed to decode image: {}", e)))?;

    encode_print_job_internal(&dyn_img, &config)
}

fn encode_print_job_internal(
    dyn_img: &DynamicImage,
    config: &WasmPrintConfig,
) -> Result<js_sys::Array, JsValue> {
    let fit = parse_fit_mode(&config.fit_mode);
    let dither = parse_dither_method(&config.dither);
    let paper_type = parse_paper_type(&config.paper_type);
    let paper_spec = parse_paper_spec(&config.paper_spec);
    let page_height = config.page_height.unwrap_or(DEFAULT_A4_PHYSICAL_HEIGHT);
    let logical_height = config.logical_height.unwrap_or(DEFAULT_A4_LOGICAL_HEIGHT);
    let feed_steps = config.feed_steps.unwrap_or(284);

    let (chunks, _, _) = prepare_image(
        dyn_img,
        DEFAULT_A4_WIDTH,
        fit,
        page_height,
        logical_height,
        true,
        dither,
        config.threshold,
        config.auto_rotate,
        config.invert,
    );

    let js_packets = js_sys::Array::new();

    // 1. Init
    let init_pkt = js_sys::Uint8Array::from(&build_init_packet()[..]);
    js_packets.push(&init_pkt);

    // 2. Config
    let cfg_pkt = js_sys::Uint8Array::from(&build_config_packet()[..]);
    js_packets.push(&cfg_pkt);

    // 3. Paper Type
    let pt_pkt = js_sys::Uint8Array::from(&build_paper_type_packet(paper_type)[..]);
    js_packets.push(&pt_pkt);

    // 4. Paper Spec
    let ps_pkt = js_sys::Uint8Array::from(&build_paper_spec_packet(paper_spec)[..]);
    js_packets.push(&ps_pkt);

    // 5. Heating Density
    let hd_pkt = js_sys::Uint8Array::from(&build_heating_density_packet(config.density)[..]);
    js_packets.push(&hd_pkt);

    // 6. Paper Feed Steps
    let fs_pkt = js_sys::Uint8Array::from(&build_paper_steps_packet(feed_steps)[..]);
    js_packets.push(&fs_pkt);

    // 7. Raster Slices (LZO1X compressed pure-Rust)
    for chunk in chunks.iter() {
        let compressed = compress_lzo1x(chunk)
            .map_err(|e| JsValue::from_str(&format!("LZO compression failed: {}", e)))?;
        let slice_pkt = build_raster_slice_packet(&compressed);
        let js_slice = js_sys::Uint8Array::from(&slice_pkt[..]);
        js_packets.push(&js_slice);
    }

    // 8. Finish
    let fin_pkt = js_sys::Uint8Array::from(&build_finish_packet()[..]);
    js_packets.push(&fin_pkt);

    Ok(js_packets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_printer_qr_native() {
        let qr = "C8:F0:9E:C3:10:0A&FZJJD340654&FZS-C3100A";
        let info = QrCodeInfo::parse(qr);
        assert_eq!(info.mac.as_deref(), Some("C8:F0:9E:C3:10:0A"));
        assert_eq!(info.serial.as_deref(), Some("FZJJD340654"));
        assert_eq!(info.model.as_deref(), Some("FZS-C3100A"));
    }

    #[test]
    fn test_config_parsing() {
        let cfg = WasmPrintConfig::default();
        assert_eq!(cfg.fit_mode, "page");
        assert_eq!(cfg.dither, "smart_hybrid");
        assert_eq!(cfg.density, 180);
        assert_eq!(cfg.threshold, 180);
    }
}

