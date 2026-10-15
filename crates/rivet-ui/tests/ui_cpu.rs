//! CPU tests of the UI core: tree, layout, hit-test, focus and dispatch
//! (no GL or Wayland; the TextRenderer only measures).

use rivet_ui::event::{EventDispatcher, UiInput};
use rivet_ui::focus::FocusManager;
use rivet_ui::hit::hit_test;
use rivet_ui::layout::layout_tree;
use rivet_ui::widget::state;
use rivet_ui::Ui;

fn ui_con_boton() -> (Ui, rivet_ui::WidgetId, rivet_ui::WidgetId) {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let label = ui.registry.create_label("Hola Rivet");
    let button = ui.registry.create_button("Salir", |ui| ui.quit());
    ui.registry.add_child(root, label).unwrap();
    ui.registry.add_child(root, button).unwrap();
    (ui, label, button)
}

#[test]
fn layout_apila_verticalmente() {
    let (mut ui, _label, button) = ui_con_boton();
    let root = ui.registry.root().unwrap();
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 400.0, 300.0).unwrap();
    ui.text = text;
    ui.style = style;
    // La raíz llena el espacio disponible.
    let root_rect = ui.registry.node(root).unwrap().rect;
    assert_eq!((root_rect.w, root_rect.h), (400.0, 300.0));
    // El botón está debajo de la etiqueta (apilado vertical).
    let button_rect = ui.registry.node(button).unwrap().rect;
    assert!(button_rect.y > 0.0);
    assert!(button_rect.w > 20.0, "botón con ancho por su texto");
}

#[test]
fn hit_y_estados_de_puntero() {
    let (mut ui, _label, button) = ui_con_boton();
    let root = ui.registry.root().unwrap();
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 400.0, 300.0).unwrap();
    ui.text = text;
    ui.style = style;

    let rect = ui.registry.node(button).unwrap().rect;
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    assert_eq!(hit_test(&ui.registry, root, cx, cy), Some(button));

    // Hover sobre el botón.
    EventDispatcher::dispatch(&mut ui, UiInput::PointerMove { x: cx, y: cy });
    assert!(ui.registry.node(button).unwrap().has_state(state::HOVER));

    // Clic completo dispara on_click (que pide quit).
    EventDispatcher::dispatch(&mut ui, UiInput::PointerDown { x: cx, y: cy });
    assert!(ui.registry.node(button).unwrap().has_state(state::ACTIVE));
    EventDispatcher::dispatch(&mut ui, UiInput::PointerUp { x: cx, y: cy });
    assert!(ui.quit_requested(), "el callback on_click ejecutó quit");
    assert!(!ui.registry.node(button).unwrap().has_state(state::ACTIVE));

    // Down en un sitio y up en otro: no es clic.
    let mut ui2 = Ui::new_dark();
    let root2 = ui2.registry.create_box(true);
    ui2.registry.set_root(root2).unwrap();
    let button2 = ui2.registry.create_button("X", |ui| ui.quit());
    ui2.registry.add_child(root2, button2).unwrap();
    EventDispatcher::dispatch(&mut ui2, UiInput::PointerDown { x: 5.0, y: 5.0 });
    EventDispatcher::dispatch(&mut ui2, UiInput::PointerUp { x: 350.0, y: 250.0 });
    assert!(!ui2.quit_requested(), "clic abortado no dispara");
}

