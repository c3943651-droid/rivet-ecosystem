//! Parseo de `index.theme` y rutas de búsqueda de temas freedesktop.
//!
//! Especificación *Icon Theme*: herencia por `Inherits`, secciones por
//! directorio con `Size`/`Scale`/`Type` y prioridad `.svg` > `.png` > `.xpm`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Cómo se interpreta el `Size` de un directorio del tema.
#[derive(Debug, Clone, Copy)]
pub(crate) enum DirKind {
    /// Solo coincide el tamaño exacto.
    Fixed,
    /// Cualquier tamaño (SVG); se rasteriza al pedido.
    Scalable,
    /// `Size ± Threshold` (por defecto ±2).
    Threshold {
        /// Límite inferior.
        min: u32,
        /// Límite superior.
        max: u32,
    },
}

/// Un directorio de iconos declarado en `index.theme`.
#[derive(Debug, Clone)]
pub(crate) struct IconDir {
    /// Subdirectorio relativo al tema (p. ej. `48x48/apps`).
    pub path: String,
    /// Tamaño nominal.
    pub size: u32,
    /// Escala (HiDPI).
    pub scale: u32,
    /// Semántica de coincidencia.
    pub kind: DirKind,
}

/// Tema parseado: directorios + herencia.
#[derive(Debug, Clone, Default)]
pub(crate) struct ThemeIndex {
    /// Nombre del tema.
    pub name: String,
    /// Directorios en orden de prioridad (como en `Directories`).
    pub dirs: Vec<IconDir>,
    /// Cadenas de herencia (p. ej. `hicolor`).
    pub inherits: Vec<String>,
}

impl ThemeIndex {
    /// Parsea un `index.theme` desde disco; devuelve None si no existe.
    pub(crate) fn load(theme_dir: &Path) -> Option<ThemeIndex> {
        let text = std::fs::read_to_string(theme_dir.join("index.theme")).ok()?;
        let mut parsed = Self::parse(&text)?;
        parsed.name = theme_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        Some(parsed)
    }

    /// Parsea el contenido INI de un `index.theme`.
    pub(crate) fn parse(text: &str) -> Option<ThemeIndex> {
        let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                current = line[1..line.len() - 1].to_ascii_lowercase();
                sections.entry(current.clone()).or_default();
            } else if let Some((key, value)) = line.split_once('=') {
                sections
                    .entry(current.clone())
                    .or_default()
                    .insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }
        let main = sections.get("icon theme")?;
        let directories = main.get("directories")?;
        let inherits = main
            .get("inherits")
            .map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).collect())
            .unwrap_or_default();

        let mut dirs = Vec::new();
        for dir in directories.split(',') {
            let dir = dir.trim();
            let Some(section) = sections.get(&dir.to_ascii_lowercase()) else {
                continue;
            };
            let size = section
                .get("size")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if size == 0 {
                continue;
            }
            let scale = section
                .get("scale")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1)
                .max(1);
            let kind = match section.get("type").map(|s| s.as_str()) {
                // Scalable: si declara MinSize/MaxSize actúa como rango;
                // si no, sirve para cualquier tamaño (se rasteriza al vuelo).
                Some("Scalable") => match section
                    .get("minsize")
                    .and_then(|v| v.parse().ok())
                    .filter(|&v: &u32| v > 0)
                {
                    Some(min) => {
                        let max = section
                            .get("maxsize")
                            .and_then(|v| v.parse().ok())
                            .filter(|&v: &u32| v > 0)
                            .unwrap_or(size);
                        DirKind::Threshold { min, max }
                    }
                    None => DirKind::Scalable,
                },
                Some("Threshold") => {
                    let threshold: u32 = section
                        .get("threshold")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(2);
                    DirKind::Threshold {
                        min: size.saturating_sub(threshold).max(1),
                        max: size + threshold,
                    }
                }
                _ => DirKind::Fixed,
            };
            dirs.push(IconDir {
                path: dir.to_string(),
                size,
                scale,
                kind,
            });
        }
        if dirs.is_empty() {
            return None;
        }
        Some(ThemeIndex {
            name: String::new(),
            dirs,
            inherits,
        })
    }

    /// `true` si el directorio sirve para el tamaño/escala pedidos.
    pub(crate) fn dir_matches(dir: &IconDir, size: u32, scale: u32) -> bool {
        if dir.scale != scale && dir.scale != 1 {
            return false;
        }
        match dir.kind {
            DirKind::Fixed => dir.size == size,
            DirKind::Scalable => true,
            DirKind::Threshold { min, max } => size >= min && size <= max,
        }
    }
}

/// Rutas de búsqueda de temas según la especificación freedesktop.
#[must_use]
pub(crate) fn theme_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut push = |p: PathBuf| {
        if !paths.contains(&p) {
            paths.push(p);
        }
    };
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        push(home.join(".icons"));
        push(home.join(".local/share/icons"));
    }
    // $XDG_DATA_DIRS (defecto: /usr/local/share:/usr/share) + icons.
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for dir in data_dirs.split(':') {
        if !dir.is_empty() {
            push(PathBuf::from(dir).join("icons"));
        }
    }
    push(PathBuf::from("/usr/share/pixmaps"));
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = "
[Icon Theme]
Name=Test
Directories=16x16/apps,scalable/apps,48x48

[16x16/apps]
Size=16
Type=Fixed

[scalable/apps]
Size=128
Type=Scalable

[48x48]
Size=48
Type=Threshold
Threshold=8
";

    #[test]
    fn parsea_index_theme() {
        let theme = ThemeIndex::parse(INDEX).expect("válido");
        assert_eq!(theme.dirs.len(), 3);
        assert_eq!(theme.dirs[0].size, 16);
        assert!(matches!(theme.dirs[1].kind, DirKind::Scalable));
        assert!(matches!(
            theme.dirs[2].kind,
            DirKind::Threshold { min: 40, max: 56 }
        ));
    }

    #[test]
    fn coincidencia_de_tamanos() {
        let theme = ThemeIndex::parse(INDEX).unwrap();
        assert!(ThemeIndex::dir_matches(&theme.dirs[0], 16, 1));
        assert!(!ThemeIndex::dir_matches(&theme.dirs[0], 17, 1));
        assert!(ThemeIndex::dir_matches(&theme.dirs[1], 48, 1)); // scalable
        assert!(ThemeIndex::dir_matches(&theme.dirs[2], 50, 1)); // 40..56
        assert!(!ThemeIndex::dir_matches(&theme.dirs[2], 64, 1));
    }

    #[test]
    fn herencia() {
        let theme = ThemeIndex::parse(
            "[Icon Theme]\nName=T\nDirectories=48x48\nInherits=breeze,hicolor\n\n[48x48]\nSize=48\n",
        )
        .unwrap();
        assert_eq!(theme.inherits, vec!["breeze", "hicolor"]);
    }
}
