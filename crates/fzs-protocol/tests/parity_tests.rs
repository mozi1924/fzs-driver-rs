//! Mathematical parity tests verifying 100% bit-exact equivalence with Python prototype.

use fzs_protocol::crc16::crc16_modbus;
use fzs_protocol::lzo::{compress_lzo1x, decompress_lzo1x};
use fzs_protocol::packet::{
    build_cancel_packet, build_config_packet, build_finish_packet, build_heating_density_packet,
    build_init_packet, build_paper_spec_packet, build_paper_steps_packet, build_paper_type_packet,
    build_query_version_packet, build_raster_slice_packet,
};
use fzs_protocol::qr::QrCodeInfo;
use fzs_protocol::status::{PaperSpec, PaperType};

#[test]
fn test_python_crc16_modbus_exact_vectors() {
    // 1. Empty string
    assert_eq!(crc16_modbus(b""), 0xFFFF);

    // 2. Feed steps 284 -> 0x011C -> LE: [0x1C, 0x01] -> Python: 0xB0C8
    assert_eq!(crc16_modbus(&284u16.to_le_bytes()), 0xB0C8);

    // 3. Feed steps 162 (FZXX) -> 0x00A2 -> LE: [0xA2, 0x00] -> Python: 0xD078
    let steps_162 = 162u16.to_le_bytes();
    assert_eq!(crc16_modbus(&steps_162), 0xD078);

    // 4. Heating density 180 (10 zeros + [0xB4, 0x00]) -> Python: 0x0213
    let mut density_payload = [0u8; 12];
    density_payload[10..12].copy_from_slice(&180u16.to_le_bytes());
    assert_eq!(crc16_modbus(&density_payload), 0x0213);
}

#[test]
fn test_python_lzo1x_compression_parity() {
    // 416 bytes of zeros (blank scanline slice)
    let zeros = vec![0u8; 416];
    let comp = compress_lzo1x(&zeros).expect("compress zeros");
    let hex = comp
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    // Byte-for-byte exact match with Python ctypes lzo1x_1_compress
    let py_reference_hex = "02000000000020006c10000c000000000000000000000000000000110000";
    assert_eq!(hex, py_reference_hex);

    // Roundtrip decompression
    let decomp = decompress_lzo1x(&comp, 416).expect("decompress");
    assert_eq!(decomp, zeros);
}

#[test]
fn test_python_frame_packets_parity() {
    // Init packet
    assert_eq!(build_init_packet(), bytes_from_hex("FA010100000000FB"));

    // Config packet
    assert_eq!(
        build_config_packet(),
        bytes_from_hex("FA020100120000000101000000010000000101000000010000FB")
    );

    // Paper type continuous & folded
    assert_eq!(
        build_paper_type_packet(PaperType::Continuous),
        bytes_from_hex("FA06010000FFFFFB")
    );
    assert_eq!(
        build_paper_type_packet(PaperType::Folded),
        bytes_from_hex("FA06020000FFFFFB")
    );

    // Paper spec 1 & 2
    assert_eq!(
        build_paper_spec_packet(PaperSpec::Spec1),
        bytes_from_hex("FA18010000FFFFFB")
    );
    assert_eq!(
        build_paper_spec_packet(PaperSpec::Spec2),
        bytes_from_hex("FA18020000FFFFFB")
    );

    // Finish, cancel, version query
    assert_eq!(build_finish_packet(), bytes_from_hex("FA16010000FFFFFB"));
    assert_eq!(build_cancel_packet(), bytes_from_hex("FA050100000000FB"));
    assert_eq!(
        build_query_version_packet(),
        bytes_from_hex("FA07000000FFFFFB")
    );

    // Post-print paper feed steps (284)
    assert_eq!(
        build_paper_steps_packet(284),
        bytes_from_hex("FA0D0102001C01C8B0FB")
    );

    // Heating density (180)
    assert_eq!(
        build_heating_density_packet(180),
        bytes_from_hex("FA12010C0000000000000000000000B4001302FB")
    );

    // Raster slice packet with LZO compressed zeros
    let comp = compress_lzo1x(&vec![0u8; 416]).unwrap();
    let slice_pkt = build_raster_slice_packet(&comp);
    assert_eq!(slice_pkt[0..3], [0xFA, 0x03, 0x01]);
    assert_eq!(slice_pkt[3..5], [30, 0]); // length = 30
    assert_eq!(slice_pkt[slice_pkt.len() - 1], 0xFB);
}

#[test]
fn test_qr_code_user_example() {
    let qr = "C8:F0:9E:C3:10:0A&FZJJD340654&FZS-C3100A";
    let info = QrCodeInfo::parse(qr);
    assert_eq!(info.mac.as_deref(), Some("C8:F0:9E:C3:10:0A"));
    assert_eq!(info.serial.as_deref(), Some("FZJJD340654"));
    assert_eq!(info.model.as_deref(), Some("FZS-C3100A"));

    let profile = info.infer_profile();
    assert_eq!(profile.default_feed_steps, 284);
}

fn bytes_from_hex(s: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut chars = s.chars();
    while let (Some(c1), Some(c2)) = (chars.next(), chars.next()) {
        let byte_str = format!("{}{}", c1, c2);
        bytes.push(u8::from_str_radix(&byte_str, 16).unwrap());
    }
    bytes
}
