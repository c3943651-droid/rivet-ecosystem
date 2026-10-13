//! Descubrimiento y carga de fuentes con `fontdb` (Rust puro, ADR-05).
//!
//! **Sin `fontconfig`**: el escaneo usa los directorios estándar de
//! `fontconfig` vía fontdb, sin enlazar la librería C.

use std::path::Path;

use fontdb::{Database, Family, Query, Weight};

use crate::error::{Result, TypographyError};
use crate::face::FontFace;
use crate::font::Font;

/// Base de fuentes del proceso: descubrimiento, carga y selección.
///
/// Las caras se identifican con [`FontId`] y se instancian como [`Font`]
/// (cara + tamaño) para medir y rastrerizar.
#[derive(Debug, Default)]
pub struct FontDb {
    db: Database,
    /// ID de la fuente empaquetada si ya se cargó (deduplica `load_bundled`).
    bundled: Option<crate::error::FontId>,
}

impl FontDb {
    /// Base vacía.
    #[must_use]
    pub fn new() -> Self {
        Self {
            db: Database::new(),
            bundled: None,
        }
    }

    /// Escanea los directorios estándar del sistema (llamar una vez).
    pub fn load_system(&mut self) {
        self.db.load_system_fonts();
    }

    /// Carga la fuente de respaldo empaquetada (Inter Regular, OFL 1.1).
    ///
    /// Doc §3: garantiza texto legible sin fuentes de sistema. Idempotente:
    /// la segunda llamada devuelve el mismo ID.
    pub fn load_bundled(&mut self) -> Result<crate::error::FontId> {
        if let Some(id) = self.bundled {
            return Ok(id);
        }
        let ids = self.load_bytes(crate::bundled::BUNDLED_FONT.to_vec())?;
        let id = ids[0];
        self.bundled = Some(id);
        Ok(id)
    }

    /// Selecciona la mejor cara o, si no hay ninguna, la empaquetada
    /// (cargándola bajo demanda). Deja la cadena usable siempre.
    pub fn select_or_bundled(
        &mut self,
        families: &[&str],
        weight: Weight,
    ) -> Result<crate::error::FontId> {
        self.select(families, weight)
            .map_or_else(|| self.load_bundled(), Ok)
    }

    /// Carga una fuente desde archivo (`.ttf`/`.otf`/`.ttc`).
    pub fn load_file(&mut self, path: impl AsRef<Path>) -> Result<Vec<crate::error::FontId>> {
        let before = self.face_count();
        let path = path.as_ref();
        if let Err(e) = self.db.load_font_file(path) {
            return Err(TypographyError::InvalidFont(format!(
                "{}: {e}",
                path.display()
            )));
        }
        self.new_ids_since(before)
    }

    /// Carga una fuente desde memoria (fuentes embebidas, assets).
    pub fn load_bytes(&mut self, data: Vec<u8>) -> Result<Vec<crate::error::FontId>> {
        let before = self.face_count();
        self.db.load_font_data(data);
        self.new_ids_since(before)
    }

    /// Selecciona la mejor cara para una consulta tipo CSS.
    ///
    /// `families`: lista priorizada (p. ej. `["Inter", Family::SansSerif]`).
    #[must_use]
    pub fn select(&self, families: &[&str], weight: Weight) -> Option<crate::error::FontId> {
        let generic = families.iter().map(|f| f.to_string()).collect::<Vec<_>>();
        let query = Query {
            families: &generic
                .iter()
                .map(|name| Family::Name(name.as_str()))
                .collect::<Vec<_>>(),
            weight,
            style: fontdb::Style::default(),
            stretch: fontdb::Stretch::default(),
        };
        self.db.query(&query)
    }

    /// Atajo: primera cara sans-serif del sistema.
    #[must_use]
    pub fn sans_serif(&self) -> Option<crate::error::FontId> {
        self.db
            .query(&Query {
                families: &[Family::SansSerif],
                weight: Weight::default(),
                style: fontdb::Style::default(),
                stretch: fontdb::Stretch::default(),
            })
            .or_else(|| self.db.faces().map(|f| f.id).next())
    }

    /// Instancia una cara como [`Font`] al tamaño dado (en píxeles).
    pub fn font(&self, id: crate::error::FontId, size_px: f32) -> Result<Font> {
        let face = self.face(id)?;
        Ok(Font::new(face, size_px))
    }

    /// Cara parseada lista para uso interno.
    pub fn face(&self, id: crate::error::FontId) -> Result<FontFace> {
        let bytes = self
            .db
            .with_face_data(id, |data, index| (data.to_vec(), index))
            .ok_or(TypographyError::UnknownFont(id))?;
        FontFace::from_bytes(bytes.0, bytes.1)
    }

    /// Nombre de familia principal de una cara.
    #[must_use]
    pub fn family_name(&self, id: crate::error::FontId) -> Option<String> {
        self.db
            .face(id)
            .and_then(|info| info.families.first().map(|f| f.0.clone()))
    }

    /// Número de caras cargadas.
    #[must_use]
    pub fn face_count(&self) -> usize {
        self.db.faces().count()
    }

    /// `true` si no hay caras (útil para decidir fallback empaquetado).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.face_count() == 0
    }

    fn new_ids_since(&self, before: usize) -> Result<Vec<crate::error::FontId>> {
        let added: Vec<_> = self.db.faces().skip(before).map(|info| info.id).collect();
        if added.is_empty() {
            return Err(TypographyError::InvalidFont(
                "la fuente no añadió ninguna cara".into(),
            ));
        }
        Ok(added)
    }
}