#[test]
fn teclado_foco_y_text_input() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let input = ui.registry.create_text_input("escribe…");
    ui.registry.add_child(root, input).unwrap();

    // Foco por Tab.
    key(&mut ui, rivet_ui::event::keysym::TAB, "", false);
    assert_eq!(ui.focus.focused(&ui.registry), Some(input));

    // Texto llega al input con foco.
    key(&mut ui, 0x6f, "o", false);
    key(&mut ui, 0x6c, "l", false);
    key(&mut ui, 0x61, "a", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().text, "ola");
    assert_eq!(ui.registry.text_input_data(input).unwrap().cursor, 3);

    // Cursor: izquierda ×2, insertar en medio.
    key(&mut ui, rivet_ui::event::keysym::LEFT, "", false);
    key(&mut ui, rivet_ui::event::keysym::LEFT, "", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().cursor, 1);
    key(&mut ui, 0x21, "!", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().text, "o!la");

    // Delete forward borra tras el cursor.
    key(&mut ui, rivet_ui::event::keysym::DELETE, "", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().text, "o!a");

    // Home + Delete forward borra el primer carácter.
    key(&mut ui, rivet_ui::event::keysym::HOME, "", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().cursor, 0);
    key(&mut ui, rivet_ui::event::keysym::DELETE, "", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().text, "!a");
    // Backspace en el inicio no borra nada.
    key(&mut ui, rivet_ui::event::keysym::BACKSPACE, "", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().text, "!a");

    // End vuelve al final.
    key(&mut ui, rivet_ui::event::keysym::END, "", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().cursor, 2);

    // Retroceso (backspace) borra el último.
    key(&mut ui, rivet_ui::event::keysym::BACKSPACE, "", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().text, "!");

    // Caracteres multibyte: el cursor respeta fronteras UTF-8.
    ui.registry
        .text_input_data_mut(input)
        .unwrap()
        .set_text("ñx");
    key(&mut ui, rivet_ui::event::keysym::LEFT, "", false);
    key(&mut ui, rivet_ui::event::keysym::BACKSPACE, "", false);
    assert_eq!(ui.registry.text_input_data(input).unwrap().text, "x");
}

#[test]
fn on_change_recibe_el_texto() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let input = ui.registry.create_text_input("...");
    ui.registry.add_child(root, input).unwrap();
    // Conectar callback: cuenta las notificaciones.
    let cell = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let cell2 = cell.clone();
    if let Ok(data) = ui.registry.text_input_data_mut(input) {
        data.on_change = Some(Box::new(move |_text, _ui| cell2.set(cell2.get() + 1)));
    }
    key(&mut ui, rivet_ui::event::keysym::TAB, "", false);
    key(&mut ui, 0x7a, "z", false);
    assert_eq!(cell.get(), 1, "on_change notificado una vez");
    // Mover el cursor no notifica.
    key(&mut ui, rivet_ui::event::keysym::LEFT, "", false);
    assert_eq!(cell.get(), 1, "mover cursor no notifica");
}

#[test]
fn shift_tab_retrocede_y_escape_desenfoca() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(false);
    ui.registry.set_root(root).unwrap();
    let b1 = ui.registry.create_button("1", |_| {});
    let input = ui.registry.create_text_input("t");
    let b2 = ui.registry.create_button("2", |_| {});
    for child in [b1, input, b2] {
        ui.registry.add_child(root, child).unwrap();
    }
    key(&mut ui, rivet_ui::event::keysym::TAB, "", false);
    assert_eq!(ui.focus.focused(&ui.registry), Some(b1));
    key(&mut ui, rivet_ui::event::keysym::TAB, "", false);
    key(&mut ui, rivet_ui::event::keysym::TAB, "", false);
    assert_eq!(ui.focus.focused(&ui.registry), Some(b2));
    // Shift+Tab retrocede (envuelve al principio).
    key(&mut ui, rivet_ui::event::keysym::TAB, "", true);
    assert_eq!(ui.focus.focused(&ui.registry), Some(input));
    // Escape desenfoca.
    key(&mut ui, rivet_ui::event::keysym::ESCAPE, "", false);
    assert_eq!(ui.focus.focused(&ui.registry), None);
}

#[test]
fn enter_activa_boton_enfocado() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let button = ui.registry.create_button("Salir", |ui| ui.quit());
    ui.registry.add_child(root, button).unwrap();
    key(&mut ui, rivet_ui::event::keysym::TAB, "", false);
    assert_eq!(ui.focus.focused(&ui.registry), Some(button));
    key(&mut ui, rivet_ui::event::keysym::RETURN, "", false);
    assert!(ui.quit_requested(), "Enter activó el botón con foco");
}

