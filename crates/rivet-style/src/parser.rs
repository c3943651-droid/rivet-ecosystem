//! Descending parser for the DSL (on top of the lexer's tokens).

use crate::ast::{Node, Prop, StateNode, ThemeNode, Value, WidgetNode};
use crate::color::Color;
use crate::error::{Result, StyleError};
use crate::lexer::{lex, Token, TokenKind};

/// Parsea el fuente completo a la lista de nodos (widgets y temas).
pub(crate) fn parse(source: &str) -> Result<Vec<Node>> {
    let tokens = lex(source)?;
    let mut parser = Parser { tokens, pos: 0 };
    let mut nodes = Vec::new();
    while !matches!(parser.peek().kind, TokenKind::Eof) {
        nodes.push(parser.parse_node()?);
    }
    Ok(nodes)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn next(&mut self) -> Token {
        let token = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        self.pos += 1;
        token
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token> {
        let token = self.next();
        if &token.kind != kind {
            return Err(StyleError::Parse {
                line: token.line,
                message: format!("se esperaba {kind}, llegó {}", token.kind),
            });
        }
        Ok(token)
    }

    fn expect_ident(&mut self) -> Result<Token> {
        let token = self.next();
        if !matches!(token.kind, TokenKind::Ident(_)) {
            return Err(StyleError::Parse {
                line: token.line,
                message: format!("se esperaba identificador, llegó {}", token.kind),
            });
        }
        Ok(token)
    }

    fn parse_node(&mut self) -> Result<Node> {
        let token = self.expect_ident()?;
        let TokenKind::Ident(keyword) = token.kind else {
            unreachable!("expect_ident garantiza Ident");
        };
        match keyword.as_str() {
            "widget" => Ok(Node::Widget(self.parse_widget()?)),
            "theme" => Ok(Node::Theme(self.parse_theme()?)),
            other => Err(StyleError::Parse {
                line: token.line,
                message: format!("palabra clave inesperada {other:?} (¿widget o theme?)"),
            }),
        }
    }

    fn parse_widget(&mut self) -> Result<WidgetNode> {
        let name = self.expect_ident()?;
        let TokenKind::Ident(name) = name.kind else {
            unreachable!();
        };
        self.expect(&TokenKind::OpenBrace)?;
        let mut props = Vec::new();
        let mut states = Vec::new();
        loop {
            match self.peek().kind.clone() {
                TokenKind::CloseBrace => {
                    self.next();
                    break;
                }
                TokenKind::Ident(ref word) if word == "state" => {
                    self.next(); // 'state'
                    states.push(self.parse_state()?);
                }
                TokenKind::Ident(_) => {
                    let line = self.peek().line;
                    let prop = self.parse_prop()?;
                    if !props.iter().any(|p: &crate::ast::Prop| p.name == prop.name) {
                        props.push(prop);
                    } else {
                        return Err(StyleError::Parse {
                            line,
                            message: format!("propiedad duplicada {:?}", prop.name),
                        });
                    }
                }
                other => {
                    let token = self.next();
                    return Err(StyleError::Parse {
                        line: token.line,
                        message: format!("se esperaba propiedad o 'state', llegó {other}"),
                    });
                }
            }
        }
        Ok(WidgetNode {
            name,
            props,
            states,
        })
    }

    fn parse_state(&mut self) -> Result<StateNode> {
        let name = self.expect_ident()?;
        let TokenKind::Ident(name) = name.kind else {
            unreachable!();
        };
        self.expect(&TokenKind::OpenBrace)?;
        let mut props = Vec::new();
        loop {
            match self.peek().kind.clone() {
                TokenKind::CloseBrace => {
                    self.next();
                    break;
                }
                TokenKind::Ident(_) => {
                    let prop = self.parse_prop()?;
                    if !props.iter().any(|p: &crate::ast::Prop| p.name == prop.name) {
                        props.push(prop);
                    } else {
                        return Err(StyleError::Parse {
                            line: self.peek().line,
                            message: format!("propiedad duplicada en state {name:?}"),
                        });
                    }
                }
                other => {
                    let token = self.next();
                    return Err(StyleError::Parse {
                        line: token.line,
                        message: format!("se esperaba propiedad en state, llegó {other}"),
                    });
                }
            }
        }
        Ok(StateNode { name, props })
    }

    fn parse_theme(&mut self) -> Result<ThemeNode> {
        let name = self.expect_ident()?;
        let TokenKind::Ident(name) = name.kind else {
            unreachable!();
        };
        self.expect(&TokenKind::OpenBrace)?;
        let mut vars = Vec::new();
        loop {
            match self.peek().kind.clone() {
                TokenKind::CloseBrace => {
                    self.next();
                    break;
                }
                TokenKind::Ident(_) => {
                    vars.push(self.parse_prop()?);
                }
                other => {
                    let token = self.next();
                    return Err(StyleError::Parse {
                        line: token.line,
                        message: format!("se esperaba variable de tema, llegó {other}"),
                    });
                }
            }
        }
        Ok(ThemeNode { name, vars })
    }

    /// `nombre: valor` (el `;` final es opcional si hay salto de bloque).
    fn parse_prop(&mut self) -> Result<Prop> {
        let ident = self.expect_ident()?;
        let TokenKind::Ident(name) = ident.kind else {
            unreachable!();
        };
        self.expect(&TokenKind::Colon)?;
        let value = if name == "transition" {
            self.parse_transition()?
        } else {
            self.parse_value()?
        };
        // `;` opcional pero consumible.
        if matches!(self.peek().kind, TokenKind::Semicolon) {
            self.next();
        }
        Ok(Prop { name, value })
    }

    /// `transition: 150ms ease` → Fn { "transition", [Number(ms), Ref(ease)] }.
    fn parse_transition(&mut self) -> Result<Value> {
        let token = self.next();
        let TokenKind::Number(ms) = token.kind else {
            return Err(StyleError::Parse {
                line: token.line,
                message: format!("duración inválida: {}", token.kind),
            });
        };
        // Unidad 'ms' opcional.
        if let TokenKind::Ident(unit) = self.peek().kind.clone() {
            if unit == "ms" {
                self.next();
            }
        }
        // Curva opcional.
        let ease = if let TokenKind::Ident(word) = self.peek().kind.clone() {
            if word == "linear" || word == "ease" {
                self.next();
                word
            } else {
                String::from("linear")
            }
        } else {
            String::from("linear")
        };
        Ok(Value::Fn {
            name: String::from("transition"),
            args: vec![Value::Number(ms), Value::Ref(ease)],
        })
    }

    fn parse_value(&mut self) -> Result<Value> {
        let token = self.next();
        match token.kind {
            TokenKind::Hex(hex) => Ok(Value::Color(Color::parse_hex(&hex)?)),
            TokenKind::Percent(p) => Ok(Value::Percent(p)),
            TokenKind::Number(first) => {
                // Unidad opcional (v1: px/ms).
                if let TokenKind::Ident(unit) = self.peek().kind.clone() {
                    if unit != "ms" && unit != "px" {
                        return Err(StyleError::Parse {
                            line: self.peek().line,
                            message: format!("unidad desconocida {unit:?} (px/ms)"),
                        });
                    }
                    self.next();
                }
                // Padding con varios números: 1, 2 o 4 valores.
                let mut numbers = vec![first];
                while let TokenKind::Number(n) = self.peek().kind {
                    numbers.push(n);
                    self.next();
                }
                if numbers.len() == 1 {
                    Ok(Value::Number(numbers[0]))
                } else {
                    Ok(Value::Numbers(numbers))
                }
            }
            TokenKind::Ident(word) => {
                if matches!(self.peek().kind, TokenKind::OpenParen) {
                    self.next();
                    // Función: nombre(args, …).
                    let mut args = Vec::new();
                    if !matches!(self.peek().kind, TokenKind::CloseParen) {
                        loop {
                            args.push(self.parse_value()?);
                            match self.peek().kind.clone() {
                                TokenKind::Comma => {
                                    self.next();
                                }
                                _ => break,
                            }
                        }
                    }
                    self.expect(&TokenKind::CloseParen)?;
                    Ok(Value::Fn { name: word, args })
                } else {
                    // Referencia a variable de tema o propiedad.
                    Ok(Value::Ref(word))
                }
            }
            other => Err(StyleError::Parse {
                line: token.line,
                message: format!("valor inesperado: {other}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
widget button {
    background: #3a7bd5;
    color: #ffffff;
    border-radius: 6;
    padding: 8 12;
    font-size: 14;
    transition: 150ms ease;

    state hover  { background: lighten(background, 10%); }
    state active { background: darken(background, 10%); }
    state disabled { color: alpha(color, 40%); }
}
theme dark {
    base: #1e1e1e;
    text: #e0e0e0;
}
"#;

    #[test]
    fn parsea_el_ejemplo_de_la_doc() {
        let nodes = parse(EXAMPLE).unwrap();
        assert_eq!(nodes.len(), 2);
        let Node::Widget(button) = &nodes[0] else {
            panic!("primer nodo es widget");
        };
        assert_eq!(button.name, "button");
        assert_eq!(button.props.len(), 6);
        assert_eq!(button.states.len(), 3);
        assert_eq!(button.states[0].name, "hover");
        let Node::Theme(dark) = &nodes[1] else {
            panic!("segundo nodo es theme");
        };
        assert_eq!(dark.name, "dark");
        assert_eq!(dark.vars.len(), 2);
    }

    #[test]
    fn errores_de_sintaxis() {
        assert!(parse("button { }").is_err(), "falta 'widget'");
        assert!(parse("widget button").is_err(), "falta bloque");
        assert!(parse("widget button { color }").is_err(), "falta ':'");
    }
}
