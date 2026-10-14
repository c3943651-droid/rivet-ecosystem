//! Bundled fallback font (doc §3, ADR-05).
//!
//! Guarantees readable text **without depending on the system**: Inter
//! Regular 4.1 embedded in the binary via `include_bytes!`.
//!
//! License: SIL Open Font License 1.1 — (c) 2016-2024 The Inter Project
//! Authors. Full text embedded in [`BUNDLED_FONT_LICENSE`].

/// Bytes de Inter Regular 4.1 (OpenType, ~595 KiB).
pub static BUNDLED_FONT: &[u8] = include_bytes!("../assets/Inter-Regular.otf");

/// Texto de la SIL Open Font License 1.1 (obligación de atribución).
pub static BUNDLED_FONT_LICENSE: &[u8] = include_bytes!("../assets/OFL-LICENSE.txt");