#[test]
fn handler_global_consume_atajos() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let input = ui.registry.create_text_input("t");
    ui.registry.add_child(root, input).unwrap();
    // Ctrl+S pide salir y consume; no llega al TextInput.
    ui.set_key_handler(|key, ui| {
        if key.modifiers.ctrl && key.keysym == 0x73 {
            ui.quit();
            true
        } else {
            false
        }
    });
    key_ctrl(&mut ui, 0x73, "s");
    assert!(ui.quit_requested());
    assert_eq!(
        ui.registry.text_input_data(input).unwrap().text,
        "",
        "el atajo no insertó texto"
    );
}

#[test]
fn clic_posiciona_cursor_del_text_input() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let input = ui.registry.create_text_input("...");
    ui.registry.add_child(root, input).unwrap();
    ui.registry
        .text_input_data_mut(input)
        .unwrap()
        .set_text("hola");
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 400.0, 300.0).unwrap();
    ui.text = text;
    ui.style = style;
    let rect = ui.registry.node(input).unwrap().rect;
    // Clic sobre la mitad derecha del texto: cursor al final.
    EventDispatcher::dispatch(
        &mut ui,
        UiInput::PointerDown {
            x: rect.x + rect.w - 1.0,
            y: rect.y + rect.h / 2.0,
        },
    );
    assert_eq!(
        ui.registry.text_input_data(input).unwrap().cursor,
        4,
        "clic al final posiciona el cursor al final"
    );
    // Clic en el extremo izquierdo (sin padding): cursor al inicio.
    EventDispatcher::dispatch(
        &mut ui,
        UiInput::PointerDown {
            x: rect.x,
            y: rect.y + rect.h / 2.0,
        },
    );
    assert_eq!(ui.registry.text_input_data(input).unwrap().cursor, 0);
}

/// Despacha una tecla con modificadores opcionales.
fn key(ui: &mut Ui, keysym: u32, text: &str, shift: bool) {
    key_mods(ui, keysym, text, false, shift);
}

/// Despacha una tecla con Ctrl.
fn key_ctrl(ui: &mut Ui, keysym: u32, text: &str) {
    key_mods(ui, keysym, text, true, false);
}

fn key_mods(ui: &mut Ui, keysym: u32, text: &str, ctrl: bool, shift: bool) {
    EventDispatcher::dispatch(
        ui,
        UiInput::Key {
            keysym,
            text: text.into(),
            modifiers: rivet_ui::Modifiers {
                ctrl,
                shift,
                alt: false,
                logo: false,
            },
            pressed: true,
        },
    );
}

#[test]
fn dirty_flags_dirigen_el_repintado() {
    let (mut ui, _label, button) = ui_con_boton();
    let root = ui.registry.root().unwrap();
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 400.0, 300.0).unwrap();
    ui.text = text;
    ui.style = style;
    // Creación deja el árbol sucio (frames iniciales).
    assert!(ui.is_dirty(), "widgets recién creados: sucios");
    ui.clear_dirty();
    assert!(!ui.is_dirty(), "limpio tras clear");

    // Un move marca el hover inicial del contenedor raíz (cambio real).
    EventDispatcher::dispatch(&mut ui, UiInput::PointerMove { x: 1.0, y: 1.0 });
    assert!(ui.is_dirty(), "hover inicial del raíz: cambio de estado");
    ui.clear_dirty();

    // Un move posterior sin cambio de hover no ensucia (ambos en el
    // padding del raíz: mismo objetivo).
    EventDispatcher::dispatch(&mut ui, UiInput::PointerMove { x: 2.0, y: 2.0 });
    assert!(!ui.is_dirty(), "hover sin cambio no ensucia");

    // Hover sobre el botón sí (cambio de estado).
    let rect = ui.registry.node(button).unwrap().rect;
    EventDispatcher::dispatch(
        &mut ui,
        UiInput::PointerMove {
            x: rect.x + rect.w / 2.0,
            y: rect.y + rect.h / 2.0,
        },
    );
    assert!(ui.is_dirty(), "hover nuevo ensucia");
    ui.clear_dirty();
    // Segundo move sobre el mismo botón: sin cambio.
    EventDispatcher::dispatch(
        &mut ui,
        UiInput::PointerMove {
            x: rect.x + rect.w / 2.0 + 1.0,
            y: rect.y + rect.h / 2.0,
        },
    );
    assert!(!ui.is_dirty(), "hover repetido no ensucia");

    // invalidate() ensucia todo.
    ui.invalidate(None);
    assert!(ui.is_dirty());
}

