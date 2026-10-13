//! Fuente de respaldo empaquetada (doc §3, ADR-05).
//!
//! Garantiza texto legible **sin depender del sistema**: Inter Regular 4.1
//! empaquetada en el binario vía `include_bytes!`.
//!
//! Licencia: SIL Open Font License 1.1 — (c) 2016-2024 The Inter Project
//! Authors. Texto completo embebido en [`BUNDLED_FONT_LICENSE`].

/// Bytes de Inter Regular 4.1 (OpenType, ~595 KiB).
pub static BUNDLED_FONT: &[u8] = include_bytes!("../assets/Inter-Regular.otf");

/// Texto de la SIL Open Font License 1.1 (obligación de atribución).
pub static BUNDLED_FONT_LICENSE: &[u8] = include_bytes!("../assets/OFL-LICENSE.txt");
