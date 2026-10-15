//! Smoke test of `Application` against a real Wayland compositor.
//!
//! Opens a window, waits for configure, presents a frame with a full UI and
//! closes. Skipped without `$WAYLAND_DISPLAY`.

use rivet_ui::{Application, Ui};

#[test]
fn application_presenta_un_frame_completo() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("skip: sin $WAYLAND_DISPLAY");
        return;
    }
    let mut app = Application::new("rivet-ui smoke", 220, 120).unwrap();

    // UI mínima: label + botón que pide quit.
    {
        let ui: &mut Ui = app.ui_mut();
        let root = ui.registry.create_panel(true);
        ui.registry.set_root(root).unwrap();
        let label = ui.registry.create_label("Hola Rivet");
        let button = ui.registry.create_button("Cerrar", |ui| ui.quit());
        ui.registry.add_child(root, label).unwrap();
        ui.registry.add_child(root, button).unwrap();
    }

    // Bombea hasta configure (con tope: el compositor debe responder).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app.configured() && std::time::Instant::now() < deadline {
        app.pump_events().unwrap();
    }
    assert!(app.configured(), "el compositor configuró la ventana");

    // Presenta un frame real (layout + render + GL + wl_shm).
    app.present_frame().unwrap();

    // El frame presentado dejó el árbol limpio: sin suciedad, pump no
    // repinta (render dirigido por dirty flags, v0.2).
    assert!(!app.ui_mut().is_dirty(), "árbol limpio tras presentar");

    // Y el bucle completo termina solo: quit por el estado interno.
    app.ui_mut().quit();
    app.run().unwrap();
}

#[test]
fn schedule_redraw_after_despierta_el_bucle() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("skip: sin $WAYLAND_DISPLAY");
        return;
    }
    let mut app = Application::new("rivet-ui timer", 120, 80).unwrap();
    // Drena el configure inicial (eventos pendientes de la ventana nueva).
    let configure_by = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app.configured() && std::time::Instant::now() < configure_by {
        app.pump_events().unwrap();
    }
    assert!(app.configured(), "el compositor configuró la ventana");
    // Sin eventos del compositor, pump vuelve en ~50 ms por el deadline.
    app.schedule_redraw_after(std::time::Duration::from_millis(50));
    let start = std::time::Instant::now();
    app.pump_events().unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed >= std::time::Duration::from_millis(40),
        "esperó el deadline (tardó {elapsed:?})"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "no se colgó esperando eventos"
    );
}

#[test]
fn multiventana_crea_presenta_cierra_y_sigue() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("skip: sin $WAYLAND_DISPLAY");
        return;
    }
    use rivet_window::WindowOptions;

    let mut app = Application::new("rivet-ui mw1", 140, 90).unwrap();
    assert_eq!(app.window_count(), 1);
    // UI mínima en la primaria.
    {
        let ui = app.ui_mut();
        let root = ui.registry.create_panel(true);
        ui.registry.set_root(root).unwrap();
        let label = ui.registry.create_label("Principal");
        ui.registry.add_child(root, label).unwrap();
    }

    let second = app
        .create_window(WindowOptions::new(120, 80).title("rivet-ui mw2"))
        .unwrap();
    assert_eq!(app.window_count(), 2);
    {
        let ui = app.ui_mut_of(second);
        let root = ui.registry.create_panel(true);
        ui.registry.set_root(root).unwrap();
        let label = ui.registry.create_label("Secundaria");
        ui.registry.add_child(root, label).unwrap();
    }

    // Ambas ventanas reciben configure.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !(app.configured() && app.configured_of(second)) && std::time::Instant::now() < deadline {
        app.pump_events().unwrap();
    }
    assert!(app.configured(), "primaria configurada");
    assert!(app.configured_of(second), "secundaria configurada");

    // Presentar frames independientes.
    app.present_frame().unwrap();
    app.present_frame_of(second).unwrap();

    // Cerrar la secundaria: la app sigue con la primaria.
    app.close_window(second).unwrap();
    assert_eq!(app.window_count(), 1);
    app.pump_events().unwrap();

    // Terminar limpio.
    app.ui_mut().quit();
    app.run().unwrap();
}

