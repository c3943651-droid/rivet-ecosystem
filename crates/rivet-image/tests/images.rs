//! Integration tests: real decoders (PNG/JPEG/WebP), SVG, system icon
//! themes, and the *missing* fallback.

use std::io::Cursor;

use rivet_image::{
    composite_over, decode, encode_png, missing_icon, render_svg, IconTheme, ImageCache, ImageData,
};

fn red_png() -> Vec<u8> {
    // 4×4 rojo opaco con esquina transparente (ejerce premultiplicación).
    let mut px = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            let alpha = if x == 0 && y == 0 { 0 } else { 255 };
            px.extend_from_slice(&[255, 0, 0, alpha]);
        }
    }
    encode_png(&ImageData::from_straight(4, 4, px)).unwrap()
}

#[test]
fn jpeg_decodifica() {
    // Genera un JPEG pequeño con el encoder de la crate image.
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba([40, 90, 200, 255]));
    let mut jpg = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut jpg, image::ImageFormat::Jpeg)
        .unwrap();
    let image = decode(&jpg.into_inner()).unwrap();
    assert_eq!(image.size(), (16, 16));
    // JPEG es con pérdida: tolerancia amplia.
    let c = &image.pixels()[(8 * 16 + 8) * 4..][..4];
    assert!(c[2] > 150, "azul dominante: {c:?}");
    assert_eq!(c[3], 255);
}

#[test]
fn webp_decodifica() {
    let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 200, 60, 255]));
    let mut webp = Cursor::new(Vec::new());
    // Encoder lossless de image-webp (disponible en image 0.25).
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut webp, image::ImageFormat::WebP)
        .unwrap();
    let image = decode(&webp.into_inner()).unwrap();
    assert_eq!(image.size(), (8, 8));
    let c = &image.pixels()[(4 * 8 + 4) * 4..][..4];
    assert_eq!(c, &[10, 200, 60, 255], "WebP lossless es exacto");
}

#[test]
fn png_con_alfa_premultiplica() {
    let image = decode(&red_png()).unwrap();
    assert_eq!(image.size(), (4, 4));
    assert_eq!(&image.pixels()[0..4], &[0, 0, 0, 0], "esquina transparente");
    assert_eq!(&image.pixels()[4..8], &[255, 0, 0, 255]);
}

#[test]
fn svg_rasteriza_al_tamano_pedido() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8">
        <rect width="8" height="8" fill="#00ff00"/>
    </svg>"##;
    let small = render_svg(svg, 8, 8).unwrap();
    let big = render_svg(svg, 32, 32).unwrap();
    assert_eq!(small.size(), (8, 8));
    assert_eq!(big.size(), (32, 32));
    assert_eq!(&big.pixels()[(16 * 32 + 16) * 4..][..4], &[0, 255, 0, 255]);
}

#[test]
fn icono_missing_es_determinista() {
    let a = missing_icon(24, 1);
    let b = missing_icon(24, 1);
    assert_eq!(a, b);
    assert_eq!(a.size(), (24, 24));
    assert!(a.pixels().contains(&255), "no todo transparente");
    let hidpi = missing_icon(24, 2);
    assert_eq!(hidpi.size(), (48, 48));
}

#[test]
fn iconos_del_sistema_o_missing() {
    // KDE/GNOME/CI tienen hicolor con iconos estándar; si no, cae al missing.
    let theme = IconTheme::with_theme("breeze");
    let icon = theme.lookup("nonexistent-icon-xyz-12345", 32, 1).unwrap();
    assert_eq!(icon.size(), (32, 32));

    // Iconos reales del sistema si existen (gated por disponibilidad).
    for name in ["edit-delete", "preferences-system", "utilities-terminal"] {
        let icon = theme.lookup(name, 32, 1).unwrap();
        assert_eq!(icon.size().0.max(1), icon.size().0);
        let _ = name;
    }

    // Simbólico con recolor: azul de tema.
    let symbolic = theme
        .lookup_symbolic("edit-delete", 16, 1, [80, 120, 255, 255])
        .unwrap();
    // Si resolvió un simbólico real, todo el RGB debe ser el tinte (o el
    // missing gris); verificamos que exista píxeles no transparentes.
    assert!(symbolic.pixels().iter().any(|&c| c > 0));
}

#[test]
fn cache_de_iconos() {
    let mut cache = ImageCache::new(4);
    let key = rivet_image::CacheKey::new("edit-delete", 32, 1);
    let first = cache.get_or_insert_with(key.clone(), missing_icon_binder);
    let second = cache.get_or_insert_with(key, missing_icon_binder);
    assert_eq!(first, second);
    assert_eq!(cache.hits(), 1, "segunda vez desde caché");
}

fn missing_icon_binder() -> ImageData {
    missing_icon(32, 1)
}

#[test]
fn composicion_y_escalado_encadenados() {
    let base = ImageData::solid(8, 8, [10, 10, 10, 255]);
    let small = missing_icon(4, 1).resize(8, 8, rivet_image::Filter::Nearest);
    let over = composite_over(&small, &base);
    assert_eq!(over.size(), (8, 8));
    // El missing tiene cuerpo claro: tras componer no es el fondo puro.
    let c = &over.pixels()[(4 * 8 + 4) * 4..][..4];
    assert!(c[0] > 10, "composición aplicó contenido: {c:?}");
}
