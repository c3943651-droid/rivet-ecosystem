//! Decodificación de PNG / JPEG / WebP (vía crate `image`, Rust puro) y
//! detección de formato por número mágico.

use std::path::Path;

use image::GenericImageView;

use crate::data::ImageData;
use crate::error::{ImageError, Result};

/// Formato detectado por los bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// PNG (`.png`).
    Png,
    /// JPEG (`.jpg`/`.jpeg`).
    Jpeg,
    /// WebP (`.webp`).
    WebP,
    /// SVG vectorial (`.svg`); se rasteriza a un tamaño dado.
    Svg,
}

impl Format {
    /// Detecta el formato por *magic bytes*; `None` si desconocido.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            Some(Format::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8]) {
            Some(Format::Jpeg)
        } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(Format::WebP)
        } else if looks_like_svg(bytes) {
            Some(Format::Svg)
        } else {
            None
        }
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
    head.contains("<svg") || (head.contains("<?xml") && head.contains("svg"))
}

/// Decodifica bytes a [`ImageData`] RGBA premultiplicado.
///
/// SVG no se decodifica aquí: necesita un tamaño objetivo
/// ([`crate::svg::render`]).
pub fn decode(bytes: &[u8]) -> Result<ImageData> {
    match Format::sniff(bytes) {
        Some(Format::Svg) => Err(ImageError::UnsupportedFormat(
            "SVG requiere tamaño: usa svg::render".into(),
        )),
        Some(_) => decode_raster(bytes),
        None => Err(ImageError::Decode("formato no reconocido".into())),
    }
}

/// Decodifica un archivo de imagen (PNG/JPEG/WebP).
pub fn decode_file(path: impl AsRef<Path>) -> Result<ImageData> {
    let bytes = std::fs::read(path)?;
    decode(&bytes)
}

fn decode_raster(bytes: &[u8]) -> Result<ImageData> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ImageError::Decode(e.to_string()))?;
    let decoded = reader
        .decode()
        .map_err(|e| ImageError::Decode(e.to_string()))?;
    let (width, height) = decoded.dimensions();
    let rgba = decoded.to_rgba8();
    Ok(ImageData::from_straight(width, height, rgba.into_raw()))
}

/// Codifica un PNG en memoria (para tests y exportación de capturas).
pub fn encode_png(image: &ImageData) -> Result<Vec<u8>> {
    // PNG necesita alfa recta: des-premultiplicar.
    let straight = straighten(&image.pixels);
    let buffer = image::RgbaImage::from_raw(image.width, image.height, straight)
        .ok_or_else(|| ImageError::Decode("buffer de tamaño incorrecto".into()))?;
    let mut out = std::io::Cursor::new(Vec::new());
    buffer
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| ImageError::Decode(e.to_string()))?;
    Ok(out.into_inner())
}

fn straighten(premultiplied: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(premultiplied.len());
    for chunk in premultiplied.chunks_exact(4) {
        let a = chunk[3] as u32;
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else if a == 255 {
            out.extend_from_slice(chunk);
        } else {
            out.extend_from_slice(&[
                (chunk[0] as u32 * 255 / a).min(255) as u8,
                (chunk[1] as u32 * 255 / a).min(255) as u8,
                (chunk[2] as u32 * 255 / a).min(255) as u8,
                chunk[3],
            ]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_magic_bytes() {
        assert_eq!(
            Format::sniff(&[0x89, b'P', b'N', b'G', 0, 0]),
            Some(Format::Png)
        );
        assert_eq!(Format::sniff(&[0xFF, 0xD8, 0xFF]), Some(Format::Jpeg));
        assert_eq!(
            Format::sniff(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some(Format::WebP)
        );
        assert_eq!(Format::sniff(b"<svg xmlns=\"x\"></svg>"), Some(Format::Svg));
        assert_eq!(Format::sniff(b"basura suelta"), None);
    }

    #[test]
    fn png_roundtrip() {
        let image = ImageData::solid(3, 2, [200, 100, 50, 255]);
        let png = encode_png(&image).unwrap();
        assert_eq!(Format::sniff(&png), Some(Format::Png));
        let back = decode(&png).unwrap();
        assert_eq!(back.size(), (3, 2));
        assert_eq!(back.pixels()[0..4], [200, 100, 50, 255]);
    }
}