#[test]
fn measure_cache_produce_rects_estables() {
    // El caché de v0.2 debe dar el mismo layout que la doble pasada.
    let (mut ui, _label, button) = ui_con_boton();
    let root = ui.registry.root().unwrap();
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 400.0, 300.0).unwrap();
    ui.text = text;
    ui.style = style;
    let root_rect = ui.registry.node(root).unwrap().rect;
    assert_eq!((root_rect.w, root_rect.h), (400.0, 300.0));
    let button_rect = ui.registry.node(button).unwrap().rect;
    assert!(button_rect.y > 0.0);
    assert!(button_rect.w > 20.0, "ancho por el texto medido");
    // Dos pasadas seguidas: deterministas.
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 400.0, 300.0).unwrap();
    ui.text = text;
    ui.style = style;
    let again = ui.registry.node(button).unwrap().rect;
    assert_eq!(button_rect, again, "layout estable entre pasadas");
}

#[test]
fn foco_avanza_entre_interactivos() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(false);
    ui.registry.set_root(root).unwrap();
    let b1 = ui.registry.create_button("1", |_| {});
    let input = ui.registry.create_text_input("t");
    let b2 = ui.registry.create_button("2", |_| {});
    for child in [b1, input, b2] {
        ui.registry.add_child(root, child).unwrap();
    }
    let mut focus = FocusManager::default();
    focus.tab_forward(&mut ui.registry).unwrap();
    assert_eq!(focus.focused(&ui.registry), Some(b1));
    focus.tab_forward(&mut ui.registry).unwrap();
    assert_eq!(focus.focused(&ui.registry), Some(input));
    focus.tab_forward(&mut ui.registry).unwrap();
    assert_eq!(focus.focused(&ui.registry), Some(b2));
    // Cíclico.
    focus.tab_forward(&mut ui.registry).unwrap();
    assert_eq!(focus.focused(&ui.registry), Some(b1));
}

#[test]
fn set_text_actualiza_sin_recrear() {
    let (mut ui, label, button) = ui_con_boton();
    ui.clear_dirty();
    ui.registry.set_label_text(label, "Texto nuevo").unwrap();
    assert_eq!(ui.registry.label_data(label).unwrap().text, "Texto nuevo");
    assert!(ui.is_dirty(), "set_text marca suciedad");
    ui.clear_dirty();
    ui.registry.set_button_text(button, "Guardar").unwrap();
    assert_eq!(ui.registry.button_data(button).unwrap().text, "Guardar");
    assert!(ui.is_dirty());
}

#[test]
fn estado_tipado_en_ui() {
    #[derive(Default)]
    struct Nota {
        contenido: String,
    }
    let mut ui = Ui::new_dark();
    assert!(ui.state::<Nota>().is_none(), "sin estado aún");
    ui.insert_state(Nota::default());
    ui.state_mut::<Nota>().unwrap().contenido.push_str("hola");
    assert_eq!(ui.state::<Nota>().unwrap().contenido, "hola");
    // Reemplazo devuelve el valor anterior.
    let viejo = ui.insert_state(Nota {
        contenido: "nueva".into(),
    });
    assert_eq!(viejo.unwrap().contenido, "hola");
    assert_eq!(ui.state::<Nota>().unwrap().contenido, "nueva");
}

