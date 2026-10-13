//! Tests CPU del núcleo de UI: árbol, layout, hit-test, foco y despacho
//! (sin GL ni Wayland; el TextRenderer solo mide).

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
    EventDispatcher::dispatch(&mut ui, UiInput::Tab);
    assert_eq!(ui.focus.focused(&ui.registry), Some(input));

    // Texto llega al input con foco.
    EventDispatcher::dispatch(
        &mut ui,
        UiInput::KeyText {
            text: "ol".into(),
            keysym: 0x6f,
            pressed: true,
        },
    );
    EventDispatcher::dispatch(
        &mut ui,
        UiInput::KeyText {
            text: "a".into(),
            keysym: 0x61,
            pressed: true,
        },
    );
    assert_eq!(ui.registry.text_input_data(input).unwrap().text, "ola");

    // Backspace borra.
    EventDispatcher::dispatch(
        &mut ui,
        UiInput::KeyText {
            text: String::new(),
            keysym: 0xFF08,
            pressed: true,
        },
    );
    assert_eq!(ui.registry.text_input_data(input).unwrap().text, "ol");
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
    EventDispatcher::dispatch(&mut ui, UiInput::Tab);
    EventDispatcher::dispatch(
        &mut ui,
        UiInput::KeyText {
            text: "z".into(),
            keysym: 0x7a,
            pressed: true,
        },
    );
    assert_eq!(cell.get(), 1, "on_change notificado una vez");
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
