//! Compilador: DSL → AST → resolución de tema/derivados → [`StyleTable`].
//!
//! Todo el trabajo caro (parseo, funciones de color, herencia de estados)
//! ocurre **aquí**, una vez por tema; el runtime solo indexa (doc §2).

use std::collections::HashMap;

use crate::ast::{Node, Value, WidgetNode};
use crate::color::Color;
use crate::error::{Result, StyleError};
use crate::parser::parse;
use crate::tables::{
    build_table, Ease, Insets, StyleTable, Transition, WidgetKind, WidgetStyle, STATE_ROW_COUNT,
};

/// Compila el fuente del DSL para el tema `theme_name` (debe estar
/// declarado en el propio fuente con `theme <name> { … }`).
///
/// # Errors
///
/// - [`StyleError::UnknownTheme`] si el tema no está declarado.
/// - Errores de léxico/sintaxis/resolución propagados.
pub fn compile(source: &str, theme_name: &str) -> Result<StyleTable> {
    let nodes = parse(source)?;

    // Tema: variables resueltas primero (literals y funciones de literals).
    let theme_node = nodes
        .iter()
        .find_map(|node| match node {
            Node::Theme(theme) if theme.name == theme_name => Some(theme.clone()),
            _ => None,
        })
        .ok_or_else(|| StyleError::UnknownTheme(theme_name.to_string()))?;
    let mut theme_vars: HashMap<String, Value> = HashMap::new();
    for var in &theme_node.vars {
        let resolved = resolve_value(&var.value, &theme_vars, &HashMap::new())?;
        theme_vars.insert(
            var.name.clone(),
            match resolved {
                Resolved::Color(c) => Value::Color(c),
                Resolved::Num(n) => Value::Number(n),
                Resolved::NumList(_) => {
                    return Err(StyleError::invalid_value(
                        "variable de tema no numérica múltiple",
                    ))
                }
            },
        );
    }

    // Widgets: base + estados por fila.
    let mut rows: Vec<[WidgetStyle; STATE_ROW_COUNT]> = (0..crate::tables::WIDGET_KIND_COUNT)
        .map(|_| [WidgetStyle::default(); STATE_ROW_COUNT])
        .collect();
    for node in &nodes {
        let Node::Widget(widget) = node else { continue };
        let index = kind_index(&widget.name)?;
        rows[index] = compile_widget(widget, &theme_vars)?;
    }
    Ok(build_table(theme_name.to_string(), rows))
}

/// Compila un widget: base + 5 filas de estado (heredando de la base).
fn compile_widget(
    widget: &WidgetNode,
    theme_vars: &HashMap<String, Value>,
) -> Result<[WidgetStyle; STATE_ROW_COUNT]> {
    // Base: valores crudos por propiedad (entorno para refs de funciones).
    let mut base_values: HashMap<String, Value> = HashMap::new();
    for prop in &widget.props {
        // La transición es sintaxis especial (ya concreta en el AST):
        // no pasa por el resolvedor de funciones de color.
        if prop.name == "transition" {
            base_values.insert(prop.name.clone(), prop.value.clone());
            continue;
        }
        // Resuelve YA contra el entorno (tema + propiedades previas).
        let resolved = resolve_value(&prop.value, theme_vars, &base_values)?;
        base_values.insert(prop.name.clone(), resolved_to_value(resolved));
    }
    let base = apply_props(WidgetStyle::default(), &base_values)?;

    let mut rows = [base; STATE_ROW_COUNT];
    for state in &widget.states {
        let row = state_row_by_name(&state.name)?;
        let mut state_values = base_values.clone();
        for prop in &state.props {
            if prop.name == "transition" {
                state_values.insert(prop.name.clone(), prop.value.clone());
                continue;
            }
            // Las referencias dentro de un estado resuelven contra la BASE
            // (derivados por tema calculados en compilación, ADR-06).
            let resolved = resolve_value(&prop.value, theme_vars, &base_values)?;
            state_values.insert(prop.name.clone(), resolved_to_value(resolved));
        }
        rows[row] = apply_props(base, &state_values)?;
    }
    Ok(rows)
}

fn resolved_to_value(resolved: Resolved) -> Value {
    match resolved {
        Resolved::Color(c) => Value::Color(c),
        Resolved::Num(n) => Value::Number(n),
        Resolved::NumList(numbers) => Value::Numbers(numbers),
    }
}

