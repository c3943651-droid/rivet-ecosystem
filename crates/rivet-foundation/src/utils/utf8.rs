//! Safe UTF-8 truncation.
//!
//! Wayland and TrueType deliver strings with a length in bytes, not in
//! characters: this helper cuts at a code point boundary without splitting a `char`.

/// Devuelve el prefijo de `s` que cabe en `max_bytes`, sin partir un `char`.
///
/// Si `max_bytes >= s.len()` se devuelve `s` completo.
#[must_use]
pub fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_parte_caracteres_multibyte() {
        let s = "héllo";
        // 'h'(1) + 'é'(2) = 3 bytes: el prefijo "hé" cabe justo en 3.
        assert_eq!(truncate_utf8(s, 3), "hé");
        // Cortar en medio de 'é' retrocede al límite de char válido.
        assert_eq!(truncate_utf8(s, 2), "h");
        assert_eq!(truncate_utf8(s, 1), "h");
        assert_eq!(truncate_utf8("日本語", 3), "日");
    }

    #[test]
    fn entrada_completa_o_vacia() {
        assert_eq!(truncate_utf8("abc", 10), "abc");
        assert_eq!(truncate_utf8("abc", 0), "");
    }
}
