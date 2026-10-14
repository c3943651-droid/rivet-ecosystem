//! Little-endian reads without external dependencies.

/// Lee un `u16` little-endian en `buf[off..off+2]`.
///
/// # Panics
///
/// Si no hay 2 bytes disponibles.
#[must_use]
pub(crate) fn read_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(buf[off..off + 2].try_into().expect("read_u16: 2 bytes"))
}

/// Lee un `i16` little-endian en `buf[off..off+2]`.
///
/// # Panics
///
/// Si no hay 2 bytes disponibles.
#[must_use]
pub(crate) fn read_i16(buf: &[u8], off: usize) -> i16 {
    i16::from_le_bytes(buf[off..off + 2].try_into().expect("read_i16: 2 bytes"))
}