/// Valor ya resuelto (sin funciones ni referencias pendientes).
enum Resolved {
    Color(Color),
    Num(f32),
    /// Lista de números resuelta (padding).
    NumList(Vec<f32>),
}

/// Entorno de resolución: variables de tema + propiedades base.
struct Env<'a> {
    theme: &'a HashMap<String, Value>,
    base: &'a HashMap<String, Value>,
}

fn resolve_value(
    value: &Value,
    theme: &HashMap<String, Value>,
    base: &HashMap<String, Value>,
) -> Result<Resolved> {
    let env = Env { theme, base };
    resolve(value, &env, 0)
}

fn resolve(value: &Value, env: &Env<'_>, depth: usize) -> Result<Resolved> {
    if depth > 8 {
        return Err(StyleError::InvalidValue(
            "referencias demasiado profundas (¿ciclo?)".into(),
        ));
    }
    match value {
        Value::Color(c) => Ok(Resolved::Color(*c)),
        Value::Number(n) => Ok(Resolved::Num(*n)),
        Value::Percent(p) => Ok(Resolved::Num(*p)),
        Value::Numbers(numbers) => Ok(Resolved::NumList(numbers.clone())),
        Value::Ref(name) => {
            // Propiedad base primero; luego variable de tema.
            if let Some(v) = env.base.get(name) {
                return resolve(v, env, depth + 1);
            }
            if let Some(v) = env.theme.get(name) {
                return resolve(
                    v,
                    &Env {
                        theme: env.theme,
                        base: &HashMap::new(),
                    },
                    depth + 1,
                );
            }
            Err(StyleError::Unresolved(name.clone()))
        }
        Value::Fn { name, args } => {
            let function = match name.as_str() {
                "lighten" => lighten_fn,
                "darken" => darken_fn,
                "alpha" => alpha_fn,
                "mix" => mix_fn,
                other => {
                    return Err(StyleError::invalid_value(format!(
                        "función desconocida {other:?} (lighten/darken/alpha/mix)"
                    )))
                }
            };
            function(args, env, depth)
        }
    }
}

fn resolve_color(value: &Value, env: &Env<'_>, depth: usize) -> Result<Color> {
    match resolve(value, env, depth)? {
        Resolved::Color(c) => Ok(c),
        Resolved::Num(_) | Resolved::NumList(_) => {
            Err(StyleError::invalid_value("se esperaba color"))
        }
    }
}

/// Porcentaje en escala 0..100 (el token `%` lo garantiza).
fn resolve_percent(value: &Value, env: &Env<'_>, depth: usize) -> Result<f32> {
    match resolve(value, env, depth)? {
        Resolved::Num(n) => Ok(n),
        Resolved::Color(_) => Err(StyleError::invalid_value("se esperaba porcentaje")),
        Resolved::NumList(_) => Err(StyleError::invalid_value("se esperaba porcentaje")),
    }
}

fn lighten_fn(args: &[Value], env: &Env<'_>, depth: usize) -> Result<Resolved> {
    let [color, percent] = args else {
        return Err(StyleError::invalid_value("lighten(color, %)"));
    };
    let color = resolve_color(color, env, depth)?;
    let percent = resolve_percent(percent, env, depth)?;
    Ok(Resolved::Color(color.lighten(percent)))
}

fn darken_fn(args: &[Value], env: &Env<'_>, depth: usize) -> Result<Resolved> {
    let [color, percent] = args else {
        return Err(StyleError::invalid_value("darken(color, %)"));
    };
    let color = resolve_color(color, env, depth)?;
    let percent = resolve_percent(percent, env, depth)?;
    Ok(Resolved::Color(color.darken(percent)))
}

fn alpha_fn(args: &[Value], env: &Env<'_>, depth: usize) -> Result<Resolved> {
    let [color, percent] = args else {
        return Err(StyleError::invalid_value("alpha(color, %)"));
    };
    let color = resolve_color(color, env, depth)?;
    let percent = resolve_percent(percent, env, depth)?;
    Ok(Resolved::Color(color.with_alpha_f(percent / 100.0)))
}

fn mix_fn(args: &[Value], env: &Env<'_>, depth: usize) -> Result<Resolved> {
    let [a, b, percent] = args else {
        return Err(StyleError::invalid_value("mix(colorA, colorB, %)"));
    };
    let a = resolve_color(a, env, depth)?;
    let b = resolve_color(b, env, depth)?;
    let percent = resolve_percent(percent, env, depth)?;
    Ok(Resolved::Color(a.mix(b, percent / 100.0)))
}

