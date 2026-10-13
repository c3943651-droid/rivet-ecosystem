//! Codificación/decodificación de IDs de 64 bits del ecosistema.
//!
//! Formato cerrado (`docs/RIVET.md` §7): bits `0..=31` índice,
//! bits `32..=63` generación; `id = (generación << 32) | índice`.

/// Construye un ID de 64 bits a partir de generación e índice.
#[inline]
#[must_use]
pub const fn make_id(generation: u32, index: u32) -> u64 {
    ((generation as u64) << 32) | (index as u64)
}

/// Extrae el índice (bits bajos) de un ID.
#[inline]
#[must_use]
pub const fn id_index(id: u64) -> u32 {
    id as u32
}

/// Extrae la generación (bits altos) de un ID.
#[inline]
#[must_use]
pub const fn id_generation(id: u64) -> u32 {
    (id >> 32) as u32
}

/// Construye un ID a partir de sus partes.
#[macro_export]
macro_rules! make_id {
    ($gen:expr, $index:expr) => {
        $crate::utils::ids::make_id($gen, $index)
    };
}

/// Extrae el índice de un ID.
#[macro_export]
macro_rules! id_index {
    ($id:expr) => {
        $crate::utils::ids::id_index($id)
    };
}

/// Extrae la generación de un ID.
#[macro_export]
macro_rules! id_generation {
    ($id:expr) => {
        $crate::utils::ids::id_generation($id)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_roundtrip() {
        for (gen, index) in [(0, 0), (1, 42), (u32::MAX, u32::MAX), (7, 0)] {
            let id = make_id(gen, index);
            assert_eq!(id_generation(id), gen);
            assert_eq!(id_index(id), index);
        }
    }

    #[test]
    fn layout_bits() {
        let id = make_id(3, 9);
        assert_eq!(id >> 32, 3);
        assert_eq!(id & 0xFFFF_FFFF, 9);
    }
}
