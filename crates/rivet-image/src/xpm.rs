//! XPM decoder (X Pixmap, a text format from the 80s).
//!
//! Hand-written parser: C comment stripping, `{ … }` array, header
//! `<width> <height> <colors> <cpp>`, color entries (`c #hex`, `c None`,
//! `c <name>`) and pixel rows by code of `cpp` characters.
//! v1 supports hex colors (`#RGB`/`#RRGGBB`/...), `None` and 16 basic names.

use crate::data::ImageData;
use crate::error::{ImageError, Result};

/// Decodifica un XPM completo.
pub fn decode_xpm(bytes: &[u8]) -> Result<ImageData> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| ImageError::Decode(format!("XPM no es UTF-8: {e}")))?;
    let lines = split_lines(text)?;
    parse_lines(&lines)
}

/// Divide el array XPM en "líneas" (cadenas separadas por comas).
fn split_lines(text: &str) -> Result<Vec<String>> {
    // Quita comentarios /* … */.
    let mut cleaned = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        cleaned.push_str(&rest[..start]);
        let Some(end_rel) = rest[start + 2..].find("*/") else {
            return Err(ImageError::Decode("XPM: comentario sin cerrar".into()));
        };
        rest = &rest[start + 2 + end_rel + 2..];
    }
    cleaned.push_str(rest);

    // Extrae el contenido de { … }.
    let Some(open) = cleaned.find('{') else {
        return Err(ImageError::Decode("XPM: falta '{'".into()));
    };
    let Some(close_rel) = cleaned[open..].rfind('}') else {
        return Err(ImageError::Decode("XPM: falta '}'".into()));
    };
    let body = &cleaned[open + 1..open + close_rel];

    // Divide por comas, recortando comillas y espacios.
    let mut lines = Vec::new();
    for part in body.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let part = part
            .strip_prefix('"')
            .and_then(|p| p.strip_suffix('"'))
            .ok_or_else(|| ImageError::Decode("XPM: línea sin comillas".into()))?;
        // Une escapes básicos (\" dentro de cadenas es raro en XPM, pero
        // los dumps antiguos a veces meten comas escapadas).
        lines.push(part.to_string());
    }
    Ok(lines)
}

fn parse_lines(lines: &[String]) -> Result<ImageData> {
    let Some(header) = lines.first() else {
        return Err(ImageError::Decode("XPM: sin cabecera".into()));
    };
    let mut header_parts = header.split_whitespace();
    let width: u32 = header_parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ImageError::Decode("XPM: ancho inválido".into()))?;
    let height: u32 = header_parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ImageError::Decode("XPM: alto inválido".into()))?;
    let n_colors: usize = header_parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ImageError::Decode("XPM: número de colores inválido".into()))?;
    let cpp: usize = header_parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ImageError::Decode("XPM: cpp inválido".into()))?;
    if width == 0 || height == 0 || n_colors == 0 || cpp == 0 {
        return Err(ImageError::Decode("XPM: dimensiones vacías".into()));
    }
    if lines.len() < 1 + n_colors + height as usize {
        return Err(ImageError::Decode("XPM: faltan líneas".into()));
    }

    // Tabla de color: código → RGBA recto.
    let mut palette = std::collections::HashMap::with_capacity(n_colors);
    for entry in &lines[1..=n_colors] {
        let mut parts = entry.split_whitespace();
        let Some(code) = parts.next() else { continue };
        // Busca el token con la clave `c` (los demás, como `m`/`g4`, se
        // prefieren solo si no hay `c`).
        let mut color = None;
        let mut keys = parts.clone().peekable();
        while let Some(key) = keys.next() {
            if key == "c" {
                color = keys.next().map(srgb_color);
                break;
            } else {
                let _ = keys.next(); // valor de otra clave (m, g4, s…)
            }
        }
        let rgba = color
            .flatten()
            .ok_or_else(|| ImageError::Decode(format!("XPM: color sin clave 'c' en {entry:?}")))?;
        palette.insert(code.to_string(), rgba);
    }

    // Píxeles.
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row_line in &lines[1 + n_colors..1 + n_colors + height as usize] {
        let row = row_line.trim();
        if row.len() < width as usize * cpp {
            return Err(ImageError::Decode("XPM: fila de píxeles corta".into()));
        }
        for x in 0..width as usize {
            let code = &row[x * cpp..(x + 1) * cpp];
            let rgba = palette
                .get(code)
                .copied()
                .ok_or_else(|| ImageError::Decode(format!("XPM: código sin color {code:?}")))?;
            // El XPM es alfa recta por naturaleza; premultiplicamos aquí.
            let a = rgba[3] as u32;
            pixels.extend_from_slice(&[
                (rgba[0] as u32 * a / 255) as u8,
                (rgba[1] as u32 * a / 255) as u8,
                (rgba[2] as u32 * a / 255) as u8,
                rgba[3],
            ]);
        }
    }
    Ok(ImageData::from_premultiplied(width, height, pixels))
}

