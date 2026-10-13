//! Transformaciones: escalado (nearest/bilinear), recorte, composición y
//! recoloreado. Todo sobre RGBA premultiplicado.

use crate::data::ImageData;

/// Filtro de escalado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// Vecino más cercano (pixel art, iconos pequeños).
    Nearest,
    /// Bilineal (fotos, iconos grandes).
    Bilinear,
}

/// Escala una imagen al tamaño exacto.
pub fn resize(src: &ImageData, width: u32, height: u32, filter: Filter) -> ImageData {
    assert!(width > 0 && height > 0, "resize: dimensiones vacías");
    let mut out = vec![0u8; (width as usize) * (height as usize) * 4];
    match filter {
        Filter::Nearest => {
            for y in 0..height {
                for x in 0..width {
                    let sx = (x as u64 * src.width as u64 / width as u64) as u32;
                    let sy = (y as u64 * src.height as u64 / height as u64) as u32;
                    copy_pixel(
                        &mut out,
                        width,
                        x,
                        y,
                        src,
                        sx.min(src.width - 1),
                        sy.min(src.height - 1),
                    );
                }
            }
        }
        Filter::Bilinear => {
            let rx = src.width as f32 / width as f32;
            let ry = src.height as f32 / height as f32;
            for y in 0..height {
                for x in 0..width {
                    // Coordenadas del centro del píxel destino en origen.
                    let fx = (x as f32 + 0.5) * rx - 0.5;
                    let fy = (y as f32 + 0.5) * ry - 0.5;
                    let x0 = fx.floor().max(0.0) as u32;
                    let y0 = fy.floor().max(0.0) as u32;
                    let x1 = (x0 + 1).min(src.width - 1);
                    let y1 = (y0 + 1).min(src.height - 1);
                    let tx = (fx - x0 as f32).clamp(0.0, 1.0);
                    let ty = (fy - y0 as f32).clamp(0.0, 1.0);
                    let p00 = pixel(src, x0, y0);
                    let p10 = pixel(src, x1, y0);
                    let p01 = pixel(src, x0, y1);
                    let p11 = pixel(src, x1, y1);
                    let top = lerp4(p00, p10, tx);
                    let bottom = lerp4(p01, p11, tx);
                    let mixed = lerp4(top, bottom, ty);
                    let i = ((y * width + x) * 4) as usize;
                    out[i..i + 4].copy_from_slice(&mixed);
                }
            }
        }
    }
    ImageData::from_premultiplied(width, height, out)
}

/// Recorta a una subregión (recortada a los límites de la imagen).
pub fn crop(src: &ImageData, x: u32, y: u32, width: u32, height: u32) -> ImageData {
    let x = x.min(src.width);
    let y = y.min(src.height);
    let width = width.min(src.width - x).max(1);
    let height = height.min(src.height - y).max(1);
    let mut out = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for row in 0..height {
        let start = (((y + row) * src.width + x) * 4) as usize;
        out.extend_from_slice(&src.pixels[start..start + (width as usize) * 4]);
    }
    ImageData::from_premultiplied(width, height, out)
}

/// Composición alfa (operator over): `above` encima de `below`.
///
/// Ambas deben tener el mismo tamaño; el resultado es premultiplicado.
pub fn composite_over(above: &ImageData, below: &ImageData) -> ImageData {
    assert_eq!(
        (above.width, above.height),
        (below.width, below.height),
        "composite_over: tamaños distintos"
    );
    let mut out = Vec::with_capacity(above.pixels.len());
    for (a, b) in above
        .pixels
        .chunks_exact(4)
        .zip(below.pixels.chunks_exact(4))
    {
        // Premultiplicado: out = a + b·(1 − aα)
        let inv = 255 - a[3];
        out.extend_from_slice(&[
            a[0].saturating_add((b[0] as u16 * inv as u16 / 255) as u8),
            a[1].saturating_add((b[1] as u16 * inv as u16 / 255) as u8),
            a[2].saturating_add((b[2] as u16 * inv as u16 / 255) as u8),
            a[3].saturating_add((b[3] as u16 * inv as u16 / 255) as u8),
        ]);
    }
    ImageData::from_premultiplied(above.width, above.height, out)
}

