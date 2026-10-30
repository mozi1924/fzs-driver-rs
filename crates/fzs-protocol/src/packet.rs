//! Frame construction for FZS / TeacherSaid Thermal Printer communication.

use crate::crc16::crc16_modbus;
use crate::status::{PaperSpec, PaperType};

pub const CHUNK_RAW_SIZE: usize = 416; // 416 bytes raw per slice (2 scanlines @ 1664 dots/line)
pub const DEFAULT_WIDTH: usize = 1664; // Standard A4 thermal width in dots (208mm @ 203 DPI)
pub const DEFAULT_PAPER_STEPS: u16 = 284; // Default feed step after print for FZS-C3100A / FZJJ
pub const DEFAULT_DENSITY: u16 = 180; // Default heating parameter (180 - 200)

/// FA 01 01 00 00 00 00 FB: Initialize / Start Print Job.
#[inline]
pub fn build_init_packet() -> Vec<u8> {
    vec![0xFA, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFB]
}

/// FA 02 01 00 12 ... FB: Print Job Configuration.
#[inline]
pub fn build_config_packet() -> Vec<u8> {
    vec![
        0xFA, 0x02, 0x01, 0x00, 0x12, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
        0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0xFB,
    ]
}

/// Set paper type (Continuous vs Folded).
#[inline]
pub fn build_paper_type_packet(paper_type: PaperType) -> Vec<u8> {
    match paper_type {
        PaperType::Continuous => vec![0xFA, 0x06, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0xFB],
        PaperType::Folded => vec![0xFA, 0x06, 0x02, 0x00, 0x00, 0xFF, 0xFF, 0xFB],
    }
}

/// Set paper specification (Spec 1 vs Spec 2).
#[inline]
pub fn build_paper_spec_packet(spec: PaperSpec) -> Vec<u8> {
    match spec {
        PaperSpec::Spec1 => vec![0xFA, 0x18, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0xFB],
        PaperSpec::Spec2 => vec![0xFA, 0x18, 0x02, 0x00, 0x00, 0xFF, 0xFF, 0xFB],
    }
}

/// Set paper feed steps after printing (FA 0D 01 ... FB).
///
/// Structure:
/// - Header:  `FA 0D 01` (3 bytes)
/// - Length:  `02 00` (2 bytes LE)
/// - Payload: `steps` (2 bytes LE)
/// - CRC16:   CRC16 of payload (2 bytes LE)
/// - Footer:  `FB` (1 byte)
///
/// Total length: 10 bytes.
pub fn build_paper_steps_packet(steps: u16) -> Vec<u8> {
    let payload = steps.to_le_bytes();
    let crc = crc16_modbus(&payload);
    let mut pkt = Vec::with_capacity(10);
    pkt.extend_from_slice(&[0xFA, 0x0D, 0x01, 0x02, 0x00]);
    pkt.extend_from_slice(&payload);
    pkt.extend_from_slice(&crc.to_le_bytes());
    pkt.push(0xFB);
    pkt
}

/// Set thermal head heating parameters / density (FA 12 01 ... FB).
///
/// Structure:
/// - Header:  `FA 12 01` (3 bytes)
/// - Length:  `0C 00` (12 bytes LE)
/// - Payload: 10 bytes zeros + `density_steps` (2 bytes LE)
/// - CRC16:   CRC16 of 12-byte payload (2 bytes LE)
/// - Footer:  `FB` (1 byte)
///
/// Total length: 20 bytes.
pub fn build_heating_density_packet(density_steps: u16) -> Vec<u8> {
    let mut payload = [0u8; 12];
    payload[10..12].copy_from_slice(&density_steps.to_le_bytes());
    let crc = crc16_modbus(&payload);

    let mut pkt = Vec::with_capacity(20);
    pkt.extend_from_slice(&[0xFA, 0x12, 0x01, 0x0C, 0x00]);
    pkt.extend_from_slice(&payload);
    pkt.extend_from_slice(&crc.to_le_bytes());
    pkt.push(0xFB);
    pkt
}

