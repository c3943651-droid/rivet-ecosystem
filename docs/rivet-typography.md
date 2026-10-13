# rivet-typography — Documentación definitiva

Subsistema de **fuentes y texto** de Rivet.

- **Versión:** 2.0 (definitiva)
- **Fecha:** 2026-10-11
- **Capa:** 2.5 (soporte de texto sobre canvas)
- **Depende de:** `rivet-foundation`; es consumido por `rivet-canvas` / `rivet-ui`.
- **Reemplaza a:** Pango + FriBidi + HarfBuzz (parcialmente, ver §7).
- **Documento paraguas:** `../RIVET.md`

---

## 1. Propósito

Cargar fuentes, **rastrerizar glifos** y **componer/"shaping"** texto para que `rivet-canvas` lo dibuje como textura. Resuelve: descubrir fuentes, parsear TrueType/OpenType, mapear caracteres→glifos (`cmap`), obtener contornos, rasterizar a máscara/atlas, medir texto y cachear.

## 2. Componentes

| Módulo | Responsabilidad |
|---|---|
| `loader` | Cargar fuentes (archivo, memoria), validar, fallback empaquetado. |
| `parser` | Tablas TTF/OTF (`head`, `hhea`, `hmtx`, `cmap`, `glyf`, `CFF `, `kern`, `GPOS`...). |
| `cmap` | Carácter → ID de glifo (formatos 4/12...). |
| `outline` | Contornos (`glyf`/CFF) → polígonos. |
| `rasterizer` | Contornos → máscara (analítica o `stb`-like). |
| `metrics` | Avance, *advance width*, *kerning*, *ascent*/`descent`, *line height*. |
| `cache` | Caché LRU de glifos por (fuente, tamaño, subpíxel). |
| `texture` | Atlas de glifos → textura para canvas. |
| `font` | Objeto `Font` (fuente + tamaño + rasgos), fallback en cadena. |

## 3. Descubrimiento de fuentes (ADR-05)

- **Descubrimiento con `fontdb`** (crate **Rust puro**): escanea directorios estándar y fuentes embebidas del sistema.
- **NO se usa `fontconfig`** (C): se evita esa dependencia.
- **Fallback empaquetado:** se incluye al menos una fuente de respaldo en el binario para garantizar texto legible sin depender del sistema. Cadena de fallback configurable.
- **Cero C de GUI.**

## 4. Parseo y rasterizado

- Parseo de tablas con crate Rust (`ttf-parser` o equivalente Rust puro). 
- `cmap`: soportar formatos 4 y 12 (BMP y Unicode completo).
- Contornos `glyf` (TrueType) y `CFF` (OpenType PostScript).
- Rasterizado con **cobertura analítica** (antialias), caché de máscaras.
- **Limitación conocida:** **hinting** complejo de TrueType (`bytecode`) fuera del alcance inicial; se acepta menor nitidez a tamaños pequeños y se compensa con rasterizado subpíxel/PPEM alto.

## 5. Shaping y bidi (alcance)

- **v1:** shaping **simple** — avance + `kern` básico de tablas; suficiente para latino y muchos casos.
- **Casos complejos** (árabe, índico, ligaduras contextuales por `GSUB`/`GPOS` completo) y **bidi (RTL)** quedan **fuera de v1** (horizonte v1.x/v2, evaluando un motor de shaping Rust como `rustybuzz`).
- Se acepta que el rendering de scripts complejos en v1 no sea correcto; se documenta como limitación.

## 6. Caché y atlas

- **Caché LRU** con clave (font_id, pixel_size, subpixel_bin).
- **Atlas de glifos** con empaquetado en una o más texturas; subida incremental a GPU.
- **VRAM:** unos **KB por glifo** (p. ej. ~32px), no MB. Se mide el total del atlas.
- `FrameStats` de canvas expone glifos rasterizados/página.

## 7. API pública (orientativa)

```rust
pub struct FontDb;                       // descubrimiento y carga
pub struct Font;                         // fuente + tamaño + rasgos
pub struct TextLayout;                   // texto → glifos posicionados
pub struct Glyph { id, advance, bearing, atlas_rect }

impl FontDb { pub fn load_system(&mut self); pub fn load_file(&mut self, p: &Path) -> Result<FontId>; }
impl Font { pub fn layout(&self, text: &str) -> TextLayout; pub fn metrics(&self) -> FontMetrics; }
```

## 8. Dependencias

- **Rust puro:** `fontdb` (descubrimiento), `ttf-parser` (parseo), rasterizador propio o crate Rust; `unicode` para segmentación básica.
- **C de sistema:** ninguno (sin `fontconfig`, sin `freetype` en v1).
- Fuente de respaldo empaquetada (licencia compatible).

## 9. Alcance

**v1:** `fontdb`, TrueType/OpenType, `cmap` 4/12, `glyf`/`CFF`, rasterizado antialias, métricas + `kern`, shaping simple (LTR), caché LRU, atlas, fallback empaquetado.

**v1.x/v2:** shaping complejo y bidi (`rustybuzz`), hinting, fuentes variables, ligaduras contextuales, texto vertical.

## 10. Decisiones y correcciones aplicadas

- Fuentes con **`fontdb`** y **sin `fontconfig`** (ADR-05).
- Shaping complejo **explícitamente fuera** de v1 (no fingir soporte total).
- VRAM por glifo en **KB**.
- Sin dependencias C de GUI.

## 11. Métricas (objetivos, a medir — `../RIVET.md` §14)

Tiempo de carga/parseo de fuente, ms de rasterizado por glifo, hit-ratio de caché, RAM por fuente cargada, VRAM del atlas, calidad (subjetiva) a tamaños pequeños. Método: `criterion` + telemetría de atlas.
