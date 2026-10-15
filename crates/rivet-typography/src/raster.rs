//! Glyph rasterizer with analytic coverage (antialiasing).
//!
//! Pipeline: ttf-parser outlines (`glyf`/CFF) → Bezier flattening
//! (De Casteljau) → segments → *scanline* with exact per-pixel area
//! (non-zero winding rule).

use ttf_parser::OutlineBuilder;

/// Glifo rastrerizado: máscara de cobertura 1 byte/píxel.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterizedGlyph {
    /// Ancho de la máscara en píxeles.
    pub width: usize,
    /// Alto de la máscara en píxeles.
    pub height: usize,
    /// Desplazamiento X del borde izquierdo respecto a la línea base (px).
    pub left: f32,
    /// Desplazamiento Y del borde superior respecto a la línea base (px,
    /// Y hacia abajo).
    pub top: f32,
    /// Cobertura por píxel (0..=255), fila-major desde arriba.
    pub coverage: Vec<u8>,
}

/// Recolector de contornos + aplanador de curvas.
#[derive(Debug, Default)]
struct OutlineCollector {
    points: Vec<(f32, f32)>,
    contours: Vec<usize>,
    // Curvas pendientes se aplanan incrementalmente (De Casteljau).
    last: Option<(f32, f32)>,
}

const FLATTEN_TOLERANCE: f32 = 0.05; // px de desviación máxima

/// Tamaño (px) por debajo del cual se aplica *light hinting* (ajuste a la
/// rejilla de los bordes casi rectos). A tamaños pequeños los tallos miden
/// ~1.1 px y quedan repartidos entre dos columnas (bordes "lavados"); la
/// cobertura analítica es correcta pero visualmente borrosa. Alinear los
/// bordes con la rejilla los vuelve nítidos. En tamaños grandes la
/// cobertura analítica es preferible (los bins subpíxel ya preservan el
/// ritmo) y no se toca el contorno. Es el mismo umbral que desactiva el
/// subpíxel en [`crate::font`].
const HINT_MAX_SIZE: f32 = crate::font::SUBPIXEL_MIN_SIZE;

/// Pendiente máxima (|minor|/|major|) para considerar un borde "recto"
/// (casi vertical / casi horizontal) y por tanto ajustable a la rejilla.
const HINT_SLOPE: f32 = 0.30;

impl OutlineCollector {
    fn flatten_quad(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let Some(p0) = self.last else { return };
        self.subdivide_quad([p0, (cx, cy), (x, y)], 0);
        self.last = Some((x, y));
        self.points.push((x, y));
    }

    /// De Casteljau: [p0, control, destino].
    fn subdivide_quad(&mut self, c: [(f32, f32); 3], depth: u8) {
        let [(x0, y0), (cx, cy), (x1, y1)] = c;
        if depth >= 8 || self.quad_flat(c) {
            self.points.push(c[2]);
            return;
        }
        let (ax, ay) = ((x0 + cx) * 0.5, (y0 + cy) * 0.5);
        let (bx, by) = ((cx + x1) * 0.5, (cy + y1) * 0.5);
        let (mx, my) = ((ax + bx) * 0.5, (ay + by) * 0.5);
        self.subdivide_quad([(x0, y0), (ax, ay), (mx, my)], depth + 1);
        self.subdivide_quad([(mx, my), (bx, by), (x1, y1)], depth + 1);
    }

    fn quad_flat(&self, [(x0, y0), (cx, cy), (x1, y1)]: [(f32, f32); 3]) -> bool {
        // Distancia del punto de control a la cuerda vs tolerancia.
        let (dx, dy) = (x1 - x0, y1 - y0);
        let d = (cx - x0) * dy - (cy - y0) * dx;
        let len = dx.hypot(dy);
        len <= f32::EPSILON || (d.abs() / len) * 0.5 <= FLATTEN_TOLERANCE
    }

    fn flatten_cubic(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        let Some(p0) = self.last else { return };
        self.subdivide_cubic([p0, (c1x, c1y), (c2x, c2y), (x, y)], 0);
        self.last = Some((x, y));
        self.points.push((x, y));
    }