/// Color XPM (`#hex`, `None`, o nombre) a RGBA recto.
fn srgb_color(spec: &str) -> Option<[u8; 4]> {
    if spec.eq_ignore_ascii_case("none") {
        return Some([0, 0, 0, 0]);
    }
    if let Some(hex) = spec.strip_prefix('#') {
        let digits: Vec<u8> = hex
            .bytes()
            .map(|b| (b as char).to_digit(16).map(|d| d as u8))
            .collect::<Option<Vec<_>>>()?;
        return match digits.len() {
            // #RGB → 4 bits por canal.
            3 => Some([digits[0] * 17, digits[1] * 17, digits[2] * 17, 255]),
            // #RRGGBB.
            6 => Some([
                digits[0] * 16 + digits[1],
                digits[2] * 16 + digits[3],
                digits[4] * 16 + digits[5],
                255,
            ]),
            // #RRRRGGGGBBBB (12 dígitos): recortamos a 8 bits.
            12 => Some([
                digits[0] * 16 + digits[1],
                digits[4] * 16 + digits[5],
                digits[8] * 16 + digits[9],
                255,
            ]),
            _ => None,
        };
    }
    // 16 nombres básicos del X11 clásico.
    let rgb = match spec.to_ascii_lowercase().as_str() {
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        "red" => [255, 0, 0],
        "green" => [0, 255, 0],
        "blue" => [0, 0, 255],
        "yellow" => [255, 255, 0],
        "magenta" => [255, 0, 255],
        "cyan" => [0, 255, 255],
        "gray" | "grey" => [190, 190, 190],
        "orange" => [255, 165, 0],
        "pink" => [255, 192, 203],
        "purple" => [160, 32, 240],
        "brown" => [165, 42, 42],
        "navy" => [0, 0, 128],
        "lime" => [50, 205, 50],
        "silver" => [192, 192, 192],
        _ => return None,
    };
    Some([rgb[0], rgb[1], rgb[2], 255])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMILEY: &str = r##"/* XPM */
static char *test_xpm[] = {
"4 3 3 1",
". c #FF0000",
"# c None",
"o c blue",
".#o.",
".#o.",
".#o."
};
"##;

    #[test]
    fn decodifica_xpm_basico() {
        let image = decode_xpm(SMILEY.as_bytes()).unwrap();
        assert_eq!(image.size(), (4, 3));
        let px = image.pixels();
        // Fila 0: rojo premultiplicado, transparente, azul, rojo.
        assert_eq!(&px[0..4], &[255, 0, 0, 255]);
        assert_eq!(&px[4..8], &[0, 0, 0, 0]);
        assert_eq!(&px[8..12], &[0, 0, 255, 255]);
        assert_eq!(&px[12..16], &[255, 0, 0, 255]);
        // Fila 2 (offset 2*4 píxeles), primer píxel.
        assert_eq!(&px[32..36], &[255, 0, 0, 255]);
    }

    #[test]
    fn colores_con_nombre_y_rbg_corto() {
        const NAMED: &str = r##"/* XPM */
static char *n[] = {
"2 1 2 1",
"a c white",
"b c #0F0",
"ab"
};
"##;
        let image = decode_xpm(NAMED.as_bytes()).unwrap();
        assert_eq!(image.pixels()[0..4], [255, 255, 255, 255]);
        // #0F0 → 0x0, 0xF(15*17=255), 0x0.
        assert_eq!(image.pixels()[4..8], [0, 255, 0, 255]);
    }

    #[test]
    fn errores_de_formato() {
        assert!(decode_xpm(b"sin llaves").is_err());
        assert!(decode_xpm(b"{ \"9 9 9 9\" }").is_err());
    }
}
