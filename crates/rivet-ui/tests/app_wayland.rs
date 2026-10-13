//! Smoke test de `Application` contra un compositor Wayland real.
//!
//! Abre una ventana, espera configure, presenta un frame con UI completa y
//! cierra. Se salta sin `$WAYLAND_DISPLAY`.

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

    // Y el bucle completo termina solo: quit por el estado interno.
    app.ui_mut().quit();
    app.run().unwrap();
}