/// Build compressed raster data packet (FA 03 01 ... FB).
///
/// Structure:
/// - Header:  `FA 03 01` (3 bytes)
/// - Length:  `comp_len` (2 bytes LE)
/// - Payload: LZO1X-1 compressed data
/// - CRC16:   CRC16 of compressed data (2 bytes LE)
/// - Footer:  `FB` (1 byte)
///
/// Total length: len(comp_bytes) + 8 bytes.
pub fn build_raster_slice_packet(comp_bytes: &[u8]) -> Vec<u8> {
    let comp_len = comp_bytes.len() as u16;
    let crc = crc16_modbus(comp_bytes);

    let mut pkt = Vec::with_capacity(comp_bytes.len() + 8);
    pkt.extend_from_slice(&[0xFA, 0x03, 0x01]);
    pkt.extend_from_slice(&comp_len.to_le_bytes());
    pkt.extend_from_slice(comp_bytes);
    pkt.extend_from_slice(&crc.to_le_bytes());
    pkt.push(0xFB);
    pkt
}

/// FA 16 01 00 00 FF FF FB: End of Print Job / Flush.
#[inline]
pub fn build_finish_packet() -> Vec<u8> {
    vec![0xFA, 0x16, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0xFB]
}

/// FA 05 01 00 00 00 00 FB: Cancel printing.
#[inline]
pub fn build_cancel_packet() -> Vec<u8> {
    vec![0xFA, 0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFB]
}

/// FA 07 00 00 00 FF FF FB: Query printer firmware version string.
#[inline]
pub fn build_query_version_packet() -> Vec<u8> {
    vec![0xFA, 0x07, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFB]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_parity_with_python() {
        assert_eq!(
            build_init_packet(),
            vec![0xFA, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFB]
        );
        assert_eq!(
            build_config_packet(),
            vec![
                0xFA, 0x02, 0x01, 0x00, 0x12, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x01,
                0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0xFB,
            ]
        );
        assert_eq!(
            build_paper_type_packet(PaperType::Continuous),
            vec![0xFA, 0x06, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0xFB]
        );
        assert_eq!(
            build_paper_type_packet(PaperType::Folded),
            vec![0xFA, 0x06, 0x02, 0x00, 0x00, 0xFF, 0xFF, 0xFB]
        );
        assert_eq!(
            build_paper_spec_packet(PaperSpec::Spec1),
            vec![0xFA, 0x18, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0xFB]
        );
        assert_eq!(
            build_paper_spec_packet(PaperSpec::Spec2),
            vec![0xFA, 0x18, 0x02, 0x00, 0x00, 0xFF, 0xFF, 0xFB]
        );
        assert_eq!(
            build_cancel_packet(),
            vec![0xFA, 0x05, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFB]
        );
        assert_eq!(
            build_finish_packet(),
            vec![0xFA, 0x16, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0xFB]
        );
        assert_eq!(
            build_query_version_packet(),
            vec![0xFA, 0x07, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFB]
        );
    }

    #[test]
    fn test_paper_steps_packet_structure() {
        let pkt = build_paper_steps_packet(284);
        assert_eq!(pkt.len(), 10);
        assert_eq!(&pkt[0..3], &[0xFA, 0x0D, 0x01]);
        assert_eq!(&pkt[3..5], &[0x02, 0x00]);
        assert_eq!(&pkt[5..7], &284u16.to_le_bytes());
        assert_eq!(&pkt[7..9], &0xB0C8u16.to_le_bytes());
        assert_eq!(pkt[9], 0xFB);
    }

    #[test]
    fn test_heating_density_packet_structure() {
        let pkt = build_heating_density_packet(180);
        assert_eq!(pkt.len(), 20);
        assert_eq!(&pkt[0..3], &[0xFA, 0x12, 0x01]);
        assert_eq!(&pkt[3..5], &[0x0C, 0x00]);
        assert_eq!(&pkt[5..15], &[0u8; 10]);
        assert_eq!(&pkt[15..17], &180u16.to_le_bytes());
        assert_eq!(&pkt[17..19], &0x0213u16.to_le_bytes());
        assert_eq!(pkt[19], 0xFB);
    }
}