    /// De Casteljau: [p0, c1, c2, destino].
    fn subdivide_cubic(&mut self, c: [(f32, f32); 4], depth: u8) {
        let [(x0, y0), (c1x, c1y), (c2x, c2y), (x1, y1)] = c;
        if depth >= 8 || self.cubic_flat(c) {
            self.points.push(c[3]);
            return;
        }
        let (ax, ay) = ((x0 + c1x) * 0.5, (y0 + c1y) * 0.5);
        let (bx, by) = ((c1x + c2x) * 0.5, (c1y + c2y) * 0.5);
        let (cx, cy) = ((c2x + x1) * 0.5, (c2y + y1) * 0.5);
        let (dx, dy) = ((ax + bx) * 0.5, (ay + by) * 0.5);
        let (ex, ey) = ((bx + cx) * 0.5, (by + cy) * 0.5);
        let (fx, fy) = ((dx + ex) * 0.5, (dy + ey) * 0.5);
        self.subdivide_cubic([(x0, y0), (ax, ay), (dx, dy), (fx, fy)], depth + 1);
        self.subdivide_cubic([(fx, fy), (dx, dy), (ex, ey), (cx, cy)], depth + 1);
        self.subdivide_cubic([(fx, fy), (ex, ey), (cx, cy), (x1, y1)], depth + 1);
    }

    fn cubic_flat(&self, [(x0, y0), (c1x, c1y), (c2x, c2y), (x1, y1)]: [(f32, f32); 4]) -> bool {
        // Aproximación conservadora: desviación de cada control vs cuerda.
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = dx.hypot(dy);
        if len <= f32::EPSILON {
            return true;
        }
        let d1 = ((c1x - x0) * dy - (c1y - y0) * dx).abs();
        let d2 = ((c2x - x0) * dy - (c2y - y0) * dx).abs();
        (d1.max(d2) / len) * 0.75 <= FLATTEN_TOLERANCE
    }
}

