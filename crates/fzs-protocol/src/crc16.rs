//! 16-bit Modbus CRC implementation matching official FZS / TeacherSaid hardware.
//! Polynomial: 0xA001 (reversed 0x8005), Initial value: 0xFFFF.

/// Compute CRC16-MODBUS checksum for the given byte slice.
///
/// Guaranteed to match the official Python and Android implementations.
#[inline]
pub fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc16_empty() {
        assert_eq!(crc16_modbus(b""), 0xFFFF);
    }

    #[test]
    fn test_crc16_feed_steps() {
        // steps = 284 -> 0x011C -> little endian: [0x1C, 0x01]
        let payload = 284u16.to_le_bytes();
        let crc = crc16_modbus(&payload);
        assert_eq!(crc, 0xB0C8);
    }

    #[test]
    fn test_crc16_density() {
        // 10 bytes zeros + density 180 (0x00B4, le: [0xB4, 0x00])
        let mut payload = vec![0u8; 10];
        payload.extend_from_slice(&180u16.to_le_bytes());
        let crc = crc16_modbus(&payload);
        assert_eq!(crc, 0x0213);
    }
}
