//! AST del DSL `.rivet-style`: representación intermedia entre el parser y
//! el compilador a tablas planas.

use crate::color::Color;

/// Hoja de la unidad de compilación: widgets y temas.
#[derive(Debug, Clone)]
pub(crate) enum Node {
    /// `widget <nombre> { <propiedades> [state <nombre> { … }]* }`.
    Widget(WidgetNode),
    /// `theme <nombre> { <variable>: <valor>; }`.
    Theme(ThemeNode),
}

/// Declaración de widget.
#[derive(Debug, Clone)]
pub(crate) struct WidgetNode {
    /// Nombre del widget (button, label, panel…).
    pub name: String,
    /// Propiedades del estado base.
    pub props: Vec<Prop>,
    /// Estados con propiedades que se solapan sobre la base.
    pub states: Vec<StateNode>,
}

/// Estado (`state hover { … }`).
#[derive(Debug, Clone)]
pub(crate) struct StateNode {
    /// Nombre del estado (hover/active/focused/disabled).
    pub name: String,
    /// Propiedades solapadas.
    pub props: Vec<Prop>,
}

/// Tema: variables de color/número.
#[derive(Debug, Clone)]
pub(crate) struct ThemeNode {
    /// Nombre del tema (dark, light…).
    pub name: String,
    /// Variables (nombre → valor).
    pub vars: Vec<Prop>,
}

/// Propiedad: `nombre: valor`.
#[derive(Debug, Clone)]
pub(crate) struct Prop {
    /// Nombre (background, color, padding…).
    pub name: String,
    /// Valor (crudo: aún sin resolver).
    pub value: Value,
}

/// Valor del DSL aún sin resolver.
#[derive(Debug, Clone)]
pub(crate) enum Value {
    /// Color literal.
    Color(Color),
    /// Número.
    Number(f32),
    /// Porcentaje (0..100).
    Percent(f32),
    /// Lista de números (padding 1/2/4 valores).
    Numbers(Vec<f32>),
    /// Referencia a variable de tema o propiedad del widget.
    Ref(String),
    /// Función de derivación: `lighten(color, 10%)`.
    Fn {
        /// Nombre de la función (lighten/darken/alpha/mix).
        name: String,
        /// Argumentos.
        args: Vec<Value>,
    },
}
