//! Minimal application using the facade: a window with a panel, text and a Quit button.
//!
//! Run in a Wayland session: `cargo run -p rivet --example hello`.

use rivet::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Application::new("Hello, Rivet", 360, 200)?;

    {
        let ui = app.ui_mut();
        let root = ui.registry.create_panel(true);
        ui.registry.set_root(root)?;

        let title = ui.registry.create_label("Hello, Rivet");
        let close = ui.registry.create_button("Quit", |ui| ui.quit());

        ui.registry.add_child(root, title)?;
        ui.registry.add_child(root, close)?;
    }

    app.run()?;
    Ok(())
}
