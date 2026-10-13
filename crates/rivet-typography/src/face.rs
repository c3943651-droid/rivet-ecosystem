//! Cara tipográfica: datos de la fuente + `ttf_parser::Face` parseada.
//!
//! `Face` toma prestados los bytes de la fuente; aquí los mantenemos vivos en
//! un `Arc<[u8]>` dentro de la misma estructura.

use std::sync::Arc;

use crate::error::{Result, TypographyError};

/// Fuente parseada con sus bytes residentes.
pub struct FontFace {
    /// Datos crudos de la fuente (mantienen vivo el préstamo de `face`).
    data: Arc<Vec<u8>>,
    /// Cara dentro de un posible `.ttc` (colección).
    index: u32,
    /// Cara parseada con lifetime "estático" ligado a `data`.
    face: ttf_parser::Face<'static>,
}

impl FontFace {
    /// Parsea una fuente desde bytes.
    pub fn from_bytes(data: Vec<u8>, index: u32) -> Result<Self> {
        let data = Arc::new(data);
        let face = ttf_parser::Face::parse(Arc::as_ref(&data).as_slice(), index)
            .map_err(|e| TypographyError::InvalidFont(e.to_string()))?;
        // SAFETY: `face` solo toma préstamos de `data`, que vive en el
        // propio `FontFace` y no se muta ni se saca de la estructura, por lo
        // que el lifetime estirado a 'static es correcto mientras viva self.
        let face =
            unsafe { std::mem::transmute::<ttf_parser::Face<'_>, ttf_parser::Face<'static>>(face) };
        Ok(Self { data, index, face })
    }

    /// Cara ttf-parser.
    #[must_use]
    pub fn face(&self) -> &ttf_parser::Face<'_> {
        // El lifetime real: ligado a &self, que guarda `data` viva.
        &self.face
    }

    /// Bytes crudos de la fuente.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Índice de cara (para colecciones).
    #[must_use]
    pub fn index(&self) -> u32 {
        self.index
    }
}

impl Clone for FontFace {
    fn clone(&self) -> Self {
        // Los bytes son Arc; el Face solo presta de ahí: clonar el Arc y
        // re-parsear sería lo conservador, pero el préstamo apunta a los
        // mismos bytes que este clon mantiene vivos.
        // SAFETY: mismo razonamiento que en `from_bytes`: `data` (clonado
        // del Arc original) vive mientras esta estructura y es inmutable.
        let face = unsafe {
            std::mem::transmute::<ttf_parser::Face<'_>, ttf_parser::Face<'static>>(
                self.face.clone(),
            )
        };
        Self {
            data: Arc::clone(&self.data),
            index: self.index,
            face,
        }
    }
}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontFace")
            .field("bytes", &self.data.len())
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}