/// Aplica un mapa de propiedades resueltas sobre un estilo base.
fn apply_props(base: WidgetStyle, values: &HashMap<String, Value>) -> Result<WidgetStyle> {
    let mut style = base;
    let env = Env {
        theme: &HashMap::new(),
        base: values,
    };
    for (name, value) in values {
        match name.as_str() {
            "background" => style.background = resolve_color(value, &env, 0)?,
            "color" | "foreground" => style.foreground = resolve_color(value, &env, 0)?,
            "border-color" => style.border_color = resolve_color(value, &env, 0)?,
            "border-width" => style.border_width = expect_number(value, &env)?,
            "border-radius" => style.border_radius = expect_number(value, &env)?,
            "font-size" => style.font_size = expect_number(value, &env)?,
            "padding" => style.padding = expect_insets(value, &env)?,
            "transition" => style.transition = expect_transition(value, &env)?,
            other => {
                return Err(StyleError::invalid_value(format!(
                    "propiedad desconocida {other:?}"
                )))
            }
        }
    }
    Ok(style)
}

fn expect_number(value: &Value, env: &Env<'_>) -> Result<f32> {
    match resolve(value, env, 0)? {
        Resolved::Num(n) => Ok(n),
        Resolved::Color(_) | Resolved::NumList(_) => {
            Err(StyleError::invalid_value("se esperaba número"))
        }
    }
}

fn expect_insets(value: &Value, env: &Env<'_>) -> Result<Insets> {
    let numbers = match value {
        Value::Numbers(numbers) => numbers.clone(),
        Value::Number(n) => vec![*n],
        Value::Ref(_) | Value::Fn { .. } => match resolve(value, env, 0)? {
            Resolved::Num(n) => vec![n],
            Resolved::NumList(numbers) => numbers,
            Resolved::Color(_) => return Err(StyleError::invalid_value("padding numérico")),
        },
        Value::Percent(_) | Value::Color(_) => {
            return Err(StyleError::invalid_value("padding numérico"))
        }
    };
    match numbers.as_slice() {
        [v] => Ok(Insets::all(*v)),
        [v, h] => Ok(Insets::vertical_horizontal(*v, *h)),
        [t, r, b, l] => Ok(Insets::trbl(*t, *r, *b, *l)),
        _ => Err(StyleError::invalid_value("padding: 1, 2 o 4 valores")),
    }
}

fn expect_transition(value: &Value, _env: &Env<'_>) -> Result<Transition> {
    // AST: Fn { name: "transition", args: [Number(ms), Ref(ease)] }.
    let Value::Fn { name, args } = value else {
        return Ok(Transition::default());
    };
    if name != "transition" {
        return Err(StyleError::invalid_value("transición mal formada"));
    }
    let [Value::Number(ms), ease] = args.as_slice() else {
        return Err(StyleError::invalid_value("transition: 150ms ease"));
    };
    let ease = match ease {
        Value::Ref(word) if word == "linear" => Ease::Linear,
        Value::Ref(word) if word == "ease" => Ease::Ease,
        _ => Ease::Linear,
    };
    Ok(Transition {
        duration_ms: ms.max(0.0) as u32,
        ease,
    })
}

fn kind_index(name: &str) -> Result<usize> {
    let kind = match name {
        "button" => WidgetKind::Button,
        "label" => WidgetKind::Label,
        "text-input" | "textinput" => WidgetKind::TextInput,
        "window" => WidgetKind::Window,
        "panel" => WidgetKind::Panel,
        "checkbox" => WidgetKind::Checkbox,
        "slider" => WidgetKind::Slider,
        "scrollbar" => WidgetKind::Scrollbar,
        "tooltip" => WidgetKind::Tooltip,
        "icon" => WidgetKind::Icon,
        other => return Err(StyleError::UnknownWidget(other.to_string())),
    };
    Ok(kind as u16 as usize)
}

fn state_row_by_name(name: &str) -> Result<usize> {
    match name {
        "normal" => Ok(0),
        "hover" => Ok(1),
        "focused" => Ok(2),
        "active" => Ok(3),
        "disabled" => Ok(4),
        other => Err(StyleError::invalid_value(format!(
            "estado desconocido {other:?} (normal/hover/focused/active/disabled)"
        ))),
    }
}