#[test]
fn foco_programatico() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let b1 = ui.registry.create_button("1", |_| {});
    let input = ui.registry.create_text_input("t");
    let b2 = ui.registry.create_button("2", |_| {});
    for child in [b1, input, b2] {
        ui.registry.add_child(root, child).unwrap();
    }
    ui.focus_next();
    assert_eq!(ui.focus.focused(&ui.registry), Some(b1));
    ui.focus_next();
    assert_eq!(ui.focus.focused(&ui.registry), Some(input));
    ui.focus_previous();
    assert_eq!(ui.focus.focused(&ui.registry), Some(b1));
    // focus_widget directo sobre el contenedor: primer interactivo dentro.
    ui.focus_widget(b2);
    assert_eq!(ui.focus.focused(&ui.registry), Some(b2));
    assert!(
        ui.registry.node(b2).unwrap().has_state(state::FOCUSED),
        "bit de foco activo"
    );
}

#[test]
fn fill_estira_hijos_en_el_contenedor() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let label = ui.registry.create_label("título");
    let input = ui.registry.create_text_input("...");
    ui.registry.set_fill(input, true, true).unwrap();
    ui.registry.add_child(root, label).unwrap();
    ui.registry.add_child(root, input).unwrap();
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 400.0, 300.0).unwrap();
    ui.text = text;
    ui.style = style;
    let input_rect = ui.registry.node(input).unwrap().rect;
    let root_rect = ui.registry.node(root).unwrap().rect;
    assert!(
        input_rect.h > 200.0,
        "fill vertical estira (h={})",
        input_rect.h
    );
    // fill horizontal: al ancho interno del contenedor.
    assert!(
        (input_rect.x + input_rect.w - (root_rect.x + root_rect.w)).abs() < 20.0,
        "el input llega al borde derecho"
    );
    // Sin fill, el label conserva su tamaño preferido.
    let label_rect = ui.registry.node(label).unwrap().rect;
    assert!(label_rect.h < 60.0, "label sin fill: tamaño preferido");
}

#[test]
fn border_de_panel_se_descuenta_del_area_de_hijos() {
    let dsl = "widget panel { border-width: 4; }\ntheme neutral {}";
    let mut ui = Ui::new(dsl, "neutral");
    let root = ui.registry.create_panel(true);
    ui.registry.set_root(root).unwrap();
    let label = ui.registry.create_label("x");
    ui.registry.add_child(root, label).unwrap();
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 200.0, 100.0).unwrap();
    ui.text = text;
    ui.style = style;
    let label_rect = ui.registry.node(label).unwrap().rect;
    // padding del panel (10) + borde (4).
    assert!((label_rect.x - 14.0).abs() < 0.01, "x={}", label_rect.x);
    assert!((label_rect.y - 14.0).abs() < 0.01, "y={}", label_rect.y);
}

#[test]
fn text_area_multilinea() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let area = ui.registry.create_text_area("escribe…");
    ui.registry.add_child(root, area).unwrap();
    assert!(
        ui.registry.text_input_data(area).unwrap().multiline,
        "área multilínea"
    );

    key(&mut ui, rivet_ui::event::keysym::TAB, "", false);
    // "ab", Enter, "cd" → dos líneas.
    key(&mut ui, 0x61, "a", false);
    key(&mut ui, 0x62, "b", false);
    key(&mut ui, rivet_ui::event::keysym::RETURN, "", false);
    key(&mut ui, 0x63, "c", false);
    key(&mut ui, 0x64, "d", false);
    assert_eq!(ui.registry.text_input_data(area).unwrap().text, "ab\ncd");
    assert_eq!(
        ui.registry.text_input_data(area).unwrap().cursor,
        5,
        "cursor al final"
    );

    // Up conserva la columna en la línea anterior.
    key(&mut ui, rivet_ui::event::keysym::UP, "", false);
    assert_eq!(ui.registry.text_input_data(area).unwrap().cursor, 2);
    // Down vuelve (columna clamp al largo de la línea).
    key(&mut ui, rivet_ui::event::keysym::DOWN, "", false);
    assert_eq!(ui.registry.text_input_data(area).unwrap().cursor, 5);

    // Backspace al inicio de la segunda línea une las líneas.
    key(&mut ui, rivet_ui::event::keysym::UP, "", false);
    key(&mut ui, rivet_ui::event::keysym::RIGHT, "", false); // cruza el \n
    assert_eq!(ui.registry.text_input_data(area).unwrap().cursor, 3);
    key(&mut ui, rivet_ui::event::keysym::BACKSPACE, "", false);
    assert_eq!(ui.registry.text_input_data(area).unwrap().text, "abcd");

    // En una sola línea Enter no inserta nada.
    let single = ui.registry.create_text_input("x");
    ui.registry.add_child(root, single).unwrap();
    ui.focus_widget(single);
    key(&mut ui, rivet_ui::event::keysym::RETURN, "", false);
    assert_eq!(ui.registry.text_input_data(single).unwrap().text, "");
}

