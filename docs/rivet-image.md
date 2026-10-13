# rivet-image — Documentación definitiva

Subsistema de **imágenes e iconos** de Rivet. **Crate nueva** (no existía en v1).

- **Versión:** 2.0 (definitiva)
- **Fecha:** 2026-10-11
- **Capa:** 2.5 (soporte de imágenes sobre canvas)
- **Depende de:** `rivet-foundation`; consumido por `rivet-canvas` / `rivet-ui`.
- **Reemplaza a:** GdkPixbuf + partes de `librsvg` + manejo de temas de iconos de GTK.
- **Documento paraguas:** `../RIVET.md`

---

## 1. Propósito y justificación

Las apps semilla (panel, lanzador tipo rofi, AnimaEngine) necesitan **mostrar imágenes**: iconos de aplicaciones, miniaturas, sprites, avatares y SVG escalable. GTK obtiene esto de GdkPixbuf + `librsvg` + el tema de iconos. Rivet necesita su equivalente. Por eso se crea `rivet-image`, que decodifica formatos, resuelve **iconos por tema** y entrega datos listos para subir a textura.

## 2. Componentes

| Módulo | Responsabilidad |
|---|---|
| `decode` | Decodificar PNG / JPEG / WebP. |
| `svg` | Rasterizar SVG a un tamaño/`scale` dado. |
| `icon` | Resolver iconos por nombre desde temas `freedesktop` (con fallback). |
| `theme` | Cargar/parsear `index.theme` y buscar en las rutas del *hicolor theme*. |
| `transform` | Escalado (nearest/bilinear), rotado, recorte, composición alfa. |
| `cache` | Caché por (clave, tamaño, escala). |
| `error` | `ImageError`. |

## 3. Decodificación

- **PNG:** crate Rust puro (`png`).
- **JPEG:** `jpeg-decoder` (Rust puro) o `image` (Rust).
- **WebP:** `image`/`webp` (Rust).
- **SVG:** `resvg`/`usvg` (**Rust puro**) para rasterizar a mapa de bits; no se usa `librsvg` (C).
- **Salida:** RGBA8 premultiplicado, listo para textura de canvas.

## 4. Iconos y temas (equivalente a GdkPixbuf/icon theme)

- Resolver iconos por **nombre** siguiendo la especificación **`freedesktop` Icon Theme**:
  - Buscar en `$XDG_DATA_DIRS/icons`, `~/.local/share/icons`, `~/.icons`, `/usr/share/icons`.
  - Leer `index.theme` (hereda de `hicolor`), tamaños y directorios.
  - Buscar extensiones por prioridad: `.svg` (preferido, escalable) → `.png` → `.xpm`.
- **Fallback:** icono "missing"/genérico empaquetado si no se encuentra.
- Nombres simbólicos (`-symbolic`) con recoloreo por color de tema cuando aplica.

## 5. Integración con canvas

```rust
pub enum ImageData { Rgba { width, height, pixels: Vec<u8> }, /* ... */ }
pub struct IconTheme;
pub struct IconCache;

impl IconTheme {
    pub fn lookup(&self, name: &str, size: u32, scale: u32) -> Result<ImageData, ImageError>;
}
```

- `rivet-canvas` sube `ImageData` a una `Texture` (`glTexSubImage2D`).
- **No** dibuja por sí mismo; produce píxeles/recursos.

## 6. Rendimiento

- **Caché** por (ruta/nombre, tamaño, escala, color de tinte). Evita re-decodificar y re-rasterizar.
- Lazy/async: decodificación grande (miniaturas) puede diferirse y notificarse; v1 puede hacerlo síncrono y medir.
- SVG se rasteriza al tamaño solicitado (no se cachea un SVG gigante).

## 7. Dependencias

- **Rust puro:** `png`, `jpeg-decoder`/`image`, `resvg`/`usvg`, utilidades de color.
- **C de sistema:** ninguna.
- Sin GTK, sin GdkPixbuf, sin `librsvg`.

## 8. Alcance

**v1:** PNG/JPEG/WebP decode, SVG rasterize, icon theme `freedesktop` con fallback, escalado, caché.

**v1.x/v2:** formatos extra (AVIF, GIF animado), iconos XPM, recolor avanzado, escalado por GPU, decodificación asíncrona.

## 9. Decisiones y correcciones aplicadas

- **Crate nueva** justificada por las apps semilla (iconos + imágenes).
- Todo **Rust puro**; sin `librsvg`/`gdk-pixbuf`.
- Salida en **RGBA premultiplicado** coherente con canvas.

## 10. Métricas (objetivos, a medir — `../RIVET.md` §14)

ms de decodificación PNG/JPEG/WebP, ms de rasterizado SVG por tamaño, hit-ratio y RAM de caché, latencia de lookup de icono. Método: `criterion`.
