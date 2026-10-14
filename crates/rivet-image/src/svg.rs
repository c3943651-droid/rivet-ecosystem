//! SVG rasterization with `resvg` (pure Rust; no `librsvg`, doc §3).
//!
//! The SVG is rasterized **at the requested size** (doc §6: a giant SVG is
//! not cached). tiny-skia output is already premultiplied RGBA.

use crate::data::ImageData;
use crate::error::{ImageError, Result};

// resvg re-exporta usvg/tiny-skia con versiones compatibles entre sí.
use resvg::tiny_skia;
use resvg::usvg;

/// Rasteriza un SVG a `width × height` píxeles (escala ajustada, sin deformar
/// salvo que el SVG lo pida: se estira al rectángulo pedido).
pub fn render(bytes: &[u8], width: u32, height: u32) -> Result<ImageData> {
    assert!(width > 0 && height > 0, "svg::render: dimensiones vacías");
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &options)
        .map_err(|e| ImageError::Decode(format!("SVG: {e}")))?;
    render_tree(&tree, width, height)
}

/// Rasteriza un árbol USVG ya parseado (evita re-parseo en cachés).
pub fn render_tree(tree: &usvg::Tree, width: u32, height: u32) -> Result<ImageData> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| ImageError::Decode("pixmap inválida".into()))?;
    // Escala del tamaño natural del SVG al pedido.
    let (sw, sh) = (tree.size().width().max(1.0), tree.size().height().max(1.0));
    let transform = tiny_skia::Transform::from_scale(width as f32 / sw, height as f32 / sh);
    resvg::render(tree, transform, &mut pixmap.as_mut());
    // Pixmap de tiny-skia: RGBA premultiplicado, exactamente nuestro formato.
    Ok(ImageData::from_premultiplied(width, height, pixmap.take()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED_SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
        <rect width="10" height="10" fill="#ff0000"/>
    </svg>"##;

    #[test]
    fn renderiza_cuadrado_rojo() {
        let image = render(RED_SQUARE.as_bytes(), 20, 20).unwrap();
        assert_eq!(image.size(), (20, 20));
        // Centro rojo opaco (premultiplicado = recto con alfa 255).
        let c = &image.pixels()[(10 * 20 + 10) * 4..][..4];
        assert_eq!(c, &[255, 0, 0, 255]);
    }

    #[test]
    fn respeta_transparencia() {
        let image = render(RED_SQUARE.as_bytes(), 10, 10).unwrap();
        // Sin fondo: todo el SVG es el rect rojo; borde sigue siendo rojo.
        assert_eq!(&image.pixels()[0..4], &[255, 0, 0, 255]);
        let empty = render(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"></svg>"##.as_bytes(),
            8,
            8,
        )
        .unwrap();
        assert_eq!(empty.pixels()[0..4], [0, 0, 0, 0]);
    }
}