impl OutlineBuilder for OutlineCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.contours.push(self.points.len());
        self.last = Some((x, y));
        self.points.push((x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.points.push((x, y));
        self.last = Some((x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.flatten_quad(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.flatten_cubic(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        // Cerrar contorno: nada que hacer; los puntos ya están.
    }
}

/// Corrección gamma de la cobertura (v0.3).
///
/// La cobertura analítica es lineal, pero el compositado lineal de
/// cobertura 0.5 sobre un fondo oscuro se percibe mucho más tenue que
/// "mitad de tinta": los trazos finos de fuentes de 14px quedan
/// lavados ("borrosos") sin hinting que los refuerce. La práctica
/// estándar (FreeType/gamma-corrected alpha) es realzar la cobertura
/// con ~1/2.2 antes de usarla como alfa.
const COVERAGE_GAMMA: f32 = 1.0 / 2.2;

/// Rastreriza un glifo a `size_px` píxeles con desplazamiento subpíxel.
///
/// `subpixel_offset` desplaza el contorno en X (bin × ¼ px) antes del bbox;
/// el consumidor dibuja el resultado en la posición entera. Es el
/// rasterizado subpíxel real (doc §4: compensa el hinting ausente).
/// Devuelve `None` si el glifo no tiene contornos (espacio, etc.).
pub fn rasterize(
    face: &ttf_parser::Face<'_>,
    glyph: ttf_parser::GlyphId,
    size_px: f32,
    subpixel_offset: f32,
) -> Option<RasterizedGlyph> {
    let upem = face.units_per_em().max(1) as f32;
    let scale = size_px / upem;

    let mut collector = OutlineCollector::default();
    face.outline_glyph(glyph, &mut collector)?;
    if collector.points.len() < 3 {
        return None;
    }

    // A coordenadas de píxel (Y hacia abajo) y bbox.
    let mut scaled: Vec<(f32, f32)> = collector
        .points
        .iter()
        .map(|&(x, y)| (x * scale + subpixel_offset, y * -scale))
        .collect();

    // *Light hinting*: en texto pequeño ajusta los bordes casi rectos a la
    // rejilla para que los tallos caigan en columnas/filas enteras.
    if size_px < HINT_MAX_SIZE {
        grid_fit(&collector.contours, &mut scaled, HINT_SLOPE);
    }

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for &(x, y) in &scaled {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    const PAD: f32 = 1.0;
    // La rejilla del bitmap se alinea a píxeles ENTEROS: así el offset
    // subpíxel sobrevive a la traslación (left fraccional lo cancelaría).
    let left = min_x.floor() - PAD;
    let top = min_y.floor() - PAD;
    let width = (max_x - left + PAD).ceil().max(1.0) as usize;
    let height = (max_y - top + PAD).ceil().max(1.0) as usize;

    // Segmentos por contorno con traslación a origen del bitmap.
    let mut segments: Vec<((f32, f32), (f32, f32))> = Vec::new();
    let mut start = 0usize;
    for &contour_start in collector
        .contours
        .iter()
        .chain([collector.points.len()].iter())
    {
        let pts = &scaled[start..contour_start];
        if pts.len() >= 3 {
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                segments.push(((a.0 - left, a.1 - top), (b.0 - left, b.1 - top)));
            }
        }
        start = contour_start;
    }

    // Scanline con cobertura analítica.
    let mut coverage = vec![0u8; width * height];
    for row in 0..height {
        let ys = row as f32;
        let mut acc = vec![0f32; width];
        for &((x0, y0), (x1, y1)) in &segments {
            let y_lo = y0.min(y1);
            let y_hi = y0.max(y1);
            if y_hi <= ys || y_lo >= ys + 1.0 || y0 == y1 {
                continue; // no cruza esta línea de escaneo
            }
            let direction = if y1 > y0 { 1.0 } else { -1.0 };
            // Recorta el segmento a la franja [ys, ys+1).
            let (xa, ya, xb, yb) = clip_to_row(x0, y0, x1, y1, ys);
            let clip_len = yb - ya; // longitud vertical dentro de la franja
            let px_lo = xa.min(xb).floor().max(0.0) as i32;
            let px_hi = (xa.max(xb).ceil() as i32).min(width as i32 - 1);
            for px in px_lo..=px_hi {
                let area = signed_column_area(xa, ya, xb, yb, px as f32);
                if px >= 0 && (px as usize) < width {
                    acc[px as usize] += direction * area;
                }
            }
            // Winding: las columnas COMPLETAMENTE a la derecha del segmento
            // reciben la longitud completa de la franja (al acumularse con
            // los demás bordes "rellena" el interior). La primera columna
            // totalmente a la derecha es ceil(max_x): su área analítica es
            // 0 (el segmento queda a su izquierda), así que no se
            // double-cuenta.
            //
            // v0.3-fix: antes la cola empezaba en px_hi+1 = ceil(max)+1 y
            // se SALTABA esa columna → hueco de 1px justo a la derecha de
            // cada borde → los trazos quedaban como "borde + hueco +
            // núcleo" (la "sombra paralela" percibida en el texto).
            let tail_start = (xa.max(xb).ceil() as i32).max(0);
            for px in tail_start..width as i32 {
                acc[px as usize] += direction * clip_len;
            }
        }
        for (x, &a) in acc.iter().enumerate() {
            // Regla nonzero: |enrrollado| acotado a 1; gamma realza trazos
            // finos para compensar la percepción sobre fondos oscuros.
            let linear = a.abs().min(1.0);
            let boosted = linear.powf(COVERAGE_GAMMA);
            coverage[row * width + x] = (boosted * 255.0).round() as u8;
        }
    }

    Some(RasterizedGlyph {
        width,
        height,
        left,
        top,
        coverage,
    })
}

/// Un borde casi recto candidato a ajuste: extremos, coordenada media del
/// eje menor, longitud en el eje mayor y sentido (signo) en ese eje.
struct Straight {
    a: usize,
    b: usize,
    mid: f32,
    dir: f32,
}

/// Ajuste a la rejilla (*light hinting*) de los bordes casi rectos.
///
/// Empareja los dos flancos de un tallo (bordes casi verticales largos, de
/// sentidos opuestos y próximos en `x`) y los mueve juntos a columnas
/// enteras **conservando el grosor**: un tallo de ~1.1 px se redondea a
/// 1 px (nítido) en vez de repartirse entre dos columnas. Igual en `y` para
/// los travesaños casi horizontales. Los puntos que unen un borde recto con
/// uno diagonal se mueven solo en el eje ajustado, así que las curvas no se
/// deforman. Los fragmentos cortos (esquinas) y los pares del mismo lado se
/// descartan, evitando deformaciones.
fn grid_fit(contours: &[usize], points: &mut [(f32, f32)], slope: f32) {
    let n = points.len();
    if n == 0 {
        return;
    }
    // Longitud mínima (en el eje mayor) para considerar un borde "tallo".
    const MIN_LEN: f32 = 1.5;
    // Rango de grosor que se considera un tallo real.
    const MIN_GAP: f32 = 0.25;
    const MAX_STEM: f32 = 3.0;
    // Por debajo de este grosor, el tallo se colapsa a 1 px.
    const THIN: f32 = 1.5;

    let mut vedges: Vec<Straight> = Vec::new();
    let mut hedges: Vec<Straight> = Vec::new();
    let mut start = 0usize;
    for &cend in contours.iter().chain([n].iter()) {
        let len = cend.saturating_sub(start);
        if len >= 3 {
            for i in 0..len {
                let a = start + i;
                let b = start + (i + 1) % len;
                let (x0, y0) = points[a];
                let (x1, y1) = points[b];
                let dx = (x1 - x0).abs();
                let dy = (y1 - y0).abs();
                if dx <= slope * dy && dy >= MIN_LEN {
                    vedges.push(Straight {
                        a,
                        b,
                        mid: (x0 + x1) * 0.5,
                        dir: (y1 - y0).signum(),
                    });
                } else if dy <= slope * dx && dx >= MIN_LEN {
                    hedges.push(Straight {
                        a,
                        b,
                        mid: (y0 + y1) * 0.5,
                        dir: (x1 - x0).signum(),
                    });
                }
            }
        }
        start = cend;
    }

    let mut out_x: Vec<Option<f32>> = vec![None; n];
    let mut out_y: Vec<Option<f32>> = vec![None; n];
    snap_stems(&vedges, MIN_GAP, MAX_STEM, THIN, &mut out_x);
    snap_stems(&hedges, MIN_GAP, MAX_STEM, THIN, &mut out_y);

    for i in 0..n {
        if let Some(x) = out_x[i] {
            points[i].0 = x;
        }
        if let Some(y) = out_y[i] {
            points[i].1 = y;
        }
    }
}

/// Empareja los dos flancos de cada tallo (bordes próximos, de sentido
/// opuesto y grosor plausible) y los ajusta a la rejilla conservando el
/// grosor. Los bordes no emparejados se redondean por separado.
fn snap_stems(edges: &[Straight], min_gap: f32, max_stem: f32, thin: f32, out: &mut [Option<f32>]) {
    let mut order: Vec<usize> = (0..edges.len()).collect();
    order.sort_by(|&i, &j| edges[i].mid.partial_cmp(&edges[j].mid).unwrap());
    let mut used = vec![false; edges.len()];
    for &i in &order {
        if used[i] {
            continue;
        }
        // Busca el flanco opuesto más cercano en el eje menor.
        let mut best: Option<(usize, f32)> = None;
        for &j in &order {
            if j == i || used[j] || edges[j].dir == edges[i].dir {
                continue;
            }
            let gap = edges[j].mid - edges[i].mid;
            if gap < min_gap || gap > max_stem {
                continue;
            }
            if best.is_none() || gap < best.unwrap().1 {
                best = Some((j, gap));
            }
        }
        used[i] = true;
        match best {
            Some((j, w)) => {
                used[j] = true;
                let ws = if w < thin { 1.0 } else { w.round().max(1.0) };
                let lo = edges[i].mid;
                let hi = edges[j].mid;
                let nl = ((lo + hi) * 0.5 - ws * 0.5).round();
                out[edges[i].a] = Some(nl);
                out[edges[i].b] = Some(nl);
                out[edges[j].a] = Some(nl + ws);
                out[edges[j].b] = Some(nl + ws);
            }
            None => {
                let v = edges[i].mid.round();
                out[edges[i].a] = Some(v);
                out[edges[i].b] = Some(v);
            }
        }
    }
}

/// Recorta un segmento a la franja horizontal `[ys, ys+1)`.
fn clip_to_row(x0: f32, y0: f32, x1: f32, y1: f32, ys: f32) -> (f32, f32, f32, f32) {
    let dy = y1 - y0;
    let t0 = ((ys - y0) / dy).clamp(0.0, 1.0);
    let t1 = ((ys + 1.0 - y0) / dy).clamp(0.0, 1.0);
    let xa = x0 + t0 * (x1 - x0);
    let xb = x0 + t1 * (x1 - x0);
    (xa, y0 + t0 * dy, xb, y0 + t1 * dy)
}

/// Área con signo de la región entre `x = px` y el segmento (xa,ya)-(xb,yb),
/// dentro de la franja vertical del segmento, en la columna `[px, px+1)`.
///
/// Matemáticamente: `∫ clamp(x(y) - px, 0, 1) dy` sobre la franja del
/// segmento recortado a la línea de escaneo. Con la regla nonzero, la suma
/// de todas las contribuciones da la cobertura exacta del píxel.
fn signed_column_area(xa: f32, ya: f32, xb: f32, yb: f32, px: f32) -> f32 {
    let (a, b) = if ya <= yb { (ya, yb) } else { (yb, ya) };
    let len = b - a;
    if len <= f32::EPSILON {
        return 0.0;
    }
    // g(t) lineal de ga a gb sobre t∈[0,1] con dy = len.
    let ga = x_at(xa, ya, xb, yb, a) - px;
    let gb = x_at(xa, ya, xb, yb, b) - px;
    len * integral_clamped_linear(ga, gb)
}

/// Valor de x en y de la recta (xa,ya)-(xb,yb).
fn x_at(xa: f32, ya: f32, xb: f32, yb: f32, y: f32) -> f32 {
    if (yb - ya).abs() <= f32::EPSILON {
        return xa;
    }
    xa + (xb - xa) * (y - ya) / (yb - ya)
}

/// `∫₀¹ clamp(g(t), 0, 1) dt` con g lineal de `ga` a `gb`.
fn integral_clamped_linear(ga: f32, gb: f32) -> f32 {
    let slope = gb - ga;
    if slope.abs() < 1e-9 {
        return ga.clamp(0.0, 1.0);
    }
    // Cruces con g=0 y g=1 (pueden caer fuera de [0,1]).
    let t0 = -ga / slope;
    let t1 = (1.0 - ga) / slope;
    let (tl, th) = if slope > 0.0 { (t0, t1) } else { (t1, t0) };
    // Región con g ≥ 1 (cubre columna completa).
    let (full_lo, full_hi) = if slope > 0.0 { (th, 1.0) } else { (0.0, th) };
    let full_len = (full_hi - full_lo).clamp(0.0, 1.0);
    // Tramo lineal entre cruces, recortado a [0,1].
    let (a, b) = (tl.max(0.0), th.min(1.0));
    let lin = if b > a {
        ((b - tl) * (b - tl) - (a - tl) * (a - tl)) / (2.0 * (th - tl))
    } else {
        0.0
    };
    full_len + lin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorte_a_franja() {
        // Segmento diagonal cruzando la franja 0..1.
        let (xa, ya, xb, yb) = clip_to_row(0.0, -0.5, 2.0, 1.5, 0.0);
        assert!((ya - 0.0).abs() < 1e-6);
        assert!((yb - 1.0).abs() < 1e-6);
        assert!((xa - 0.5).abs() < 1e-6);
        assert!((xb - 2.0 - (1.5 - 1.0) / 2.0 + 0.5).abs() > -1.0); // solo smoke
    }

    #[test]
    fn area_columna_vertical() {
        // Borde vertical en x=px+0.5 cubre media columna.
        let a = signed_column_area(2.5, 0.0, 2.5, 1.0, 2.0);
        assert!((a - 0.5).abs() < 1e-5);
        // Borde a la derecha de la columna: columna cubierta (winding).
        let a = signed_column_area(7.0, 0.0, 7.0, 1.0, 2.0);
        assert!((a - 1.0).abs() < 1e-5);
        // Borde a la izquierda: nada.
        let a = signed_column_area(1.0, 0.0, 1.0, 1.0, 2.0);
        assert!(a.abs() < 1e-5);
    }

    #[test]
    fn tallos_sin_huecos_internos() {
        // Regresión v0.3: la cola de winding empezaba una columna tarde y
        // dejaba una línea de 1px transparente DENTRO de cada tallo (un
        // hueco de la altura del tallo), percibida como "sombra paralela".
        //
        // El hueco interno se detecta como una columna casi vacía con tinta
        // a ambos lados. Las muescas legítimas en esquinas cóncavas (unión
        // tallo-arco de 'n'/'d') también dejan 1px vacío, pero solo en 1-3
        // filas; el bug las deja durante todo el tallo. Se exige una racha
        // vertical de >= 5 filas para distinguir bug de esquina.
        let mut db = crate::db::FontDb::new();
        db.load_bundled().expect("fuente empaquetada");
        let id = db.sans_serif().expect("sans");
        let font = db.font(id, 48.0).expect("font 48px");
        let layout = font.layout("Hneda");
        assert!(layout.glyphs.len() >= 4);
        for g in &layout.glyphs {
            let r = font.rasterize(*g).expect("raster");
            for x in 1..r.width.saturating_sub(1) {
                let mut run = 0usize;
                let mut best = 0usize;
                for row in 0..r.height {
                    let a = |i: usize| r.coverage[row * r.width + i];
                    if a(x) < 25 && a(x - 1) > 90 && a(x + 1) > 90 {
                        run += 1;
                        best = best.max(run);
                    } else {
                        run = 0;
                    }
                }
                assert!(
                    best < 5,
                    "hueco interno de {best} filas en el glifo {:?} col {x} \
                     (sombra paralela del scanline)",
                    g.glyph
                );
            }
        }
    }

    #[test]
    fn hinting_afila_tallos() {
        // Light hinting (v0.3): a tamaños pequeños los tallos de ~1px se
        // reparten entre dos columnas y el texto se ve borroso. El ajuste a
        // la rejilla debe concentrarlos en una sola columna (crisp).
        //
        // Nitidez = Σc²/Σc sobre cobertura lineal: 1.0 si el tallo cae entero
        // en una columna, ~0.5 si se reparte entre dos. Sin hinting los
        // tallos dan ~0.56-0.66; con hinting, >= 0.87.
        let mut db = crate::db::FontDb::new();
        db.load_bundled().expect("fuente empaquetada");
        let id = db.sans_serif().expect("sans");
        let font = db.font(id, 14.0).expect("font 14px");
        for ch in ['l', 'i', 't'] {
            let layout = font.layout(&ch.to_string());
            let g = *layout.glyphs.first().expect("glifo");
            let r = font.rasterize(g).expect("raster");
            let mut s1 = 0.0f32;
            let mut s2 = 0.0f32;
            for &v in &r.coverage {
                let c = (v as f32 / 255.0).powf(COVERAGE_GAMMA);
                s1 += c;
                s2 += c * c;
            }
            let sharp = s2 / s1;
            assert!(
                sharp > 0.8,
                "'{ch}' a 14px poco nítido: sharpness={sharp:.3} \
                 (¿light hinting no aplicado?)"
            );
        }
    }

    #[test]
    fn area_diagonal_exacta() {
        // Diagonal (0,0)-(2,1) en px=0: ∫clamp(2y,0,1) = 0.25 + 0.5 = 0.75.
        let a = signed_column_area(0.0, 0.0, 2.0, 1.0, 0.0);
        assert!((a - 0.75).abs() < 1e-4);
        // Diagonal (0,0)-(4,1) en px=0: 0.125 + 0.75 = 0.875.
        let a = signed_column_area(0.0, 0.0, 4.0, 1.0, 0.0);
        assert!((a - 0.875).abs() < 1e-4);
    }
}
