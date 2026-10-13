//! Icono *missing* generado proceduralmente (fallback empaquetado, doc §4).
//!
//! No requiere asset: se dibuja un documento con borde y una X roja sobre
//! fondo transparente, escalable a cualquier tamaño.

use crate::data::ImageData;

/// Dibuja el icono *missing* a `size × scale` píxeles.
#[must_use]
pub fn missing_icon(size: u32, scale: u32) -> ImageData {
    let dim = size.max(1) * scale.max(1);
    let mut pixels = Vec::with_capacity((dim * dim * 4) as usize);
    let d = dim as f32;
    // Geometría relativa del documento.
    let (left, right) = (d * 0.22, d * 0.78);
    let (top, bottom) = (d * 0.08, d * 0.92);
    let stroke = (d / 16.0).max(1.0);
    // Diagonales de la X.
    let (x0, x1) = (d * 0.34, d * 0.66);
    let (y0, y1) = (d * 0.34, d * 0.66);
    for y in 0..dim {
        for x in 0..dim {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let inside_doc = (left..=right).contains(&fx) && (top..=bottom).contains(&fy);
            let border = inside_doc
                && (fx < left + stroke
                    || fx > right - stroke
                    || fy < top + stroke
                    || fy > bottom - stroke);
            // Distancia a cada diagonal de la X (dentro de su caja).
            let slope = (y1 - y0) / (x1 - x0);
            let dist_a = (fy - y0 - (fx - x0) * slope).abs();
            let dist_b = (fy - y1 + (fx - x0) * slope).abs();
            let cross = (x0..=x1).contains(&fx)
                && (y0..=y1).contains(&fy)
                && (dist_a <= stroke || dist_b <= stroke);
            let mut px = [0, 0, 0, 0];
            if cross {
                px = [214, 38, 38, 255]; // X roja opaca
            } else if border {
                px = [90, 90, 96, 255]; // borde del documento
            } else if inside_doc {
                px = [235, 235, 238, 255]; // cuerpo del documento
            }
            pixels.extend_from_slice(&px);
        }
    }
    ImageData::from_premultiplied(dim, dim, pixels)
}
