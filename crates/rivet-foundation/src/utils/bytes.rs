//! Helpers de endianness para protocolos binarios (Wayland, TrueType).
//!
//! Ambos protocolos serializan en little-endian; estos helpers leen y
//! escriben valores desde buffers sin copiar ni depender de la arquitectura.

/// Lee un `u16` little-endian en `buf[off..off+2]`.
///
/// # Panics
///
/// Si el buffer no tiene 2 bytes disponibles desde `off`.
#[must_use]
pub fn from_le_u16(buf: &[u8], off: usize) -> u16 {
    let b: [u8; 2] = buf[off..off + 2]
        .try_into()
        .expect("from_le_u16 requiere 2 bytes");
    u16::from_le_bytes(b)
}

/// Lee un `u32` little-endian en `buf[off..off+4]`.
///
/// # Panics
///
/// Si el buffer no tiene 4 bytes disponibles desde `off`.
#[must_use]
pub fn from_le_u32(buf: &[u8], off: usize) -> u32 {
    let b: [u8; 4] = buf[off..off + 4]
        .try_into()
        .expect("from_le_u32 requiere 4 bytes");
    u32::from_le_bytes(b)
}

/// Lee un `f32` little-endian en `buf[off..off+4]` (coords de Wayland).
///
/// # Panics
///
/// Si el buffer no tiene 4 bytes disponibles desde `off`.
#[must_use]
pub fn from_le_f32(buf: &[u8], off: usize) -> f32 {
    f32::from_bits(from_le_u32(buf, off))
}

/// Escribe un `u16` little-endian en `buf[off..off+2]`.
///
/// # Panics
///
/// Si el buffer no tiene 2 bytes disponibles desde `off`.
pub fn to_le_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

/// Escribe un `u32` little-endian en `buf[off..off+4]`.
///
/// # Panics
///
/// Si el buffer no tiene 4 bytes disponibles desde `off`.
pub fn to_le_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lecturas_little_endian() {
        let buf = [0x39, 0x05, 0xCD, 0xAB, 0x00, 0x00, 0x80, 0x3F];
        assert_eq!(from_le_u16(&buf, 0), 0x0539);
        assert_eq!(from_le_u32(&buf, 0), 0xAB_CD_05_39);
        assert!((from_le_f32(&buf, 4) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn escrituras_little_endian() {
        let mut buf = [0u8; 6];
        to_le_u16(&mut buf, 0, 0x0539);
        to_le_u32(&mut buf, 2, 0xAB_CD_05_39);
        assert_eq!(buf, [0x39, 0x05, 0x39, 0x05, 0xCD, 0xAB]);
    }
}
