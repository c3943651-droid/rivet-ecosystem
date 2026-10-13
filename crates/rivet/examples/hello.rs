//! App mínima con la fachada: ventana con panel, texto y botón Salir.
//!
//! Ejecutar en sesión Wayland: `cargo run -p rivet --example hello`.

use rivet::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Application::new("Hola Rivet", 360, 200)?;

    {
        let ui = app.ui_mut();
        let root = ui.registry.create_panel(true);
        ui.registry.set_root(root)?;

        let title = ui.registry.create_label("Hola, Rivet ✨");
        let close = ui.registry.create_button("Salir", |ui| ui.quit());

        ui.registry.add_child(root, title)?;
        ui.registry.add_child(root, close)?;
    }

    app.run()?;
    Ok(())
}
