//! Tokenizador del DSL `.rivet-style`: casero, sin dependencias.
//!
//! Tokens: identificadores, números, porcentajes, colores hex,
//! puntuación (`{ } : ; ( ) ,`) y palabras clave. Comentarios `//` y
//! `/* … */` desechados en el lexer.

use std::fmt;

/// Token del DSL con la línea de origen.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    /// Clase del token.
    pub kind: TokenKind,
    /// Línea (1-based) para errores.
    pub line: usize,
}

/// Clases de token.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    /// Identificador (widget, color, función, propiedad…).
    Ident(String),
    /// Número entero o decimal.
    Number(f32),
    /// Número con sufijo `%`.
    Percent(f32),
    /// Color `#RRGGBB[AA]` (sin normalizar).
    Hex(String),
    /// Llave `{`.
    OpenBrace,
    /// Llave `}`.
    CloseBrace,
    /// Dos puntos.
    Colon,
    /// Punto y coma.
    Semicolon,
    /// Paréntesis de apertura.
    OpenParen,
    /// Paréntesis de cierre.
    CloseParen,
    /// Coma.
    Comma,
    /// Fin de archivo.
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Ident(s) => write!(f, "{s:?}"),
            TokenKind::Number(v) => write!(f, "{v}"),
            TokenKind::Percent(v) => write!(f, "{v}%"),
            TokenKind::Hex(s) => write!(f, "#{s}"),
            TokenKind::OpenBrace => write!(f, "{{"),
            TokenKind::CloseBrace => write!(f, "}}"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Semicolon => write!(f, ";"),
            TokenKind::OpenParen => write!(f, "("),
            TokenKind::CloseParen => write!(f, ")"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Eof => write!(f, "fin de archivo"),
        }
    }
}

/// Tokeniza el fuente completo.
pub(crate) fn lex(source: &str) -> crate::error::Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    let mut line = 1usize;
    while let Some(&(index, ch)) = chars.peek() {
        match ch {
            '\n' => {
                line += 1;
                chars.next();
            }
            c if c.is_whitespace() => {
                chars.next();
            }
            '/' => {
                // Comentario de línea o bloque.
                let mut it = chars.clone();
                it.next();
                if it.peek().is_some_and(|&(_, c)| c == '/') {
                    while let Some(&(_, c)) = chars.peek() {
                        if c == '\n' {
                            break;
                        }
                        chars.next();
                    }
                } else if it.peek().is_some_and(|&(_, c)| c == '*') {
                    chars.next();
                    chars.next();
                    let mut closed = false;
                    while let Some((_, c)) = chars.next() {
                        if c == '\n' {
                            line += 1;
                        }
                        if c == '*' && chars.peek().is_some_and(|&(_, c2)| c2 == '/') {
                            chars.next();
                            closed = true;
                            break;
                        }
                    }
                    if !closed {
                        return Err(crate::error::StyleError::Lex {
                            line,
                            message: "comentario de bloque sin cerrar".into(),
                        });
                    }
                } else {
                    return Err(crate::error::StyleError::Lex {
                        line,
                        message: format!("carácter inesperado '/' en {index}"),
                    });
                }
            }
            '#' => {
                chars.next();
                let mut hex = String::new();
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_ascii_hexdigit() {
                        hex.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if hex.len() != 3 && hex.len() != 6 && hex.len() != 8 {
                    return Err(crate::error::StyleError::Lex {
                        line,
                        message: format!("hex de {} dígitos (se esperaban 3, 6 u 8)", hex.len()),
                    });
                }
                tokens.push(Token {
                    kind: TokenKind::Hex(hex),
                    line,
                });
            }
            '{' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::OpenBrace,
                    line,
                });
            }
            '}' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::CloseBrace,
                    line,
                });
            }
            ':' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::Colon,
                    line,
                });
            }
            ';' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::Semicolon,
                    line,
                });
            }
            '(' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::OpenParen,
                    line,
                });
            }
            ')' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::CloseParen,
                    line,
                });
            }
            ',' => {
                chars.next();
                tokens.push(Token {
                    kind: TokenKind::Comma,
                    line,
                });
            }
            c if c.is_ascii_digit() || c == '-' || c == '+' => {
                let mut number = String::new();
                if ch == '-' || ch == '+' {
                    number.push(ch);
                    chars.next();
                }
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        number.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let value: f32 = number.parse().map_err(|_| crate::error::StyleError::Lex {
                    line,
                    message: format!("número inválido {number:?}"),
                })?;
                let percent = if chars.peek().is_some_and(|&(_, c)| c == '%') {
                    chars.next();
                    true
                } else {
                    false
                };
                tokens.push(Token {
                    kind: if percent {
                        TokenKind::Percent(value)
                    } else {
                        TokenKind::Number(value)
                    },
                    line,
                });
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut ident = String::new();
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        ident.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token {
                    kind: TokenKind::Ident(ident),
                    line,
                });
            }
            other => {
                return Err(crate::error::StyleError::Lex {
                    line,
                    message: format!("carácter inesperado {other:?}"),
                });
            }
        }
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        line,
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn tokens_basicos() {
        assert_eq!(
            kinds("widget button { }"),
            vec![
                TokenKind::Ident("widget".into()),
                TokenKind::Ident("button".into()),
                TokenKind::OpenBrace,
                TokenKind::CloseBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn numeros_colores_y_comentarios() {
        let tokens = kinds("// línea\n/* bloque */ padding: 8 12.5; #1e1e1e 50%");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Ident("padding".into()),
                TokenKind::Colon,
                TokenKind::Number(8.0),
                TokenKind::Number(12.5),
                TokenKind::Semicolon,
                TokenKind::Hex("1e1e1e".into()),
                TokenKind::Percent(50.0),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_error_de_comentario_abierto() {
        assert!(lex("/* sin cerrar").is_err());
    }
}