#[test]
fn frame_cubre_toda_la_ventana_y_espera_el_fondo() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("skip: sin $WAYLAND_DISPLAY");
        return;
    }
    use rivet_style::{WidgetKind as StyleKind, WidgetState as StyleState};

    // Tema con fondo de panel conocido para muestrear píxeles.
    let dsl = "widget panel { background: #2e2e2e; }\nwidget label { color: #ffffff; font-size: 16; }\ntheme neutral {}";
    let mut app = Application::new("rivet-ui pixels", 180, 140).unwrap();
    app.set_theme(dsl, "neutral");
    {
        let ui = app.ui_mut();
        let root = ui.registry.create_panel(true);
        ui.registry.set_root(root).unwrap();
        let label = ui.registry.create_label("Rivet");
        let input = ui.registry.create_text_input("escribe");
        ui.registry.add_child(root, label).unwrap();
        ui.registry.add_child(root, input).unwrap();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app.configured() && std::time::Instant::now() < deadline {
        app.pump_events().unwrap();
    }
    assert!(app.configured());
    app.present_frame().unwrap();

    let (bw, bh) = app.buffer_size_of(0);
    assert!(
        bw >= 100 && bh >= 80,
        "el compositor no encogió la ventana ({bw}×{bh})"
    );
    let px = app.presented_frame_rgba(0);
    assert_eq!(px.len(), bw as usize * bh as usize * 4);

    let expected = rivet_style::compile(dsl, "neutral")
        .unwrap()
        .get(StyleKind::Panel, StyleState::NORMAL)
        .background;
    let near = |x: u32, y: u32| {
        let i = (y * bw + x) as usize * 4;
        let (r, g, b, a) = (px[i], px[i + 1], px[i + 2], px[i + 3]);
        assert!(
            (i32::from(r) - i32::from(expected.r)).abs() <= 10
                && (i32::from(g) - i32::from(expected.g)).abs() <= 10
                && (i32::from(b) - i32::from(expected.b)).abs() <= 10
                && a > 200,
            "píxel ({x},{y}) = rgba({r},{g},{b},{a}), esperaba ~{expected:?}"
        );
    };
    // Regresión v0.3: sin `gl.resize` el viewport quedaba a 64×64 y las
    // esquinas más allá renderizaban basura. Muestrear las cuatro
    // esquinas y el centro: todo debe ser el fondo del panel.
    near(2, 2);
    near(bw - 3, 2);
    near(2, bh - 3);
    near(bw - 3, bh - 3);
    near(bw / 2, bh / 2);

    // El TEXTO también se dibuja: píxeles no-fondo dentro del rect del
    // label (captura regresiones del atlas/glifos: "letras derretidas").
    let (label_rect, scale) = {
        let ui = app.ui();
        let root = ui.registry.root().unwrap();
        (
            ui.registry.node(root).unwrap().rect,
            app.buffer_size_of(0).0 as f32 / ui.registry.node(root).unwrap().rect.w.max(1.0),
        )
    };
    let _ = scale;
    // El label es el primer hijo del panel raíz.
    let label = app
        .ui()
        .registry
        .children(app.ui().registry.root().unwrap())
        .next()
        .unwrap();
    let rect = app.ui().registry.node(label).unwrap().rect;
    let mut text_pixels = 0usize;
    let x0 = rect.x as u32 + 1;
    let y0 = rect.y as u32 + 1;
    for y in y0..(y0 + rect.h as u32 - 1) {
        for x in x0..(x0 + rect.w as u32 - 1) {
            if (x >= bw) || (y >= bh) {
                continue;
            }
            let i = (y * bw + x) as usize * 4;
            let (r, g, b) = (px[i], px[i + 1], px[i + 2]);
            let far = (i32::from(r) - i32::from(expected.r)).abs() > 40
                || (i32::from(g) - i32::from(expected.g)).abs() > 40
                || (i32::from(b) - i32::from(expected.b)).abs() > 40;
            if far {
                text_pixels += 1;
            }
        }
    }
    assert!(
        text_pixels > 20,
        "el label dejó traza visible ({text_pixels} px de texto)"
    );
    let _ = label_rect;
}
