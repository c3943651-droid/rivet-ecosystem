//! Integration tests: compile the documented DSL and query the tables
//! (fully deterministic, no external dependencies).

use rivet_style::{compile, Color, Ease, Insets, WidgetKind, WidgetState, EXAMPLE_DSL};

#[test]
fn compila_el_ejemplo_y_deriva_colores() {
    let table = compile(EXAMPLE_DSL, "dark").unwrap();

    // Base del botón.
    let normal = table.get(WidgetKind::Button, WidgetState::NORMAL);
    assert_eq!(normal.background, Color::rgb(0x3a, 0x7b, 0xd5));
    assert_eq!(normal.foreground, Color::WHITE);
    assert_eq!(normal.border_radius, 6.0);
    assert_eq!(normal.padding, Insets::vertical_horizontal(8.0, 12.0));
    assert_eq!(normal.font_size, 14.0);
    assert_eq!(normal.transition.duration_ms, 150);
    assert_eq!(normal.transition.ease, Ease::Ease);

    // Hover: lighten(background, 10%) — derivado en COMPILACIÓN (ADR-06).
    let hover = table.get(WidgetKind::Button, WidgetState::HOVER);
    assert_eq!(
        hover.background,
        Color::rgb(0x3a, 0x7b, 0xd5).lighten(10.0),
        "el hover es el lighten del base precalculado"
    );
    assert!(hover.background.r > normal.background.r);
    // El resto de propiedades hereda del base.
    assert_eq!(hover.padding, normal.padding);
    assert_eq!(hover.font_size, normal.font_size);

    // Active: darken.
    let active = table.get(WidgetKind::Button, WidgetState::ACTIVE);
    assert!(active.background.r < normal.background.r);

    // Disabled: alpha(color, 40%).
    let disabled = table.get(WidgetKind::Button, WidgetState::DISABLED);
    assert_eq!(disabled.foreground.a, (255.0_f32 * 0.4).round() as u8);
}

#[test]
fn prioridad_de_estado_en_la_consulta() {
    let table = compile(EXAMPLE_DSL, "dark").unwrap();
    // Con hover+disabled activos gana disabled (doc §6).
    let mixed = table.get(
        WidgetKind::Button,
        WidgetState::HOVER.union(WidgetState::DISABLED),
    );
    let disabled = table.get(WidgetKind::Button, WidgetState::DISABLED);
    assert_eq!(mixed.foreground, disabled.foreground);
}

#[test]
fn swap_de_tema_es_compilar_la_otra_tabla() {
    let dark = compile(EXAMPLE_DSL, "dark").unwrap();
    let light = compile(EXAMPLE_DSL, "light").unwrap();
    // Botón sin fondo definido en el DSL: cae al default, pero la tabla
    // lleva el nombre del tema.
    assert_eq!(dark.theme(), "dark");
    assert_eq!(light.theme(), "light");
    // Los tipos no estilados devuelven el default (mismo valor en ambos
    // temas: la diferencia vive donde el DSL declara variables).
    let d = dark.get(WidgetKind::Label, WidgetState::NORMAL);
    let l = light.get(WidgetKind::Label, WidgetState::NORMAL);
    assert_eq!(d.background, l.background, "defaults neutrales");
}

#[test]
fn variables_de_tema_en_propiedades() {
    const DSL: &str = r#"
widget window {
    background: base;
    color: text;
}
theme dark {
    base: #1e1e1e;
    text: #e0e0e0;
}
"#;
    let table = compile(DSL, "dark").unwrap();
    let window = table.get(WidgetKind::Window, WidgetState::NORMAL);
    assert_eq!(window.background, Color::rgb(0x1e, 0x1e, 0x1e));
    assert_eq!(window.foreground, Color::rgb(0xe0, 0xe0, 0xe0));
}

#[test]
fn mix_y_alpha_en_compilacion() {
    const DSL: &str = r#"
widget panel {
    background: mix(#000000, #ffffff, 50%);
    color: alpha(#ff8800, 25%);
}
theme dark { base: #101010; }
"#;
    let table = compile(DSL, "dark").unwrap();
    let panel = table.get(WidgetKind::Panel, WidgetState::NORMAL);
    assert_eq!(panel.background, Color::rgb(128, 128, 128));
    assert_eq!(panel.foreground, Color::rgba(255, 136, 0, 64));
}

#[test]
fn errores_utiles() {
    // Tema no declarado.
    assert!(matches!(
        compile(EXAMPLE_DSL, "nada"),
        Err(rivet_style::StyleError::UnknownTheme(_))
    ));
    // Widget desconocido.
    let bad = "widget teleport {} theme dark { base: #000; }";
    assert!(matches!(
        compile(bad, "dark"),
        Err(rivet_style::StyleError::UnknownWidget(_))
    ));
    // Estado desconocido.
    let bad =
        "widget button { color: #fff; state flying { color: #000; } } theme dark { base: #000; }";
    assert!(compile(bad, "dark").is_err());
    // Referencia sin resolver.
    let bad = "widget button { color: elsewhere; } theme dark { base: #000; }";
    assert!(matches!(
        compile(bad, "dark"),
        Err(rivet_style::StyleError::Unresolved(_))
    ));
    // Sintaxis.
    assert!(compile("widget button { color }", "dark").is_err());
}

#[test]
fn transiciones_interpola_para_el_motor() {
    let table = compile(EXAMPLE_DSL, "dark").unwrap();
    let normal = table.get(WidgetKind::Button, WidgetState::NORMAL);
    let hover = table.get(WidgetKind::Button, WidgetState::HOVER);
    let mid = normal.lerp(hover, 0.5);
    assert_eq!(
        mid.background,
        normal.background.mix(hover.background, 0.5),
        "lerp para el motor de animación de canvas"
    );
}