/// Recolorea iconos simbólicos: RGB ← tinte, alfa conservado.
///
/// `tint` en alfa recta; la salida se re-premultiplica.
pub fn recolor(src: &ImageData, tint: [u8; 4]) -> ImageData {
    let mut out = src.pixels.clone();
    for chunk in out.chunks_exact_mut(4) {
        let a = chunk[3];
        chunk[0] = (tint[0] as u16 * a as u16 / 255) as u8;
        chunk[1] = (tint[1] as u16 * a as u16 / 255) as u8;
        chunk[2] = (tint[2] as u16 * a as u16 / 255) as u8;
    }
    ImageData::from_premultiplied(src.width, src.height, out)
}

fn copy_pixel(out: &mut [u8], out_width: u32, x: u32, y: u32, src: &ImageData, sx: u32, sy: u32) {
    let dst = ((y * out_width + x) * 4) as usize;
    let s = ((sy * src.width + sx) * 4) as usize;
    out[dst..dst + 4].copy_from_slice(&src.pixels[s..s + 4]);
}

fn pixel(src: &ImageData, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * src.width + x) * 4) as usize;
    [
        src.pixels[i],
        src.pixels[i + 1],
        src.pixels[i + 2],
        src.pixels[i + 3],
    ]
}

fn lerp4(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3])]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient() -> ImageData {
        let mut px = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                px.extend_from_slice(&[x * 60, y * 60, 128, 255]);
            }
        }
        ImageData::from_premultiplied(4, 4, px)
    }

    #[test]
    fn nearest_conserva_pixeles() {
        let img = gradient();
        let out = resize(&img, 2, 2, Filter::Nearest);
        assert_eq!(out.size(), (2, 2));
        // Esquina: muestreo del píxel equivalente.
        assert_eq!(&out.pixels()[0..4], &img.pixels()[0..4]);
    }

    #[test]
    fn bilinear_interpola() {
        let img = ImageData::solid(2, 2, [0, 0, 0, 255]);
        let mut quad = img.clone();
        for chunk in quad.pixels.chunks_exact_mut(4).skip(2) {
            chunk[..3].copy_from_slice(&[255, 255, 255]);
        }
        // Mitad inferior blanca: al escalar 2x, la fila interpolada es gris.
        let out = resize(&quad, 4, 4, Filter::Bilinear);
        let mid = &out.pixels()[(2 * out.width as usize + 1) * 4..][..4];
        assert!(mid[0] > 0 && mid[0] < 255, "interpolación: {mid:?}");
    }

    #[test]
    fn crop_recorta() {
        let img = ImageData::solid(4, 4, [10, 20, 30, 255]);
        let out = crop(&img, 1, 1, 2, 2);
        assert_eq!(out.size(), (2, 2));
        assert_eq!(out.pixels().len(), 16);
        let beyond = crop(&img, 3, 3, 99, 99);
        assert_eq!(beyond.size(), (1, 1));
    }

    #[test]
    fn composite_over_opaco_e_transparente() {
        let base = ImageData::solid(2, 1, [255, 0, 0, 255]);
        let mut over = base.clone();
        for chunk in over.pixels.chunks_exact_mut(4).skip(1) {
            chunk.copy_from_slice(&[0, 0, 0, 0]); // píxel transparente
        }
        let out = composite_over(&over, &base);
        // El píxel transparente deja ver el fondo; el opaco se mantiene.
        assert_eq!(&out.pixels()[0..4], &[255, 0, 0, 255]);
        assert_eq!(&out.pixels()[4..8], &[255, 0, 0, 255]);
    }

    #[test]
    fn recolor_conserva_alfa() {
        let mut px = vec![10, 20, 30, 128, 0, 0, 0, 255];
        let img = ImageData::from_premultiplied(2, 1, std::mem::take(&mut px));
        let out = recolor(&img, [255, 255, 255, 255]);
        // Simbólico: RGB = tinte premultiplicado por alfa original.
        assert_eq!(&out.pixels()[0..4], &[128, 128, 128, 128]);
        assert_eq!(&out.pixels()[4..8], &[255, 255, 255, 255]);
    }

    #[test]
    fn premultiplicacion_desde_recta() {
        let img = ImageData::from_straight(1, 1, vec![255, 128, 0, 128]);
        assert_eq!(img.pixels(), &[128, 64, 0, 128]);
    }
}
