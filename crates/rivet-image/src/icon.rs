//! Resolución de iconos por nombre según el tema freedesktop (doc §4).
//!
//! Búsqueda: tema pedido → cadena `Inherits` (hicolor al final) →
//! `/usr/share/pixmaps` → icono *missing* generado (fallback empaquetado
//! equivalente, sin depender de archivos del sistema).

use std::path::PathBuf;

use crate::data::ImageData;
use crate::decode;
use crate::error::{ImageError, Result};
use crate::missing;
use crate::svg;
use crate::theme::{theme_search_paths, ThemeIndex};

/// Extensiones en orden de prioridad (doc §4: `.svg` preferido).
const EXTENSIONS: [&str; 3] = ["svg", "png", "xpm"];

/// Profundidad máxima de herencia de temas (evita ciclos).
const MAX_DEPTH: usize = 8;

/// Resolvedor de iconos por tema.
pub struct IconTheme {
    search_paths: Vec<PathBuf>,
    themes: Vec<ThemeIndex>,
}

impl Default for IconTheme {
    fn default() -> Self {
        Self::new()
    }
}

impl IconTheme {
    /// Tema vacío (solo fallbacks); usa [`IconTheme::with_theme`] para
    /// cargar temas reales.
    #[must_use]
    pub fn new() -> Self {
        let search_paths = theme_search_paths();
        let themes = Self::load_theme(&search_paths, "hicolor")
            .into_iter()
            .collect();
        Self {
            search_paths,
            themes,
        }
    }

    /// Instancia con el tema pedido (más hicolor como base).
    #[must_use]
    pub fn with_theme(name: &str) -> Self {
        let search_paths = theme_search_paths();
        let mut themes = Self::load_theme(&search_paths, name)
            .into_iter()
            .collect::<Vec<_>>();
        if !themes.iter().any(|t| t.name == "hicolor") {
            themes.extend(Self::load_theme(&search_paths, "hicolor"));
        }
        Self {
            search_paths,
            themes,
        }
    }

    /// Busca un icono por nombre y lo entrega al tamaño/escala pedidos.
    ///
    /// SVG se rasteriza al vuelo; bitmaps se escalan (bilineal) si no hay
    /// coincidencia exacta. Si nada se encuentra, devuelve el icono
    /// *missing* generado (doc §4: fallback empaquetado).
    pub fn lookup(&self, name: &str, size: u32, scale: u32) -> Result<ImageData> {
        let scale = scale.max(1);
        // Nombres simbólicos: probar tal cual y sin sufijo.
        let base = name.strip_suffix("-symbolic").unwrap_or(name);
        let candidates = if name.ends_with("-symbolic") {
            vec![name.to_string(), base.to_string()]
        } else {
            vec![name.to_string()]
        };
        for candidate in &candidates {
            if let Some(image) = self.lookup_exact(candidate, size, scale) {
                return Ok(image);
            }
        }
        // Fallback determinista: icono missing generado.
        Ok(missing::missing_icon(size, scale))
    }

    /// Busca un icono simbólico y lo recolorea con el tinte del tema.
    pub fn lookup_symbolic(
        &self,
        name: &str,
        size: u32,
        scale: u32,
        tint: [u8; 4],
    ) -> Result<ImageData> {
        let name = name.strip_suffix("-symbolic").unwrap_or(name);
        let symbolic = format!("{name}-symbolic");
        if let Some(image) = self.lookup_exact(&symbolic, size, scale) {
            return Ok(image.recolor(tint));
        }
        if let Some(image) = self.lookup_exact(name, size, scale) {
            return Ok(image.recolor(tint));
        }
        Ok(missing::missing_icon(size, scale))
    }

    fn lookup_exact(&self, name: &str, size: u32, scale: u32) -> Option<ImageData> {
        let mut visited = std::collections::HashSet::new();
        for theme in &self.themes {
            if let Some(image) = self.search_theme(theme, name, size, scale, 0, &mut visited) {
                return Some(image);
            }
        }
        self.lookup_pixmaps(name, size, scale)
    }

    fn search_theme(
        &self,
        theme: &ThemeIndex,
        name: &str,
        size: u32,
        scale: u32,
        depth: usize,
        visited: &mut std::collections::HashSet<String>,
    ) -> Option<ImageData> {
        if depth >= MAX_DEPTH || !visited.insert(theme.name.clone()) {
            return None;
        }
        for dir in &theme.dirs {
            if !ThemeIndex::dir_matches(dir, size, scale) {
                continue;
            }
            for root in &self.search_paths {
                let dir_path = root.join(&theme.name).join(&dir.path);
                for ext in EXTENSIONS {
                    let file = dir_path.join(format!("{name}.{ext}"));
                    if let Ok(bytes) = std::fs::read(&file) {
                        return Some(match ext {
                            "svg" => svg::render(&bytes, size * scale, size * scale).ok()?,
                            _ => scale_to(decode::decode(&bytes).ok()?, size, scale),
                        });
                    }
                }
            }
        }
        // Herencia.
        for parent in &theme.inherits {
            let parent_theme = Self::load_theme(&self.search_paths, parent)?;
            if let Some(image) =
                self.search_theme(&parent_theme, name, size, scale, depth + 1, visited)
            {
                return Some(image);
            }
        }
        None
    }

    fn lookup_pixmaps(&self, name: &str, size: u32, scale: u32) -> Option<ImageData> {
        for root in &self.search_paths {
            if root.ends_with("pixmaps") {
                for ext in ["png", "svg", "xpm"] {
                    let file = root.join(format!("{name}.{ext}"));
                    if let Ok(bytes) = std::fs::read(&file) {
                        let image = if ext == "svg" {
                            svg::render(&bytes, size * scale, size * scale).ok()?
                        } else {
                            decode::decode(&bytes).ok()?
                        };
                        return Some(scale_to(image, size, scale));
                    }
                }
            }
        }
        None
    }

    fn load_theme(paths: &[PathBuf], name: &str) -> Option<ThemeIndex> {
        for root in paths {
            let dir = root.join(name);
            if dir.join("index.theme").is_file() {
                if let Some(theme) = ThemeIndex::load(&dir) {
                    return Some(theme);
                }
            }
        }
        None
    }
}

/// Escala una imagen para que quepa en `size × scale` si es necesario.
fn scale_to(image: ImageData, size: u32, scale: u32) -> ImageData {
    let target = size.max(1) * scale.max(1);
    let (w, h) = image.size();
    if w == target && h == target {
        image
    } else if w <= target && h <= target {
        // No ampliar bitmaps pequeños: mantener (solo recortar al canvas si
        // sobresalen). v1: si es más grande que el objetivo, reducir.
        image
    } else {
        // Reducción conservando proporciones al máximo lado.
        let k = target.max(1) as f32 / w.max(h) as f32;
        let (nw, nh) = (
            (w as f32 * k).round().max(1.0) as u32,
            (h as f32 * k).round().max(1.0) as u32,
        );
        image.resize(nw, nh, crate::transform::Filter::Bilinear)
    }
}

/// Error de icono con contexto (no encontrado en ningún tema).
#[allow(dead_code)]
fn not_found(name: &str) -> ImageError {
    ImageError::NotFound(format!("icono {name:?}"))
}