#[test]
fn text_area_mide_por_lineas() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let area = ui.registry.create_text_area("");
    ui.registry.add_child(root, area).unwrap();
    ui.registry
        .text_input_data_mut(area)
        .unwrap()
        .set_text("uno\ndos\ntres");
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 200.0, 400.0).unwrap();
    ui.text = text;
    ui.style = style;
    let rect = ui.registry.node(area).unwrap().rect;
    // Tres líneas apiladas: sensiblemente más alto que una.
    assert!(rect.h > 30.0, "alto por líneas (h={})", rect.h);
}

#[test]
fn scroll_de_contenedor_con_rueda() {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let scroll_box = ui.registry.create_box(true);
    ui.registry.set_scrollable(scroll_box, true).unwrap();
    ui.registry.set_fill(scroll_box, true, true).unwrap();
    ui.registry.add_child(root, scroll_box).unwrap();
    // Contenido más alto que el viewport.
    for i in 0..20 {
        let label = ui.registry.create_label(format!("línea {i}"));
        ui.registry.add_child(scroll_box, label).unwrap();
    }
    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(&mut ui.registry, &mut text, &style, root, 200.0, 150.0).unwrap();
    ui.text = text;
    ui.style = style;
    let (content_h, viewport_h) = ui.registry.scroll_metrics(scroll_box).unwrap();
    assert!(
        content_h > viewport_h,
        "el contenido desborda el viewport ({content_h} > {viewport_h})"
    );
    assert_eq!(ui.registry.scroll_offset(scroll_box).unwrap(), 0.0);

    // Rueda sobre el área del scroll: desplaza.
    let rect = ui.registry.node(scroll_box).unwrap().rect;
    EventDispatcher::dispatch(
        &mut ui,
        UiInput::Scroll {
            x: rect.x + 5.0,
            y: rect.y + 5.0,
            dx: 0.0,
            dy: 1.0,
        },
    );
    let after = ui.registry.scroll_offset(scroll_box).unwrap();
    assert!(after > 0.0, "la rueda desplazó ({after})");
    assert!(ui.is_dirty(), "el scroll marca suciedad");

    // Clamp: muchas ruedas no pasan del final del contenido.
    for _ in 0..100 {
        EventDispatcher::dispatch(
            &mut ui,
            UiInput::Scroll {
                x: rect.x + 5.0,
                y: rect.y + 5.0,
                dx: 0.0,
                dy: 1.0,
            },
        );
    }
    let max = ui.registry.scroll_offset(scroll_box).unwrap();
    for _ in 0..10 {
        EventDispatcher::dispatch(
            &mut ui,
            UiInput::Scroll {
                x: rect.x + 5.0,
                y: rect.y + 5.0,
                dx: 0.0,
                dy: 1.0,
            },
        );
    }
    assert_eq!(ui.registry.scroll_offset(scroll_box).unwrap(), max, "clamp");
}
