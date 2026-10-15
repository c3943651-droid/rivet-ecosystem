//! Busca el "halo/sombra paralela" en las cadenas del notepad.
fn main() {
    let mut app = rivet_ui::Application::new("halo", 260, 160).unwrap();
    {
        let ui = app.ui_mut();
        let root = ui.registry.create_panel(true);
        ui.registry.set_root(root).unwrap();
        let b1 = ui.registry.create_button("Eliminar nota", |ui| ui.quit());
        let b2 = ui.registry.create_button("+ Nueva nota", |ui| ui.quit());
        let label = ui.registry.create_label("Sin nota seleccionada");
        for c in [b1, b2, label] {
            ui.registry.add_child(root, c).unwrap();
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !app.configured() && std::time::Instant::now() < deadline {
        app.pump_events().unwrap();
    }
    app.present_frame().unwrap();
    let (bw, _bh) = app.buffer_size_of(0);
    let px = app.presented_frame_rgba(0);
    let ui = app.ui();
    let root = ui.registry.root().unwrap();
    let label = ui.registry.children(root).nth(2).unwrap();
    let rect = ui.registry.node(label).unwrap().rect;
    println!("label rect {rect:?}  (texto #d7dce4 sobre panel #16181d)");
    // Zoom de las primeras 3 letras "Sin" con 2px de margen.
    for y in (rect.y as u32).saturating_sub(2)..(rect.y as u32 + rect.h as u32 + 2) {
        let mut row = String::new();
        for x in (rect.x as u32).saturating_sub(2)..(rect.x as u32 + 30) {
            let i = ((y * bw + x) * 4) as usize;
            let (r, g, b) = (px[i], px[i + 1], px[i + 2]);
            // brillo 0-9
            let lum = (u32::from(r) * 3 + u32::from(g) * 6 + u32::from(b)) / 10;
            row.push(char::from(b'0' + (lum * 10 / 256) as u8));
        }
        println!("{y:3} {row}");
    }
    // y el botón (texto claro sobre azul)
    let b1 = ui.registry.children(root).next().unwrap();
    let brect = ui.registry.node(b1).unwrap().rect;
    println!("botón rect {brect:?}");
    for y in brect.y as u32..(brect.y as u32 + brect.h as u32).min(brect.y as u32 + 14) {
        let mut row = String::new();
        for x in brect.x as u32..(brect.x as u32 + 30).min(brect.x as u32 + brect.w as u32) {
            let i = ((y * bw + x) * 4) as usize;
            let (r, g, b) = (px[i], px[i + 1], px[i + 2]);
            let lum = (u32::from(r) * 3 + u32::from(g) * 6 + u32::from(b)) / 10;
            row.push(char::from(b'0' + (lum * 10 / 256) as u8));
        }
        println!("{y:3} {row}");
    }
}
